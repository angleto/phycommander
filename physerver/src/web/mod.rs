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
    /// Handle to the iso transport's per-direction counters. Set
    /// only when running in iso mode; absent in bulk mode. Folded
    /// into the /api/rt_stats response when present.
    pub iso_stats: std::sync::OnceLock<Arc<phycmd_core::transport::IsoStats>>,
    /// Handle to the waveform generator inside IsoTransport. Only
    /// set in iso mode; the /api/waveform endpoints return 503 in
    /// bulk mode where there is no per-microframe rendering path.
    pub waveforms: std::sync::OnceLock<Arc<phycmd_core::WaveformBank>>,
    /// Typed control-plane client for the on-chip function generator
    /// (vendor SETUP requests on EP0). Set in iso mode along with
    /// the iso stats; absent in bulk mode where there is no shared
    /// libusb dev_handle.
    pub waveform_dev: std::sync::OnceLock<Arc<phycmd_core::transport::WaveformDevice>>,
    /// Handle to the iso transport itself. Used by the telemetry-detail
    /// toggle endpoint; absent in bulk mode.
    pub iso_transport: std::sync::OnceLock<Arc<phycmd_core::transport::IsoTransport>>,
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
            iso_stats: std::sync::OnceLock::new(),
            waveforms: std::sync::OnceLock::new(),
            waveform_dev: std::sync::OnceLock::new(),
            iso_transport: std::sync::OnceLock::new(),
        }
    }

    /// Register the RT scheduler's stats so /api/rt_stats can read them.
    pub fn set_rt_stats(&self, stats: Arc<phycmd_core::RtStats>) {
        let _ = self.rt_stats.set(stats);
    }

    /// Register the iso transport's stats (only set when running in
    /// iso mode). Folded into /api/rt_stats when present.
    pub fn set_iso_stats(&self, stats: Arc<phycmd_core::transport::IsoStats>) {
        let _ = self.iso_stats.set(stats);
    }

    /// Register the iso transport's WaveformBank, exposing the
    /// /api/waveform endpoints to the dashboard / scripts.
    pub fn set_waveforms(&self, w: Arc<phycmd_core::WaveformBank>) {
        let _ = self.waveforms.set(w);
    }

    pub fn set_waveform_dev(&self, dev: Arc<phycmd_core::transport::WaveformDevice>) {
        let _ = self.waveform_dev.set(dev);
    }

    pub fn set_iso_transport(&self, t: Arc<phycmd_core::transport::IsoTransport>) {
        let _ = self.iso_transport.set(t);
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
        .route("/logo.svg", get(|| async { svg_asset(include_str!("../../static/logo.svg")) }))
        .route(
            "/logo-mark.svg",
            get(|| async { svg_asset(include_str!("../../static/logo-mark.svg")) }),
        )
        .route("/api/status", get(get_status))
        .route("/api/command", post(set_command))
        .route("/api/command", get(get_command))
        .route("/api/gpio/set", post(set_gpio))
        .route("/api/dac/set", post(set_dac))
        .route("/api/adc/read", get(read_adc))
        .route("/api/sysinfo", get(get_sysinfo))
        .route("/api/rt_stats", get(get_rt_stats))
        .route("/api/reset_errors", post(reset_errors))
        .route("/api/reset_telemetry", post(reset_telemetry))
        .route("/api/telemetry/detail", get(get_telemetry_detail))
        .route("/api/telemetry/detail", post(set_telemetry_detail))
        .route("/api/waveform", get(get_waveforms))
        .route("/api/waveform/:channel", post(set_waveform))
        .route("/api/waveform/:channel", axum::routing::delete(disable_waveform))
        // Firmware fn-gen (vendor SETUP plane). Available in iso mode only.
        .route("/api/fngen/caps", get(fngen_caps))
        .route("/api/fngen/state/:channel", get(fngen_state))
        .route("/api/fngen/stop/:channel", post(fngen_stop))
        .route("/api/fngen/dac_clock", get(fngen_dac_get_clock))
        .route("/api/fngen/dac_clock", post(fngen_dac_set_clock))
        .route("/api/fngen/adc_rate", get(fngen_adc_get_rate))
        .route("/api/fngen/adc_rate", post(fngen_adc_set_rate))
        .route("/api/fngen/play_builtin/:channel", post(fngen_play_builtin))
        .route("/api/fngen/play_arbitrary/:channel", post(fngen_play_arbitrary))
        .route("/api/fngen/play_lut/:channel", post(fngen_play_lut))
        .route("/api/fngen/play_threshold/:channel", post(fngen_play_threshold))
        .route("/api/fngen/play_pulse_trig/:channel", post(fngen_play_pulse_trig))
        .route("/api/fngen/play_pid/:channel", post(fngen_play_pid))
        .route("/ws", get(websocket_handler))
        .layer(CorsLayer::permissive())
        .with_state(state)
}

