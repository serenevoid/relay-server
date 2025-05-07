// use std::{thread, time::Duration};

// mod controller;
// use controller::SerialController;

use axum::{
    body::Body,
    http::{Response, StatusCode},
    response::IntoResponse,
    routing::get,
    Router,
};
use std::path::PathBuf;
use tower_http::services::ServeDir;
use tokio::fs;
const ADDR: &str = "127.0.0.1:3000";

#[tokio::main]
async fn main() {

    // let mut port = SerialController::open_port().unwrap();
    // thread::sleep(Duration::from_secs(12));
    // let response = port.write(101).unwrap();
    // println!("{}", response);

    let app = Router::new()
        .route("/", get(serve_index))
        .fallback_service(ServeDir::new("assets"));

    let listener = tokio::net::TcpListener::bind(ADDR).await.unwrap();
    println!("Server running on: {}", ADDR);
    axum::serve(listener, app).await.unwrap();
}

async fn serve_index() -> impl IntoResponse {
    let path = PathBuf::from("assets/index.html");

    match fs::read(&path).await {
        Ok(contents) => Response::builder()
            .header("Content-Type", "text/html")
            .body(Body::from(contents))
            .unwrap(),
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "Failed to load index.html").into_response(),
    }
}
