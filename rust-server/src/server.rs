use crate::{filesystem::save_json, ADDR, CONFIG_PATH};
use crate::relayboard::Board;
use axum::{
    body::Body, extract::{Json, Query, State}, http::{Response, StatusCode}, response::IntoResponse, routing::{get, post}, Router
};
use tokio::spawn;
use tokio::sync::{broadcast, Mutex};
use tokio::time::sleep;
use std::collections::HashSet;
use std::net::Ipv4Addr;
use std::{
    fs, path::PathBuf, sync::Arc, time
};
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

    let _ = board.lock().await.clone().unwrap().set_relay(&data).await;

    let app_state = Arc::new(AppState {
        json_data: state.clone(),
        tx,
        board
    });

    let app = Router::new()
        .route("/", get(serve_index))
        .fallback_service(ServeDir::new("public"))
        .route("/data", get(serve_data_handler).post(receive_data_handler))
        .route("/register", post(register_board))
        .with_state(app_state.clone());

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
    let poll_timeout = time::Duration::from_secs(60);

    let event_future = async {
        loop {
            match rx.recv().await {
                Ok(event) => {
                    return Ok(event);
                }
                Err(broadcast::error::RecvError::Lagged(_count)) => {
                    return Err(StatusCode::RESET_CONTENT);
                }
                Err(broadcast::error::RecvError::Closed) => {
                    return Err(StatusCode::INTERNAL_SERVER_ERROR);
                }
            }
        }
    };

    match tokio::time::timeout(poll_timeout, event_future).await {
        Ok(Ok(event)) => {
            (StatusCode::OK, Json(event)).into_response()
        }
        Ok(Err(status)) => status.into_response(),
        Err(_) => {
            StatusCode::NO_CONTENT.into_response()
        }
    }
}

async fn receive_data_handler(
    State(state): State<Arc<AppState>>,
    Json(updated_item): Json<Item>
    ) -> impl IntoResponse {
    let mut data = state.json_data.lock().await;
    let mut item_updated = false;
    let mut status_code = Some(StatusCode::NOT_FOUND);
    let mut updated_item_for_event: Option<Item> = None;
    for item in &mut data.relays {
        if item.id == updated_item.id {
            let now = time::SystemTime::now();
            match now.duration_since(item.last_updated) {
                Ok(duration) if duration > time::Duration::from_secs(2) => {
                    if updated_item.state {
                        item.name = updated_item.name.clone();
                        item.ipv4 = updated_item.ipv4.clone();
                    } else {
                        item.name = "-".to_string();
                        item.ipv4 = "-.-.-.-".to_string();
                    }
                    item.state = updated_item.state;
                    item.last_updated = time::SystemTime::now();
                    updated_item_for_event = Some(updated_item.clone());
                    item_updated = true;
                },
                Ok(_) => println!("Less than 2 seconds. Skipping relay {}...", item.id),
                Err(e) => eprintln!("System time went backwards: {:?}", e),
            }
        }
    };
    if item_updated {
        if let Err(e) = save_json(&data, CONFIG_PATH) {
            eprintln!("Error saving data to file: {}", e);
            status_code = Some(StatusCode::INTERNAL_SERVER_ERROR);
        } else {
            if let Some(updated) = updated_item_for_event {
                let event = ServerEvent { updated_item: updated };
                let _ = state.tx.send(event);
            }
            status_code = Some(StatusCode::OK);
        }
    }

    let board_guard = state.board.lock().await;
    if let Some(board_ref) = board_guard.as_ref() {
        let _ = board_ref.set_relay(&data).await;
    };

    if !updated_item.state {
        return status_code.unwrap();
    }
    let state_clone = Arc::clone(&state);

    spawn(async move {
        let mut data = state_clone.json_data.lock().await;

        let board_guard = state_clone.board.lock().await;
        let board_ref = match board_guard.as_ref() {
            Some(b) => b,
            None => {
                eprintln!("No board initialized.");
                return;
            }
        };

        let first_scan: HashSet<Ipv4Addr> = board_ref.get_panels().await.into_iter().collect();

        sleep(time::Duration::from_secs(45)).await;

        let second_scan: HashSet<Ipv4Addr> = board_ref.get_panels().await.into_iter().collect();
        let new_devices: Vec<Ipv4Addr> = second_scan
            .difference(&first_scan)
            .cloned()
            .collect();

        let mut ip: Option<Ipv4Addr> = None;
        if new_devices.is_empty() {
            println!("No new devices found.");
        } else {
            if new_devices.len() == 1 {
                ip = Some(*new_devices.first().unwrap());
            }
        }
        if ip.is_none() { return; }
        let mut updated_item_for_event: Option<Item> = None;
        for item in &mut data.relays {
            if item.id == updated_item.id {
                item.ipv4 = ip.unwrap().to_string();
                updated_item_for_event = Some(item.clone());
                item_updated = true;
            }
        }
        if item_updated {
            if let Err(e) = save_json(&data, CONFIG_PATH) {
                eprintln!("Error saving data to file: {}", e);
            } else {
                if let Some(updated) = updated_item_for_event {
                    let event = ServerEvent { updated_item: updated };
                    tokio::time::sleep(time::Duration::from_millis(500)).await;
                    let _ = state_clone.tx.send(event);
                }
            }
        }
    });

    status_code.unwrap()
}

pub async fn register_board(
    State(state): State<Arc<AppState>>,
    Json(details): Json<BoardDetails>
) -> impl IntoResponse {
    if details.device == "relayBoard" {
        let mut board_lock = state.board.lock().await;
        if board_lock.is_some() {
            println!("Board already registered with IP: {:?}", board_lock.as_ref().unwrap().ip);
            return (StatusCode::CONFLICT, "Board already registered").into_response();
        }

        let new_board = Arc::new(Board::new(details.ip.clone().parse::<Ipv4Addr>().unwrap()).await);
        *board_lock = Some(new_board.clone());

        let data = state.json_data.lock().await;
        let _ = new_board.set_relay(&data).await;

        return (StatusCode::OK, format!("Board registered successfully with IP: {}", details.ip)).into_response();
    }
    (StatusCode::BAD_REQUEST, "Invalid device type").into_response()
}
