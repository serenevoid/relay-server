// use std::{thread, time::Duration};

// mod controller;
// use controller::SerialController;

use axum::{
    body::Body,
    http::{Response, StatusCode},
    response::IntoResponse,
    routing::{get, post},
    Router,
    Json
};
use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex}
};
use tower_http::services::ServeDir;
use tokio::fs;
use serde_json::{json, Value};

type SharedJson = Arc<Mutex<Value>>;

const ADDR: &str = "0.0.0.0:3402";

#[tokio::main]
async fn main() {

    // let mut port = SerialController::open_port().unwrap();
    // thread::sleep(Duration::from_secs(12));
    // let response = port.write(101).unwrap();
    // println!("{}", response);

    let data = load_or_init_json("data.json");
    let state = Arc::new(Mutex::new(data));

    let app = Router::new()
        .route("/", get(serve_index))
        .fallback_service(ServeDir::new("public"))
        .route("/data.json", get(serve_data));

    let listener = tokio::net::TcpListener::bind(ADDR).await.unwrap();
    println!("Server running on: {}", ADDR);
    axum::serve(listener, app).await.unwrap();
}

fn load_or_init_json(path: &str) -> Value {
    if Path::new(path).exists() {
        let contents = fs::read_to_string(path).unwrap_or_else(|_| "{}".into());
        serde_json::from_str(&contents).unwrap_or_else(|_| json!({}))
    } else {
        json!({})
    }
}

async fn serve_index() -> impl IntoResponse {
    let path = PathBuf::from("public/index.html");

    match fs::read(&path).await {
        Ok(contents) => Response::builder()
            .header("Content-Type", "text/html")
            .body(Body::from(contents))
            .unwrap(),
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "Failed to load index.html").into_response(),
    }
}

async fn serve_data() -> impl IntoResponse {
    
}
