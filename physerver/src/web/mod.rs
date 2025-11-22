use crate::protocol::{Command, Status};
use axum::{
    extract::{
        ws::{WebSocket, WebSocketUpgrade},
        State,
    },
    http::StatusCode,
    response::{Html, IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::{broadcast, RwLock};
use tower_http::cors::CorsLayer;
use tracing::{info, warn};

/// Shared application state
pub struct AppState {
    pub current_command: Arc<RwLock<Command>>,
    pub current_status: Arc<RwLock<Status>>,
    pub status_broadcast: broadcast::Sender<Status>,
}

impl AppState {
    pub fn new() -> Self {
        let (tx, _) = broadcast::channel(100);

        Self {
            current_command: Arc::new(RwLock::new(Command::default())),
            current_status: Arc::new(RwLock::new(Status::default())),
            status_broadcast: tx,
        }
    }
}

/// Create the web server router
pub fn create_router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/", get(index_handler))
        .route("/api/status", get(get_status))
        .route("/api/command", post(set_command))
        .route("/api/command", get(get_command))
        .route("/api/gpio/set", post(set_gpio))
        .route("/api/dac/set", post(set_dac))
        .route("/api/adc/read", get(read_adc))
        .route("/ws", get(websocket_handler))
        .layer(CorsLayer::permissive())
        .with_state(state)
}

/// Serve the main HTML page
async fn index_handler() -> Html<&'static str> {
    Html(include_str!("../../static/index.html"))
}

/// Get current status (REST API)
async fn get_status(State(state): State<Arc<AppState>>) -> Json<Status> {
    let status = state.current_status.read().await;
    Json(status.clone())
}

/// Get current command (REST API)
async fn get_command(State(state): State<Arc<AppState>>) -> Json<Command> {
    let cmd = state.current_command.read().await;
    Json(cmd.clone())
}

/// Set command (REST API)
async fn set_command(
    State(state): State<Arc<AppState>>,
    Json(cmd): Json<Command>,
) -> Response {
    let mut current = state.current_command.write().await;
    *current = cmd;
    info!("Command updated via REST API");

    (StatusCode::OK, "Command updated").into_response()
}

#[derive(Deserialize)]
struct GpioRequest {
    pin: u8,
    value: bool,
}

/// Set a single GPIO pin
async fn set_gpio(
    State(state): State<Arc<AppState>>,
    Json(req): Json<GpioRequest>,
) -> Response {
    if req.pin >= 16 {
        return (StatusCode::BAD_REQUEST, "Pin must be 0-15").into_response();
    }

    let mut cmd = state.current_command.write().await;

    if req.value {
        cmd.digital_out |= 1 << req.pin;
    } else {
        cmd.digital_out &= !(1 << req.pin);
    }

    info!("GPIO pin {} set to {}", req.pin, req.value);

    (StatusCode::OK, "GPIO updated").into_response()
}

#[derive(Deserialize)]
struct DacRequest {
    channel: u8,
    value: u16,
}

/// Set DAC output
async fn set_dac(
    State(state): State<Arc<AppState>>,
    Json(req): Json<DacRequest>,
) -> Response {
    if req.channel >= 2 {
        return (StatusCode::BAD_REQUEST, "Channel must be 0 or 1").into_response();
    }

    if req.value > 4095 {
        return (StatusCode::BAD_REQUEST, "Value must be 0-4095").into_response();
    }

    let mut cmd = state.current_command.write().await;
    cmd.dac[req.channel as usize] = req.value;

    info!("DAC channel {} set to {}", req.channel, req.value);

    (StatusCode::OK, "DAC updated").into_response()
}

#[derive(Serialize)]
struct AdcResponse {
    channels: [u16; 8],
}

/// Read ADC values
async fn read_adc(State(state): State<Arc<AppState>>) -> Json<AdcResponse> {
    let status = state.current_status.read().await;
    Json(AdcResponse {
        channels: status.adc,
    })
}

/// WebSocket handler
async fn websocket_handler(
    ws: WebSocketUpgrade,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    ws.on_upgrade(|socket| websocket_connection(socket, state))
}

/// Handle WebSocket connection
async fn websocket_connection(mut socket: WebSocket, state: Arc<AppState>) {
    info!("WebSocket client connected");

    let mut rx = state.status_broadcast.subscribe();

    loop {
        tokio::select! {
            // Receive status updates and send to client
            status = rx.recv() => {
                match status {
                    Ok(status) => {
                        let json = serde_json::to_string(&status).unwrap();
                        if socket.send(axum::extract::ws::Message::Text(json)).await.is_err() {
                            break;
                        }
                    }
                    Err(e) => {
                        warn!("Broadcast receive error: {}", e);
                        break;
                    }
                }
            }

            // Receive messages from client (commands)
            msg = socket.recv() => {
                match msg {
                    Some(Ok(axum::extract::ws::Message::Text(text))) => {
                        if let Ok(cmd) = serde_json::from_str::<Command>(&text) {
                            let mut current = state.current_command.write().await;
                            *current = cmd;
                            info!("Command updated via WebSocket");
                        }
                    }
                    Some(Ok(axum::extract::ws::Message::Close(_))) => {
                        break;
                    }
                    None => break,
                    _ => {}
                }
            }
        }
    }

    info!("WebSocket client disconnected");
}

/// Start the web server
pub async fn start_web_server(state: Arc<AppState>, port: u16) -> anyhow::Result<()> {
    let app = create_router(state);

    let addr = format!("0.0.0.0:{}", port);
    info!("Starting web server on http://{}", addr);

    let listener = tokio::net::TcpListener::bind(&addr).await?;

    axum::serve(listener, app).await?;

    Ok(())
}
