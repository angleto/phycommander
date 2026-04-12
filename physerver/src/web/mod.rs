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
use std::sync::atomic::{AtomicU16, Ordering};
use std::sync::Arc;
use tokio::sync::{broadcast, RwLock};
use tower_http::cors::CorsLayer;
use tracing::{info, warn};

/// Shared application state
pub struct AppState {
    pub current_command: Arc<RwLock<Command>>,
    pub current_status: Arc<RwLock<Status>>,
    pub status_broadcast: broadcast::Sender<Status>,
    /// Baseline for the firmware's error_count counter. Set to the raw
    /// value at startup, and updated on POST /api/reset_errors. The
    /// status broadcast on the WebSocket / GET /api/status reports
    /// `raw_count - baseline` so the user sees 0 after a reset.
    pub error_baseline: AtomicU16,
    /// `true` once the baseline has been initialised from the first
    /// status frame received after startup.
    pub error_baseline_initialized: std::sync::atomic::AtomicBool,
    /// Handle to the RT scheduler's stats (set by main after the
    /// scheduler is created). Served by /api/rt_stats.
    pub rt_stats: std::sync::OnceLock<Arc<phycmd_core::RtStats>>,
}

impl AppState {
    pub fn new() -> Self {
        let (tx, _) = broadcast::channel(100);

        Self {
            current_command: Arc::new(RwLock::new(Command::default())),
            current_status: Arc::new(RwLock::new(Status::default())),
            status_broadcast: tx,
            error_baseline: AtomicU16::new(0),
            error_baseline_initialized: std::sync::atomic::AtomicBool::new(false),
            rt_stats: std::sync::OnceLock::new(),
        }
    }

    /// Register the RT scheduler's stats so /api/rt_stats can read them.
    pub fn set_rt_stats(&self, stats: Arc<phycmd_core::RtStats>) {
        let _ = self.rt_stats.set(stats);
    }

    /// Apply the error_count baseline to a raw Status: subtract the
    /// stored baseline with wrap-safe arithmetic (the firmware counter
    /// saturates at u16::MAX, it doesn't wrap, so simple subtraction
    /// is OK). Returns a new Status with adjusted error_count.
    pub fn apply_error_baseline(&self, mut status: Status) -> Status {
        let baseline = self.error_baseline.load(Ordering::Relaxed);
        status.error_count = status.error_count.saturating_sub(baseline);
        status
    }

    /// Called from the RT loop when a fresh status arrives: on the
    /// first frame, initialise the baseline to the current counter
    /// so subsequent frames show 0 until a real error occurs.
    pub fn maybe_init_error_baseline(&self, raw_error_count: u16) {
        if !self.error_baseline_initialized.load(Ordering::Relaxed) {
            self.error_baseline.store(raw_error_count, Ordering::Relaxed);
            self.error_baseline_initialized.store(true, Ordering::Relaxed);
            info!(
                "error_count baseline initialised at {} (accumulated during previous runs)",
                raw_error_count
            );
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
        .route("/api/sysinfo", get(get_sysinfo))
        .route("/api/rt_stats", get(get_rt_stats))
        .route("/api/reset_errors", post(reset_errors))
        .route("/ws", get(websocket_handler))
        .layer(CorsLayer::permissive())
        .with_state(state)
}

/// Serve the main HTML page
async fn index_handler() -> Html<&'static str> {
    Html(include_str!("../../static/index.html"))
}

/// Get current status (REST API). The error_count in the returned
/// status has the baseline applied (see AppState::apply_error_baseline).
async fn get_status(State(state): State<Arc<AppState>>) -> Json<Status> {
    let status = state.current_status.read().await.clone();
    Json(state.apply_error_baseline(status))
}

/// Reset the error_count baseline to the current raw firmware counter.
/// Subsequent /api/status and WebSocket frames will report 0 errors
/// until a new CRC/header error is accumulated by the device.
async fn reset_errors(State(state): State<Arc<AppState>>) -> Response {
    let current_raw = state.current_status.read().await.error_count;
    // Restore the original baseline before the current subtraction, so
    // the raw counter is recovered, then bump it to the current raw.
    // Simpler: just set baseline = raw, giving a displayed delta of 0.
    state.error_baseline.store(current_raw, Ordering::Relaxed);
    state.error_baseline_initialized.store(true, Ordering::Relaxed);
    info!("error_count baseline reset to raw={}", current_raw);
    (StatusCode::OK, format!("Errors reset (raw counter was {})", current_raw))
        .into_response()
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

/// Read a fresh system telemetry snapshot (hwmon sensors, CPU freq, load,
/// memory, uptime). This reads directly from sysfs/procfs on every call —
/// cheap enough for a REST endpoint polled every few seconds.
async fn get_sysinfo() -> Json<crate::sysinfo::SysInfoSnapshot> {
    Json(crate::sysinfo::read_snapshot())
}

/// Return a snapshot of the RT scheduler statistics: tick count,
/// missed ticks, latency/jitter histogram, etc. Populated by the
/// RtScheduler running in a dedicated SCHED_FIFO thread.
#[derive(Serialize)]
struct RtStatsResponse {
    tick_count: u64,
    tick_ok: u64,
    missed_ticks: u64,
    transport_errors: u64,
    latency_max_us: u32,
    mean_latency_us: f64,
    jitter_min_us: i64,
    jitter_max_us: i64,
    mean_abs_jitter_us: f64,
    jitter_histogram: [u64; 9],
    jitter_buckets_us: [i32; 8],
    success_ratio: f64,
}

async fn get_rt_stats(State(state): State<Arc<AppState>>) -> Json<RtStatsResponse> {
    let response = if let Some(stats) = state.rt_stats.get() {
        let snap = stats.snapshot();
        RtStatsResponse {
            tick_count: snap.tick_count,
            tick_ok: snap.tick_ok,
            missed_ticks: snap.missed_ticks,
            transport_errors: snap.transport_errors,
            latency_max_us: snap.latency_max_us,
            mean_latency_us: snap.mean_latency_us,
            jitter_min_us: snap.jitter_min_us,
            jitter_max_us: snap.jitter_max_us,
            mean_abs_jitter_us: snap.mean_abs_jitter_us,
            jitter_histogram: snap.jitter_histogram,
            jitter_buckets_us: {
                let mut arr = [0i32; 8];
                for (i, v) in phycmd_core::JITTER_BUCKET_BOUNDS_US.iter().enumerate() {
                    if i < 8 { arr[i] = *v; }
                }
                arr
            },
            success_ratio: snap.success_ratio(),
        }
    } else {
        // Scheduler not yet registered (service still booting)
        RtStatsResponse {
            tick_count: 0, tick_ok: 0, missed_ticks: 0, transport_errors: 0,
            latency_max_us: 0, mean_latency_us: 0.0,
            jitter_min_us: 0, jitter_max_us: 0, mean_abs_jitter_us: 0.0,
            jitter_histogram: [0; 9], jitter_buckets_us: [0; 8],
            success_ratio: 1.0,
        }
    };
    Json(response)
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
                        // Apply error_count baseline before serialising
                        let adjusted = state.apply_error_baseline(status);
                        let json = serde_json::to_string(&adjusted).unwrap();
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