fn svg_asset(body: &'static str) -> impl IntoResponse {
    (
        [("content-type", "image/svg+xml"), ("cache-control", "public, max-age=3600")],
        body,
    )
}

/// Serve the main HTML page. We set `Cache-Control: no-store` so the
/// browser always fetches the latest dashboard after a redeploy —
/// without it, Safari/Firefox heuristics were serving stale HTML and
/// the user kept hitting bugs that were already fixed on the server.
async fn index_handler() -> impl IntoResponse {
    (
        [
            ("cache-control", "no-store, max-age=0"),
            ("content-type", "text/html; charset=utf-8"),
        ],
        include_str!("../../static/index.html"),
    )
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
    (StatusCode::OK, format!("Errors reset (raw counter was {})", current_raw)).into_response()
}

#[derive(serde::Serialize, serde::Deserialize)]
struct TelemetryDetailFlag {
    enabled: bool,
}

async fn get_telemetry_detail(State(state): State<Arc<AppState>>) -> Response {
    let on = state.iso_transport.get().map(|t| t.telemetry_detail()).unwrap_or(false);
    Json(TelemetryDetailFlag { enabled: on }).into_response()
}
async fn set_telemetry_detail(
    State(state): State<Arc<AppState>>,
    Json(req): Json<TelemetryDetailFlag>,
) -> Response {
    match state.iso_transport.get() {
        Some(t) => {
            t.set_telemetry_detail(req.enabled);
            Json(TelemetryDetailFlag { enabled: req.enabled }).into_response()
        }
        None => (StatusCode::SERVICE_UNAVAILABLE, "iso transport not active").into_response(),
    }
}

/// POST /api/reset_telemetry — zero the RT-scheduler stats and iso
/// packet counters so the dashboard can start fresh without a
/// service restart. Also re-applies the error_count baseline so the
/// two meters agree on "since now".
async fn reset_telemetry(State(state): State<Arc<AppState>>) -> Response {
    let mut cleared = Vec::new();
    if let Some(stats) = state.rt_stats.get() {
        stats.reset();
        cleared.push("rt_stats");
    }
    if let Some(iso) = state.iso_stats.get() {
        iso.reset();
        cleared.push("iso_stats");
    }
    let current_raw = state.current_status.read().await.error_count;
    state.error_baseline.store(current_raw, Ordering::Relaxed);
    state.error_baseline_initialized.store(true, Ordering::Relaxed);
    cleared.push("error_baseline");
    info!("telemetry reset: {}", cleared.join(", "));
    (StatusCode::OK, format!("Telemetry reset ({})", cleared.join(", "))).into_response()
}

/// GET /api/waveform — return all 4 channel specs (or 503 in bulk mode).
async fn get_waveforms(State(state): State<Arc<AppState>>) -> Response {
    let Some(w) = state.waveforms.get() else {
        return (StatusCode::SERVICE_UNAVAILABLE, "waveform generator only available in iso mode")
            .into_response();
    };
    Json(w.snapshot()).into_response()
}

