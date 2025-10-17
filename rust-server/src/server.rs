use crate::{filesystem::save_json, ADDR, CONFIG_PATH};
use crate::relayboard::Board;
use axum::{
    body::Body,
    extract::{Json, Query, State},
    http::{Response, StatusCode},
    response::IntoResponse,
    routing::{get, post},
    Router
};
use tokio::{spawn, sync::{broadcast, Mutex}, time::{sleep, Duration}};
use std::{collections::HashSet, net::Ipv4Addr, fs, path::PathBuf, sync::Arc, time};
use tower_http::services::ServeDir;
use serde::{Serialize, Deserialize};

#[derive(Debug, Deserialize)]
pub struct InitialQueryParams {
    pub initial_event: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Item {
    pub id: u32,
    pub name: String,
    pub ipv4: String,
    pub last_updated: time::SystemTime,
    pub state: bool
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct BoardDetails {
    pub device: String,
    pub ip: String
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct States {
    pub relays: Vec<Item>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerEvent {
    updated_item: Item,
}

pub struct AppState {
    pub json_data: Arc<Mutex<States>>,
    pub tx: broadcast::Sender<ServerEvent>,
    pub board: Mutex<Option<Arc<Board>>>
}

pub async fn launch_server(data: States) {
    let state = Arc::new(Mutex::new(data.clone()));
    let (tx, _rx) = broadcast::channel::<ServerEvent>(16);
    let board = Mutex::new(Some(Arc::new(Board::new(Board::find_ip().await).await)));

    {
        let board_guard = board.lock().await;
        if let Some(b) = board_guard.as_ref() {
            let _ = b.set_relay(&data).await;
        }
    }

    let app_state = Arc::new(AppState { json_data: state, tx, board });

    let app = Router::new()
        .route("/", get(serve_index))
        .fallback_service(ServeDir::new("public"))
        .route("/data", get(serve_data_handler).post(receive_data_handler))
        .route("/register", post(register_board))
        .with_state(app_state);

    let listener = tokio::net::TcpListener::bind(ADDR).await.unwrap();
    println!("Server running on: {}", listener.local_addr().unwrap());
    axum::serve(listener, app).await.unwrap();
}

async fn serve_index() -> impl IntoResponse {
    let path = PathBuf::from("public/index.html");
    match fs::read(&path) {
        Ok(contents) => Response::builder()
            .header("Content-Type", "text/html")
            .body(Body::from(contents))
            .unwrap(),
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "Failed to load index.html").into_response(),
    }
}

async fn serve_data_handler(
    State(state): State<Arc<AppState>>,
    Query(query_params): Query<InitialQueryParams>
) -> impl IntoResponse {
    if query_params.initial_event.is_some() {
        let data = state.json_data.lock().await;
        return (StatusCode::OK, Json(data.clone())).into_response();
    }

    let mut rx = state.tx.subscribe();
    let poll_timeout = Duration::from_secs(60);

    let event_future = async move {
        match rx.recv().await {
            Ok(event) => Ok(event),
            Err(broadcast::error::RecvError::Lagged(_)) => Err(StatusCode::RESET_CONTENT),
            Err(broadcast::error::RecvError::Closed) => Err(StatusCode::INTERNAL_SERVER_ERROR),
        }
    };

    match tokio::time::timeout(poll_timeout, event_future).await {
        Ok(Ok(event)) => (StatusCode::OK, Json(event)).into_response(),
        Ok(Err(status)) => status.into_response(),
        Err(_) => StatusCode::NO_CONTENT.into_response(),
    }
}

async fn receive_data_handler(
    State(state): State<Arc<AppState>>,
    Json(updated_item): Json<Item>
) -> impl IntoResponse {
    let mut data = state.json_data.lock().await;
    let mut updated_item_for_event = None;

    for item in &mut data.relays {
        if item.id == updated_item.id {
            if let Ok(duration) = time::SystemTime::now().duration_since(item.last_updated) {
                if duration > Duration::from_secs(2) {
                    item.name = if updated_item.state { updated_item.name.clone() } else { "-".to_string() };
                    item.ipv4 = if updated_item.state { updated_item.ipv4.clone() } else { "-.-.-.-".to_string() };
                    item.state = updated_item.state;
                    item.last_updated = time::SystemTime::now();
                    updated_item_for_event = Some(item.clone());
                }
            }
        }
    }

    // Save JSON asynchronously to prevent blocking the main runtime
    let cloned_data = data.clone();
    drop(data);

    tokio::task::spawn_blocking(move || {
        if let Err(e) = save_json(&cloned_data, CONFIG_PATH) {
            eprintln!("Error saving data to file: {}", e);
        }
    });

    // Send update event
    if let Some(updated) = updated_item_for_event.clone() {
        let _ = state.tx.send(ServerEvent { updated_item: updated });
    }

    // Handle board update without blocking
    if let Some(board_ref) = state.board.lock().await.as_ref().cloned() {
        let state_clone = Arc::clone(&state);
        spawn(async move {
            let data = state_clone.json_data.lock().await;
            let _ = board_ref.set_relay(&data).await;
        });
    }

    // Only trigger scan task for active items
    if updated_item.state {
        spawn(scan_and_update_new_devices(Arc::clone(&state), updated_item));
    }

    StatusCode::OK
}

async fn scan_and_update_new_devices(state: Arc<AppState>, updated_item: Item) {
    // Independent scanning task (no locks held across .await)
    let board_ref = {
        let board_guard = state.board.lock().await;
        board_guard.as_ref().cloned()
    };

    if let Some(board_ref) = board_ref {
        let first_scan: HashSet<Ipv4Addr> = board_ref.get_panels().await.into_iter().collect();
        sleep(Duration::from_secs(45)).await;
        let second_scan: HashSet<Ipv4Addr> = board_ref.get_panels().await.into_iter().collect();
        let new_devices: Vec<Ipv4Addr> = second_scan.difference(&first_scan).cloned().collect();

        if let Some(ip) = new_devices.first() {
            let mut data = state.json_data.lock().await;
            for item in &mut data.relays {
                if item.id == updated_item.id {
                    item.ipv4 = ip.to_string();
                }
            }
            let cloned_data = data.clone();
            let event_item = data.relays.iter().find(|i| i.id == updated_item.id).cloned();
            drop(data);

            tokio::task::spawn_blocking(move || {
                let _ = save_json(&cloned_data, CONFIG_PATH);
            });

            if let Some(updated) = event_item {
                let _ = state.tx.send(ServerEvent { updated_item: updated });
            }
        }
    }
}

pub async fn register_board(
    State(state): State<Arc<AppState>>,
    Json(details): Json<BoardDetails>
) -> impl IntoResponse {
    if details.device != "relayBoard" {
        return (StatusCode::BAD_REQUEST, "Invalid device type").into_response();
    }

    let mut board_lock = state.board.lock().await;
    if board_lock.is_some() {
        return (StatusCode::CONFLICT, "Board already registered").into_response();
    }

    let ip = match details.ip.parse::<Ipv4Addr>() {
        Ok(ip) => ip,
        Err(_) => return (StatusCode::BAD_REQUEST, "Invalid IP format").into_response(),
    };

    let new_board = Arc::new(Board::new(ip).await);
    *board_lock = Some(new_board.clone());

    let data = state.json_data.lock().await.clone();
    drop(board_lock);

    spawn(async move {
        let _ = new_board.set_relay(&data).await;
    });

    (StatusCode::OK, format!("Board registered successfully with IP: {}", details.ip)).into_response()
}
