mod controller;

use axum::{
    body::Body,
    http::{Response, StatusCode},
    response::IntoResponse,
    routing::get,
    extract::{State, Json},
    Router
};
use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    fs
};
use tower_http::services::ServeDir;
use serde::{Serialize, Deserialize};


#[derive(Debug, Serialize, Deserialize, Clone)]
struct Item {
    id: u32,
    name: String,
    panel_category: String,
    ipv4: String,
    state: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct States {
    relays: Vec<Item>,
}

type SharedJson = Arc<Mutex<States>>;

const ADDR: &str = "0.0.0.0:3402";
const CONFIG_PATH: &str = "./data.json";

#[tokio::main]
async fn main() {
    let data = load_or_init_json(CONFIG_PATH);
    let state = Arc::new(Mutex::new(data));

    let app = Router::new()
        .route("/", get(serve_index))
        .fallback_service(ServeDir::new("public"))
        .route("/data.json", get(serve_data_handler).post(receive_data_handler))
        .with_state(state.clone());

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

async fn serve_data_handler(State(state): State<SharedJson>) -> impl IntoResponse {
    let data = state.lock().unwrap();
    Json(data.clone())
}

async fn receive_data_handler(State(state): State<SharedJson>, Json(updated_item): Json<Item>) -> impl IntoResponse {
    let mut data = state.lock().unwrap();
    let mut status_code = None;
    if let Some(existing_item) = data.relays.iter_mut().find(|item| item.id == updated_item.id) {
        existing_item.name = updated_item.name;
        existing_item.ipv4 = updated_item.ipv4;
        existing_item.panel_category = updated_item.panel_category;
        existing_item.state = updated_item.state;
        if let Err(e) = save_json(&data) {
            eprintln!("Error saving data to file: {}", e);
            return StatusCode::INTERNAL_SERVER_ERROR;
        }
        status_code = Some(StatusCode::OK);
    } else {
        status_code = Some(StatusCode::NOT_FOUND);
    }
    println!("{}", controller::set_relays(101).unwrap());
    status_code.unwrap()
}