/// POST /api/waveform/{channel} — enable / update the spec for a channel.
/// Body is a JSON [`phycmd_core::WaveformSpec`] (`max_value` ignored, set
/// by the server based on channel kind).
async fn set_waveform(
    State(state): State<Arc<AppState>>,
    axum::extract::Path(channel): axum::extract::Path<String>,
    Json(spec): Json<phycmd_core::WaveformSpec>,
) -> Response {
    let Some(w) = state.waveforms.get() else {
        return (StatusCode::SERVICE_UNAVAILABLE, "waveform generator only available in iso mode")
            .into_response();
    };
    match w.set(&channel, spec) {
        Ok(()) => {
            info!(
                "waveform {channel} set: shape={:?} freq={} amp={} off={} enabled={}",
                spec.shape, spec.freq_hz, spec.amplitude, spec.offset, spec.enabled
            );
            (StatusCode::OK, "ok").into_response()
        }
        Err(e) => (StatusCode::BAD_REQUEST, e).into_response(),
    }
}

/// DELETE /api/waveform/{channel} — disable the channel's waveform,
/// returning control of that DAC/PWM to staging snapshots.
async fn disable_waveform(
    State(state): State<Arc<AppState>>,
    axum::extract::Path(channel): axum::extract::Path<String>,
) -> Response {
    let Some(w) = state.waveforms.get() else {
        return (StatusCode::SERVICE_UNAVAILABLE, "waveform generator only available in iso mode")
            .into_response();
    };
    let lock = match w.channel(&channel) {
        Some(l) => l,
        None => return (StatusCode::BAD_REQUEST, "unknown channel").into_response(),
    };
    lock.write().enabled = false;
    info!("waveform {channel} disabled");
    (StatusCode::OK, "ok").into_response()
}

// =================================================================
//   Firmware fn-gen REST handlers (vendor SETUP plane on EP0)
//
//   All endpoints return 503 in bulk mode (no shared dev_handle).
//   In iso mode they tunnel typed POSTs into libusb_control_transfer
//   via WaveformDevice. Every JSON body mirrors the Wave*Spec /
//   Wave*Header struct from PROTOCOL.md §6.2.
// =================================================================

fn fngen_unavailable_response() -> Response {
    (
        StatusCode::SERVICE_UNAVAILABLE,
        "function generator only available in iso mode (config: type = \"iso\")",
    )
        .into_response()
}

fn fngen_err(e: phycmd_core::transport::WaveformError) -> Response {
    let msg = e.to_string();
    let code = match e {
        phycmd_core::transport::WaveformError::ControlTransferStalled(_) => StatusCode::BAD_REQUEST,
        phycmd_core::transport::WaveformError::UnknownChannel(_) => StatusCode::NOT_FOUND,
        _ => StatusCode::INTERNAL_SERVER_ERROR,
    };
    (code, msg).into_response()
}

async fn fngen_caps(State(state): State<Arc<AppState>>) -> Response {
    let Some(d) = state.waveform_dev.get() else {
        return fngen_unavailable_response();
    };
    match d.caps() {
        Ok(c) => Json(c).into_response(),
        Err(e) => fngen_err(e),
    }
}

async fn fngen_state(
    State(state): State<Arc<AppState>>,
    axum::extract::Path(channel): axum::extract::Path<String>,
) -> Response {
    let Some(d) = state.waveform_dev.get() else {
        return fngen_unavailable_response();
    };
    match d.state(&channel) {
        Ok(s) => Json(s).into_response(),
        Err(e) => fngen_err(e),
    }
}

async fn fngen_stop(
    State(state): State<Arc<AppState>>,
    axum::extract::Path(channel): axum::extract::Path<String>,
) -> Response {
    let Some(d) = state.waveform_dev.get() else {
        return fngen_unavailable_response();
    };
    match d.stop(&channel) {
        Ok(()) => (StatusCode::OK, "ok").into_response(),
        Err(e) => fngen_err(e),
    }
}

#[derive(Serialize)]
struct U32Body {
    value: u32,
}
#[derive(Deserialize)]
struct U32Req {
    value: u32,
}

