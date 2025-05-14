use crate::{controller, filesystem::save_json, ADDR, CONFIG_PATH};
use axum::{
    body::Body, extract::{Json, Query, State}, http::{Response, StatusCode}, response::IntoResponse, routing::get, Router
};
use tokio::sync::{broadcast, Mutex};
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
    pub panel_category: String,
    pub ipv4: String,
    pub last_updated: time::SystemTime,
    pub state: bool
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
    pub tx: broadcast::Sender<ServerEvent>
}

pub async fn launch_server(data: States) {
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
                Ok(duration) if duration > time::Duration::from_secs(10) => {
                    if updated_item.state {
                        item.name = updated_item.name.clone();
                        item.ipv4 = updated_item.ipv4.clone();
                        item.panel_category = updated_item.panel_category.clone();
                    } else {
                        item.name = "-".to_string();
                        item.ipv4 = "-.-.-.-".to_string();
                        item.panel_category = "-".to_string();
                    }
                    item.state = updated_item.state;
                    item.last_updated = time::SystemTime::now();
                    updated_item_for_event = Some(updated_item.clone());
                    item_updated = true;
                },
                Ok(_) => println!("Less than 10 seconds. Skipping relay {}...", item.id),
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
                if let Err(_e) = state.tx.send(event) {}
            }
            status_code = Some(StatusCode::OK);
        }
    }
    controller::set_relays(&data).unwrap();
    status_code.unwrap()
}
