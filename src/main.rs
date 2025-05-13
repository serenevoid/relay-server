mod controller;

use axum::{
    body::Body, extract::{Json, Query, State}, http::{Response, StatusCode}, response::IntoResponse, routing::get, Router
};
use tokio::sync::{broadcast, Mutex};
use std::{
    fs, path::{Path, PathBuf}, sync::Arc, time::Duration
};
use tower_http::services::ServeDir;
use serde::{Serialize, Deserialize};

#[derive(Debug, Deserialize)]
pub struct InitialQueryParams {
    pub initial_event: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct Item {
    id: u32,
    name: String,
    panel_category: String,
    ipv4: String,
    state: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct States {
    relays: Vec<Item>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerEvent {
    updated_item: Item,
}

pub struct AppState {
    pub json_data: Arc<Mutex<States>>,
    pub tx: broadcast::Sender<ServerEvent>
}

const ADDR: &str = "0.0.0.0:3402";
const CONFIG_PATH: &str = "./data.json";

#[tokio::main]
async fn main() {
    let data = load_or_init_json(CONFIG_PATH);
    let state = Arc::new(Mutex::new(data));
    let (tx, _rx) = broadcast::channel::<ServerEvent>(16);
    let app_state = Arc::new(AppState {
        json_data: state.clone(),
        tx
    });

    let app = Router::new()
        .route("/", get(serve_index))
        .fallback_service(ServeDir::new("public"))
        .route("/data", get(serve_data_handler).post(receive_data_handler))
        .with_state(app_state.clone());

    let listener = tokio::net::TcpListener::bind(ADDR).await.unwrap();
    println!("Server running on: {}", listener.local_addr().unwrap());
    axum::serve(listener, app).await.unwrap();
}

fn load_or_init_json(path: &str) -> States {
    if Path::new(path).exists() {
        let contents = fs::read_to_string(path).unwrap_or_else(|_| "{}".into());
        serde_json::from_str(&contents).unwrap_or_else(|_| States { relays: vec![] })
    } else {
        States { relays: (1..=10)
            .map(|i| Item {
                id: i,
                name: String::from("user"),
                panel_category: String::from("panel_category"),
                ipv4: String::from("-.-.-.-"),
                state: false
            })
            .collect()
        }
    }
}

fn save_json(data: &States) -> Result<(), std::io::Error> {
    fs::write(CONFIG_PATH, serde_json::to_string_pretty(data).unwrap())
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
    let mut relay_state: u16 = 0;
    for item in &mut data.relays {
        if item.id == updated_item.id {
            item.name = updated_item.name.clone();
            item.ipv4 = updated_item.ipv4.clone();
            item.panel_category = updated_item.panel_category.clone();
            item.state = updated_item.state;
            updated_item_for_event = Some(updated_item.clone());
            item_updated = true;
        }
        if item.state {
            relay_state |= 1 << (item.id - 1);
        }
    };
    if item_updated {
        if let Err(e) = save_json(&data) {
            eprintln!("Error saving data to file: {}", e);
            status_code = Some(StatusCode::INTERNAL_SERVER_ERROR);
        } else {
            if let Some(updated) = updated_item_for_event {
                let event = ServerEvent { updated_item: updated };
                if let Err(_e) = state.tx.send(event) {}
            }
            status_code = Some(StatusCode::OK);
        }
    }
    println!("{}", controller::set_relays(relay_state).unwrap());
    status_code.unwrap()
}