async fn fngen_dac_get_clock(State(state): State<Arc<AppState>>) -> Response {
    let Some(d) = state.waveform_dev.get() else {
        return fngen_unavailable_response();
    };
    match d.dac_get_clock() {
        Ok(v) => Json(U32Body { value: v }).into_response(),
        Err(e) => fngen_err(e),
    }
}
async fn fngen_dac_set_clock(
    State(state): State<Arc<AppState>>,
    Json(req): Json<U32Req>,
) -> Response {
    let Some(d) = state.waveform_dev.get() else {
        return fngen_unavailable_response();
    };
    match d.dac_set_clock(req.value) {
        Ok(()) => (StatusCode::OK, "ok").into_response(),
        Err(e) => fngen_err(e),
    }
}
async fn fngen_adc_get_rate(State(state): State<Arc<AppState>>) -> Response {
    let Some(d) = state.waveform_dev.get() else {
        return fngen_unavailable_response();
    };
    match d.adc_get_rate() {
        Ok(v) => Json(U32Body { value: v }).into_response(),
        Err(e) => fngen_err(e),
    }
}
async fn fngen_adc_set_rate(
    State(state): State<Arc<AppState>>,
    Json(req): Json<U32Req>,
) -> Response {
    let Some(d) = state.waveform_dev.get() else {
        return fngen_unavailable_response();
    };
    match d.adc_set_rate(req.value) {
        Ok(()) => (StatusCode::OK, "ok").into_response(),
        Err(e) => fngen_err(e),
    }
}

#[derive(Deserialize)]
struct PlayBuiltinReq {
    shape: String, // "sine"|"square"|"triangle"|"sawtooth"|"dc"
    freq_hz: f64,  // converted to mHz on the wire
    amplitude: u16,
    offset: u16,
    #[serde(default = "default_duty")]
    duty: f32,
}
fn default_duty() -> f32 {
    0.5
}

fn shape_code(name: &str) -> Option<u8> {
    match name.to_ascii_lowercase().as_str() {
        "dc" => Some(1),
        "sine" => Some(2),
        "square" => Some(3),
        "triangle" => Some(4),
        "sawtooth" => Some(5),
        _ => None,
    }
}

async fn fngen_play_builtin(
    State(state): State<Arc<AppState>>,
    axum::extract::Path(channel): axum::extract::Path<String>,
    Json(req): Json<PlayBuiltinReq>,
) -> Response {
    let Some(d) = state.waveform_dev.get() else {
        return fngen_unavailable_response();
    };
    let Some(shape) = shape_code(&req.shape) else {
        return (StatusCode::BAD_REQUEST, format!("unknown shape {:?}", req.shape)).into_response();
    };
    let spec = phycmd_core::WaveBuiltinSpec {
        shape,
        flags: 0,
        duty_x10: (req.duty.clamp(0.0, 1.0) * 1000.0).round() as u16,
        amplitude: req.amplitude,
        offset: req.offset,
        freq_mhz: (req.freq_hz * 1000.0).round() as u32,
        phase_offset_x16: 0,
        _reserved1: 0,
    };
    match d.play_builtin(&channel, &spec) {
        Ok(()) => (StatusCode::OK, "ok").into_response(),
        Err(e) => fngen_err(e),
    }
}

#[derive(Deserialize)]
struct PlayArbitraryReq {
    samples: Vec<i16>,
    sample_rate_hz: u32,
    #[serde(default)]
    loop_count: u16,
}

async fn fngen_play_arbitrary(
    State(state): State<Arc<AppState>>,
    axum::extract::Path(channel): axum::extract::Path<String>,
    Json(req): Json<PlayArbitraryReq>,
) -> Response {
    let Some(d) = state.waveform_dev.get() else {
        return fngen_unavailable_response();
    };
    if req.samples.is_empty() || req.samples.len() > 1024 {
        return (StatusCode::BAD_REQUEST, "samples must be 1..1024").into_response();
    }
    let header = phycmd_core::WaveArbHeader {
        n_samples: req.samples.len() as u16,
        loop_count: req.loop_count,
        sample_rate_hz: req.sample_rate_hz,
    };
    match d.play_arbitrary(&channel, &header, &req.samples) {
        Ok(()) => (StatusCode::OK, "ok").into_response(),
        Err(e) => fngen_err(e),
    }
}

#[derive(Deserialize)]
struct PlayLutReq {
    input_src: String, // "adc" | "din"
    input_arg: u16,
    entries: Vec<i16>,
    #[serde(default)]
    output_mask: u16,
}
fn parse_input_src(s: &str) -> Option<u8> {
    match s.to_ascii_lowercase().as_str() {
        "adc" => Some(1),
        "din" => Some(2),
        _ => None,
    }
}

async fn fngen_play_lut(
    State(state): State<Arc<AppState>>,
    axum::extract::Path(channel): axum::extract::Path<String>,
    Json(req): Json<PlayLutReq>,
) -> Response {
    let Some(d) = state.waveform_dev.get() else {
        return fngen_unavailable_response();
    };
    let Some(src) = parse_input_src(&req.input_src) else {
        return (StatusCode::BAD_REQUEST, "input_src must be \"adc\" or \"din\"").into_response();
    };
    if req.entries.is_empty() || req.entries.len() > 4096 {
        return (StatusCode::BAD_REQUEST, "entries must be 1..4096").into_response();
    }
    let header = phycmd_core::WaveLutSpec {
        input_src: src,
        _reserved0: 0,
        input_arg: req.input_arg,
        n_entries: req.entries.len() as u16,
        output_mask: req.output_mask,
        _reserved1: 0,
    };
    match d.play_lut(&channel, &header, &req.entries) {
        Ok(()) => (StatusCode::OK, "ok").into_response(),
        Err(e) => fngen_err(e),
    }
}

#[derive(Deserialize)]
struct PlayThresholdReq {
    input_src: String,
    input_arg: u16,
    thr_high: u16,
    thr_low: u16,
    val_high: u16,
    val_low: u16,
}

async fn fngen_play_threshold(
    State(state): State<Arc<AppState>>,
    axum::extract::Path(channel): axum::extract::Path<String>,
    Json(req): Json<PlayThresholdReq>,
) -> Response {
    let Some(d) = state.waveform_dev.get() else {
        return fngen_unavailable_response();
    };
    let Some(src) = parse_input_src(&req.input_src) else {
        return (StatusCode::BAD_REQUEST, "input_src must be \"adc\" or \"din\"").into_response();
    };
    let spec = phycmd_core::WaveThresholdSpec {
        input_src: src,
        _reserved0: 0,
        input_arg: req.input_arg,
        thr_high: req.thr_high,
        thr_low: req.thr_low,
        val_high: req.val_high,
        val_low: req.val_low,
        _reserved1: 0,
    };
    match d.play_threshold(&channel, &spec) {
        Ok(()) => (StatusCode::OK, "ok").into_response(),
        Err(e) => fngen_err(e),
    }
}

#[derive(Deserialize)]
struct PlayPulseReq {
    din_bit: u8,
    edge: String, // "rising" | "falling" | "any"
    #[serde(default = "default_active_level")]
    active_level: u8,
    duration_us: u32,
    #[serde(default)]
    cooldown_us: u32,
}
fn default_active_level() -> u8 {
    1
}

async fn fngen_play_pulse_trig(
    State(state): State<Arc<AppState>>,
    axum::extract::Path(channel): axum::extract::Path<String>,
    Json(req): Json<PlayPulseReq>,
) -> Response {
    let Some(d) = state.waveform_dev.get() else {
        return fngen_unavailable_response();
    };
    let edge = match req.edge.to_ascii_lowercase().as_str() {
        "rising" => 1,
        "falling" => 2,
        "any" => 3,
        _ => return (StatusCode::BAD_REQUEST, "edge must be rising|falling|any").into_response(),
    };
    let spec = phycmd_core::WavePulseSpec {
        input_din_bit: req.din_bit,
        edge,
        active_level: req.active_level,
        _reserved0: 0,
        duration_us: req.duration_us,
        cooldown_us: req.cooldown_us,
    };
    match d.play_pulse_trig(&channel, &spec) {
        Ok(()) => (StatusCode::OK, "ok").into_response(),
        Err(e) => fngen_err(e),
    }
}

#[derive(Deserialize)]
struct PlayPidReq {
    input_src: String,
    input_arg: u16,
    sample_rate_hz: u32,
    setpoint: i32,
    kp: f32,
    #[serde(default)]
    ki: f32,
    #[serde(default)]
    kd: f32,
    #[serde(default = "default_pid_min")]
    out_min: u16,
    #[serde(default = "default_pid_max")]
    out_max: u16,
    #[serde(default = "default_pid_iclamp")]
    integral_clamp: i32,
}
fn default_pid_min() -> u16 {
    0
}
fn default_pid_max() -> u16 {
    4095
}
fn default_pid_iclamp() -> i32 {
    1_000_000
}

async fn fngen_play_pid(
    State(state): State<Arc<AppState>>,
    axum::extract::Path(channel): axum::extract::Path<String>,
    Json(req): Json<PlayPidReq>,
) -> Response {
    let Some(d) = state.waveform_dev.get() else {
        return fngen_unavailable_response();
    };
    let Some(src) = parse_input_src(&req.input_src) else {
        return (StatusCode::BAD_REQUEST, "input_src must be \"adc\" or \"din\"").into_response();
    };
    let q16 = |f: f32| (f * 65536.0).round() as i32;
    let spec = phycmd_core::WavePidSpec {
        input_src: src,
        _reserved0: 0,
        input_arg: req.input_arg,
        sample_rate_hz: req.sample_rate_hz,
        setpoint: req.setpoint,
        kp_q16_16: q16(req.kp),
        ki_q16_16: q16(req.ki),
        kd_q16_16: q16(req.kd),
        out_min: req.out_min,
        out_max: req.out_max,
        integral_clamp: req.integral_clamp,
    };
    match d.play_pid(&channel, &spec) {
        Ok(()) => (StatusCode::OK, "ok").into_response(),
        Err(e) => fngen_err(e),
    }
}

/// Get current command (REST API)
async fn get_command(State(state): State<Arc<AppState>>) -> Json<Command> {
    let cmd = state.current_command.read().await;
    Json(cmd.clone())
}

/// Set command (REST API)
async fn set_command(State(state): State<Arc<AppState>>, Json(cmd): Json<Command>) -> Response {
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
async fn set_gpio(State(state): State<Arc<AppState>>, Json(req): Json<GpioRequest>) -> Response {
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
async fn set_dac(State(state): State<Arc<AppState>>, Json(req): Json<DacRequest>) -> Response {
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
    Json(AdcResponse { channels: status.adc })
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
    /// Iso-mode counters. `None` in bulk mode.
    #[serde(skip_serializing_if = "Option::is_none")]
    iso: Option<IsoStatsResponse>,
}

#[derive(Serialize)]
struct IsoStatsResponse {
    iso_in_pkts_ok: u64,
    iso_in_errors: u64,
    iso_in_short: u64,
    iso_in_crc_errors: u64,
    iso_out_pkts_ok: u64,
    iso_out_errors: u64,
    commands_taken: u64,
}

async fn get_rt_stats(State(state): State<Arc<AppState>>) -> Json<RtStatsResponse> {
    let iso = state.iso_stats.get().map(|s| {
        let snap = s.snapshot();
        IsoStatsResponse {
            iso_in_pkts_ok: snap.iso_in_pkts_ok,
            iso_in_errors: snap.iso_in_errors,
            iso_in_short: snap.iso_in_short,
            iso_in_crc_errors: snap.iso_in_crc_errors,
            iso_out_pkts_ok: snap.iso_out_pkts_ok,
            iso_out_errors: snap.iso_out_errors,
            commands_taken: snap.commands_taken,
        }
    });

    let mut response = if let Some(stats) = state.rt_stats.get() {
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
                    if i < 8 {
                        arr[i] = *v;
                    }
                }
                arr
            },
            success_ratio: snap.success_ratio(),
            iso: None,
        }
    } else {
        // Scheduler not yet registered (service still booting)
        RtStatsResponse {
            tick_count: 0,
            tick_ok: 0,
            missed_ticks: 0,
            transport_errors: 0,
            latency_max_us: 0,
            mean_latency_us: 0.0,
            jitter_min_us: 0,
            jitter_max_us: 0,
            mean_abs_jitter_us: 0.0,
            jitter_histogram: [0; 9],
            jitter_buckets_us: [0; 8],
            success_ratio: 1.0,
            iso: None,
        }
    };
    response.iso = iso;
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
