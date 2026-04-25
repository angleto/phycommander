// SPDX-License-Identifier: AGPL-3.0-or-later
// SPDX-FileCopyrightText: 2014-2026 Angelo Leto <angelo@leto.blue>

use std::sync::{
    atomic::{AtomicU16, Ordering},
    Arc,
};

use anyhow::Context as _;
use axum::{
    extract::{
        ws::{WebSocket, WebSocketUpgrade},
        State,
    },
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use tokio::sync::{broadcast, RwLock};
use tower_http::cors::CorsLayer;
use tracing::{info, warn};

use crate::{
    adc_capture::{self, SharedAdcRing},
    protocol::{Command, Status},
};

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
    /// Full-rate ADC sample ring. Fed by the bus-drain task in
    /// `main.rs`, read by the `/api/adc/capture` handler.
    pub adc_ring: SharedAdcRing,
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
            adc_ring: adc_capture::new_shared(),
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
        .route("/api/adc/capture", get(adc_capture_handler))
        .route("/api/sysinfo", get(get_sysinfo))
        .route("/api/version", get(get_version))
        .route("/api/health", get(get_health))
        .route("/metrics", get(get_metrics))
        .route("/api/rt_stats", get(get_rt_stats))
        .route("/api/reset_errors", post(reset_errors))
        .route("/api/reset_telemetry", post(reset_telemetry))
        .route("/api/telemetry/detail", get(get_telemetry_detail))
        .route("/api/telemetry/detail", post(set_telemetry_detail))
        .route("/api/waveform", get(get_waveforms))
        .route("/api/waveform/:channel", post(set_waveform))
        .route("/api/waveform/:channel", axum::routing::delete(disable_waveform))
        .route("/api/waveform/coexistence", get(waveform_coexistence))
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

/// GET /api/waveform/coexistence — reports, per shared channel (DAC0,
/// DAC1, PWM0, PWM1), whether the host-side `WaveformBank` generator
/// and the firmware on-chip function generator are both active. When
/// both are active the firmware wins (see `waveforms.rs` module doc);
/// this endpoint surfaces the overlap so the dashboard can draw a
/// warning badge on the affected channel row.
#[derive(Serialize)]
struct ChannelCoexistence {
    host_enabled: bool,
    firmware_active: bool,
    firmware_shape: &'static str,
    overridden_by_firmware: bool,
}
#[derive(Serialize)]
struct CoexistenceReport {
    dac0: ChannelCoexistence,
    dac1: ChannelCoexistence,
    pwm0: ChannelCoexistence,
    pwm1: ChannelCoexistence,
}
async fn waveform_coexistence(State(state): State<Arc<AppState>>) -> Response {
    let (Some(w), Some(d)) = (state.waveforms.get(), state.waveform_dev.get()) else {
        return (StatusCode::SERVICE_UNAVAILABLE, "coexistence report only available in iso mode")
            .into_response();
    };

    // Host-side: single cheap RwLock read per channel.
    let snap = w.snapshot();
    // Firmware-side: one EP0 control transfer per channel (4 total).
    // Returning a BUSY error for a channel leaves it reported as
    // `firmware_active = false`, which is the safe default: the UI
    // won't draw a spurious warning if the firmware is momentarily
    // reconnecting.
    let firmware_state = |name: &'static str| -> (bool, &'static str) {
        match d.state(name) {
            Ok(st) => {
                let active = st.shape != 0; // 0 == SHAPE_OFF
                (active, st.shape_name)
            }
            Err(_) => (false, "unavailable"),
        }
    };
    let compose = |host_enabled: bool, fw: (bool, &'static str)| ChannelCoexistence {
        host_enabled,
        firmware_active: fw.0,
        firmware_shape: fw.1,
        overridden_by_firmware: host_enabled && fw.0,
    };

    let report = CoexistenceReport {
        dac0: compose(snap.dac0.enabled, firmware_state("dac0")),
        dac1: compose(snap.dac1.enabled, firmware_state("dac1")),
        pwm0: compose(snap.pwm0.enabled, firmware_state("pwm0")),
        pwm1: compose(snap.pwm1.enabled, firmware_state("pwm1")),
    };
    Json(report).into_response()
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
    // The firmware gates DAC writes on FLAG_DAC_ENABLE. Silently
    // skipping the write when a caller only asks for a DAC value was
    // a real footgun: `curl /api/dac/set` would update the cached
    // command but the firmware would ignore it because
    // dac_enable was still false. Force the flag true here so every
    // call to /api/dac/set actually drives the pin.
    cmd.flags.dac_enable = true;

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

#[derive(Deserialize)]
struct AdcCaptureQuery {
    /// Return only samples with seq >= this cursor. Omit (or 0) on first call.
    #[serde(default)]
    since: u64,
    /// Cap on how many samples to return; defaults to the ring capacity.
    #[serde(default)]
    max: Option<usize>,
}

#[derive(Serialize)]
struct AdcCaptureResponse {
    /// Nominal sampling rate (Hz). Matches the iso IN microframe cadence.
    sample_rate_hz: u32,
    /// Oldest + newest seq currently held by the ring (for client
    /// liveness diagnostics).
    ring_first_seq: u64,
    ring_last_seq: u64,
    /// Seq of the first sample in the returned payload (or 0 if empty).
    first_seq: u64,
    /// Seq of the last sample in the returned payload (or 0 if empty).
    last_seq: u64,
    /// Samples in chronological order, each a 10-field record. Kept
    /// as parallel arrays of u16 so JSON overhead stays minimal.
    adc: [Vec<u16>; 8],
    din: Vec<u16>,
    dout: Vec<u16>,
}

async fn adc_capture_handler(
    State(state): State<Arc<AppState>>,
    axum::extract::Query(q): axum::extract::Query<AdcCaptureQuery>,
) -> Json<AdcCaptureResponse> {
    let max = q.max.unwrap_or(adc_capture::CAPACITY);
    let (samples, first, last) = {
        let ring = state.adc_ring.lock();
        let (rf, rl) = ring.span();
        (ring.snapshot_since(q.since, max), rf, rl)
    };

    let mut adc_cols: [Vec<u16>; 8] = Default::default();
    let mut din = Vec::with_capacity(samples.len());
    let mut dout = Vec::with_capacity(samples.len());
    for s in &samples {
        for (i, v) in s.adc.iter().enumerate() {
            adc_cols[i].push(*v);
        }
        din.push(s.din);
        dout.push(s.dout);
    }

    let (first_seq, last_seq) = if samples.is_empty() {
        (0, 0)
    } else {
        (samples.first().unwrap().seq, samples.last().unwrap().seq)
    };

    Json(AdcCaptureResponse {
        sample_rate_hz: 8000,
        ring_first_seq: first,
        ring_last_seq: last,
        first_seq,
        last_seq,
        adc: adc_cols,
        din,
        dout,
    })
}

/// Read a fresh system telemetry snapshot (hwmon sensors, CPU freq, load,
/// memory, uptime). This reads directly from sysfs/procfs on every call —
/// cheap enough for a REST endpoint polled every few seconds.
async fn get_sysinfo() -> Json<crate::sysinfo::SysInfoSnapshot> {
    Json(crate::sysinfo::read_snapshot())
}

/// Version / build identity. Returns the Cargo package version
/// (authoritative for API compatibility), the short git hash of the
/// commit the binary was built from (with `-dirty` suffix when the
/// working tree wasn't clean), and the UTC build timestamp. Makes
/// deploy-diagnosis ("is this the binary we just shipped, or a stale
/// one?") trivial without touching systemd or the filesystem.
#[derive(Serialize)]
struct VersionResponse {
    version: &'static str,
    git_hash: &'static str,
    build_date: &'static str,
    rustc_target: &'static str,
}

async fn get_version() -> Json<VersionResponse> {
    Json(VersionResponse {
        version: env!("CARGO_PKG_VERSION"),
        git_hash: env!("PHYCMD_GIT_HASH"),
        build_date: env!("PHYCMD_BUILD_DATE"),
        rustc_target: env!("PHYCMD_TARGET"),
    })
}

/// Liveness / readiness probe for systemd, prometheus blackbox,
/// reverse-proxy health checks, etc. Returns 200 only when the iso
/// transport is actively moving packets (iso_in_rate_hz >= 1 kHz),
/// the firmware reports usb_configured, and the transport isn't
/// mid-reconnect. Anything else returns 503 with a diagnostic JSON
/// body so operators can tell at a glance which check tripped.
///
/// The 1 kHz lower bound is deliberately an order of magnitude
/// below the nominal 8 kHz so the probe tolerates brief dips (EMA
/// warmup right after a reset_telemetry, single dropped URB) but
/// still catches a genuine transport stall.
#[derive(Serialize)]
struct HealthResponse {
    status: &'static str,
    checks: HealthChecks,
}

#[derive(Serialize, Clone, Copy, Debug)]
pub struct HealthChecks {
    pub iso_rate_ok: bool,
    pub iso_in_rate_hz: f32,
    pub usb_configured: bool,
    pub reconnecting: bool,
    pub transport_mode: &'static str,
}

impl HealthChecks {
    /// Overall health verdict: true iff every individual check
    /// passes. Kept as a method rather than a boolean field so the
    /// JSON response shows the per-check breakdown and callers
    /// never disagree on "what does healthy mean?".
    pub fn healthy(&self) -> bool {
        self.iso_rate_ok && self.usb_configured && !self.reconnecting
    }
}

/// Compute the current health view from the shared `AppState`.
/// Shared between the `/api/health` HTTP handler and the systemd
/// watchdog task in `main.rs`; there must be exactly one definition
/// of "healthy" in the binary or systemd and the probe will drift
/// apart.
pub async fn compute_health(state: &AppState) -> HealthChecks {
    let (iso_rate_hz, reconnecting, mode) = match (state.iso_stats.get(), state.iso_transport.get())
    {
        (Some(stats), Some(t)) => {
            let snap = stats.snapshot();
            (snap.iso_in_rate_hz, t.is_reconnecting(), "iso")
        }
        // Non-iso deployment (bulk or serial): the rate estimate
        // lives in a different path and we can't use it here. Fall
        // back to usb_configured only.
        _ => (f32::NAN, false, "non-iso"),
    };

    let usb_configured = state.current_status.read().await.flags.usb_configured;

    // 1 kHz threshold: an order of magnitude under nominal 8 kHz, so
    // transient EMA warmup / brief URB hiccups don't flap the probe,
    // but a genuine transport stall (rate -> 0) trips it quickly.
    // For non-iso mode (NaN) we skip this check.
    let iso_rate_ok = iso_rate_hz.is_nan() || iso_rate_hz >= 1000.0;

    HealthChecks {
        iso_rate_ok,
        iso_in_rate_hz: if iso_rate_hz.is_nan() {
            0.0
        } else {
            iso_rate_hz
        },
        usb_configured,
        reconnecting,
        transport_mode: mode,
    }
}

async fn get_health(
    State(state): State<Arc<AppState>>,
) -> (axum::http::StatusCode, Json<HealthResponse>) {
    use axum::http::StatusCode;

    let checks = compute_health(&state).await;
    let healthy = checks.healthy();

    let body = HealthResponse { status: if healthy { "ok" } else { "degraded" }, checks };

    let code = if healthy {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };
    (code, Json(body))
}

/// Prometheus exposition format (`text/plain; version=0.0.4`).
/// Returns the same counters as `/api/rt_stats` but in a shape that
/// any stock prometheus/grafana-agent can scrape without a custom
/// collector. Label-less; this is one single instance per host.
async fn get_metrics(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let snap = snapshot_rt_stats(&state);

    let mut out = String::with_capacity(2048);
    use std::fmt::Write;

    let _ = writeln!(out, "# HELP phycmd_tick_total Scheduler tick count since startup.");
    let _ = writeln!(out, "# TYPE phycmd_tick_total counter");
    let _ = writeln!(out, "phycmd_tick_total {}", snap.tick_count);

    let _ = writeln!(out, "# HELP phycmd_tick_ok_total Scheduler ticks completed without error.");
    let _ = writeln!(out, "# TYPE phycmd_tick_ok_total counter");
    let _ = writeln!(out, "phycmd_tick_ok_total {}", snap.tick_ok);

    let _ = writeln!(out, "# HELP phycmd_missed_ticks_total Scheduler deadlines missed.");
    let _ = writeln!(out, "# TYPE phycmd_missed_ticks_total counter");
    let _ = writeln!(out, "phycmd_missed_ticks_total {}", snap.missed_ticks);

    let _ = writeln!(out, "# HELP phycmd_transport_errors_total Transport-layer errors.");
    let _ = writeln!(out, "# TYPE phycmd_transport_errors_total counter");
    let _ = writeln!(out, "phycmd_transport_errors_total {}", snap.transport_errors);

    let _ = writeln!(out, "# HELP phycmd_jitter_us_mean_abs Mean absolute scheduler jitter (us).");
    let _ = writeln!(out, "# TYPE phycmd_jitter_us_mean_abs gauge");
    let _ = writeln!(out, "phycmd_jitter_us_mean_abs {}", snap.mean_abs_jitter_us);

    let _ = writeln!(out, "# HELP phycmd_jitter_us_max Maximum scheduler jitter since reset (us).");
    let _ = writeln!(out, "# TYPE phycmd_jitter_us_max gauge");
    let _ = writeln!(out, "phycmd_jitter_us_max {}", snap.jitter_max_us);

    let _ =
        writeln!(out, "# HELP phycmd_latency_us_mean Mean scheduler wake-to-done latency (us).");
    let _ = writeln!(out, "# TYPE phycmd_latency_us_mean gauge");
    let _ = writeln!(out, "phycmd_latency_us_mean {}", snap.mean_latency_us);

    // Jitter histogram as a proper Prometheus histogram. Bucket
    // bounds are the upper edges (exclusive) of each band in us.
    let _ = writeln!(out, "# HELP phycmd_jitter_us Scheduler jitter distribution (us, absolute).");
    let _ = writeln!(out, "# TYPE phycmd_jitter_us histogram");
    let mut cumulative: u64 = 0;
    for (i, &edge) in snap.jitter_buckets_us.iter().enumerate() {
        cumulative += snap.jitter_histogram[i] as u64;
        let _ = writeln!(out, "phycmd_jitter_us_bucket{{le=\"{}\"}} {}", edge, cumulative);
    }
    cumulative += snap.jitter_histogram[snap.jitter_buckets_us.len()] as u64;
    let _ = writeln!(out, "phycmd_jitter_us_bucket{{le=\"+Inf\"}} {}", cumulative);
    let _ = writeln!(out, "phycmd_jitter_us_count {}", cumulative);

    if let Some(iso) = &snap.iso {
        let _ = writeln!(out, "# HELP phycmd_iso_in_pkts_ok_total Iso IN packets decoded OK.");
        let _ = writeln!(out, "# TYPE phycmd_iso_in_pkts_ok_total counter");
        let _ = writeln!(out, "phycmd_iso_in_pkts_ok_total {}", iso.iso_in_pkts_ok);

        let _ = writeln!(out, "# HELP phycmd_iso_in_errors_total Iso IN transport errors.");
        let _ = writeln!(out, "# TYPE phycmd_iso_in_errors_total counter");
        let _ = writeln!(out, "phycmd_iso_in_errors_total {}", iso.iso_in_errors);

        let _ = writeln!(out, "# HELP phycmd_iso_in_short_total Iso IN packets < 64 bytes.");
        let _ = writeln!(out, "# TYPE phycmd_iso_in_short_total counter");
        let _ = writeln!(out, "phycmd_iso_in_short_total {}", iso.iso_in_short);

        let _ = writeln!(out, "# HELP phycmd_iso_in_crc_errors_total Iso IN CRC failures.");
        let _ = writeln!(out, "# TYPE phycmd_iso_in_crc_errors_total counter");
        let _ = writeln!(out, "phycmd_iso_in_crc_errors_total {}", iso.iso_in_crc_errors);

        let _ = writeln!(out, "# HELP phycmd_iso_out_pkts_ok_total Iso OUT packets sent OK.");
        let _ = writeln!(out, "# TYPE phycmd_iso_out_pkts_ok_total counter");
        let _ = writeln!(out, "phycmd_iso_out_pkts_ok_total {}", iso.iso_out_pkts_ok);

        let _ = writeln!(out, "# HELP phycmd_iso_in_rate_hz Smoothed iso IN packet rate (Hz).");
        let _ = writeln!(out, "# TYPE phycmd_iso_in_rate_hz gauge");
        let _ = writeln!(out, "phycmd_iso_in_rate_hz {}", iso.iso_in_rate_hz);
    }

    ([("content-type", "text/plain; version=0.0.4; charset=utf-8")], out)
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
    /// Server-side smoothed estimate of iso IN packets per second.
    /// Authoritative rate: compute client-side deltas if you need
    /// finer granularity but expect browser-clock jitter (±5%) on a
    /// link that is actually rock-steady.
    iso_in_rate_hz: f32,
}

/// Build an [`RtStatsResponse`] from the live scheduler + iso
/// counters. Shared between the HTTP `/api/rt_stats` handler and
/// the WebSocket push loop, so both endpoints stay bit-for-bit
/// identical.
fn snapshot_rt_stats(state: &AppState) -> RtStatsResponse {
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
            iso_in_rate_hz: snap.iso_in_rate_hz,
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
    response
}

async fn get_rt_stats(State(state): State<Arc<AppState>>) -> Json<RtStatsResponse> {
    Json(snapshot_rt_stats(&state))
}

/// WebSocket handler
async fn websocket_handler(
    ws: WebSocketUpgrade,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    ws.on_upgrade(|socket| websocket_connection(socket, state))
}

/// Handle WebSocket connection.
///
/// Message wire formats sent to the client:
///   - **Status** (pre-existing): raw `Status` JSON, no envelope. Emitted on every broadcast tick
///     from the device (~250 Hz typical). Consumers that just need live GPIO/ADC should look at
///     these.
///   - **RT stats** (new): `{"type":"rt_stats","data":{...}}` emitted at 1 Hz. Eliminates the
///     dashboard's HTTP poll on `/api/rt_stats`; any future consumer needing scheduler or iso
///     counters can subscribe to this WS stream instead.
/// Legacy clients that naively `JSON.parse` and treat everything as
/// `Status` must guard on the `type` field — see
/// `physerver/static/index.html` for the reference pattern.
async fn websocket_connection(mut socket: WebSocket, state: Arc<AppState>) {
    info!("WebSocket client connected");

    let mut rx = state.status_broadcast.subscribe();
    let mut stats_tick = tokio::time::interval(std::time::Duration::from_secs(1));
    // `Delay` missed-tick policy: if the task is slow (e.g. the
    // client is back-pressuring) we don't want to fire a flood of
    // stats updates to catch up — skip stale ticks.
    stats_tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    loop {
        tokio::select! {
            // Receive status updates and send to client (raw Status JSON).
            status = rx.recv() => {
                match status {
                    Ok(status) => {
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

            // Periodic rt_stats push (1 Hz). Wrapped in a typed
            // envelope so the client can distinguish it from the
            // Status stream above.
            _ = stats_tick.tick() => {
                let snap = snapshot_rt_stats(&state);
                // Minimal envelope: { "type": "rt_stats", "data": {...} }
                let envelope = serde_json::json!({ "type": "rt_stats", "data": snap });
                let json = envelope.to_string();
                if socket.send(axum::extract::ws::Message::Text(json)).await.is_err() {
                    break;
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

/// Per-request middleware that enforces a bearer token on every
/// non-probe endpoint when [`AuthConfig::bearer_token`] is set.
///
/// `/api/health` and `/metrics` are intentionally left open — those
/// are consumed by systemd, prometheus, k8s probes, reverse proxies,
/// etc. Requiring a token there would force every operator to
/// configure auth into their probe machinery for zero security gain
/// (the endpoints only expose counters and a pass/fail bit that any
/// attacker could also get by watching `/api/status` succeed).
async fn bearer_auth_middleware(
    axum::extract::State(expected): axum::extract::State<Arc<Option<String>>>,
    req: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    use axum::http::StatusCode;

    // Off when bearer_token isn't configured — the Arc is always
    // cheap to clone and this is the fast path.
    let Some(expected) = expected.as_ref() else {
        return next.run(req).await;
    };

    // Probe endpoints: always allowed.
    let path = req.uri().path();
    if matches!(path, "/api/health" | "/metrics") {
        return next.run(req).await;
    }

    let authorized = req
        .headers()
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.strip_prefix("Bearer "))
        .map(|t| constant_time_eq(t.as_bytes(), expected.as_bytes()))
        .unwrap_or(false);

    if authorized {
        next.run(req).await
    } else {
        (StatusCode::UNAUTHORIZED, [("www-authenticate", "Bearer")], "unauthorized\n")
            .into_response()
    }
}

/// Timing-safe token comparison — if we did a plain `==` an attacker
/// could learn a correct prefix by measuring request latency. This
/// isn't a paranoia measure: with a high-rate LAN attacker and low
/// RTT, even a few ns of timing leak is exploitable over a few
/// minutes of brute force.
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff: u8 = 0;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

/// Start the web server.
///
/// Behaviour:
///   - No TLS, no auth: plain HTTP with `axum::serve` (pre-2.x behaviour).
///   - Auth only: wrap the router in a bearer-token middleware; still HTTP.
///   - TLS only: serve HTTPS via `axum_server` + rustls (browser warns on self-signed cert but
///     WebSocket / fetch work fine on LAN).
///   - Both: HTTPS + bearer.
pub async fn start_web_server(
    state: Arc<AppState>,
    port: u16,
    auth: crate::config::AuthConfig,
) -> anyhow::Result<()> {
    let token = Arc::new(auth.bearer_token.clone());
    let has_auth = token.is_some();

    let mut app = create_router(state);
    if has_auth {
        app = app.layer(axum::middleware::from_fn_with_state(
            Arc::clone(&token),
            bearer_auth_middleware,
        ));
        info!("HTTP bearer-token auth is active (probes on /api/health and /metrics remain open)");
    }

    let addr: std::net::SocketAddr = format!("0.0.0.0:{}", port).parse()?;

    match (auth.tls_cert_file, auth.tls_key_file) {
        (Some(cert), Some(key)) => {
            info!("Starting web server on https://{}", addr);
            let config = axum_server::tls_rustls::RustlsConfig::from_pem_file(&cert, &key)
                .await
                .context("Failed to load TLS cert/key")?;
            axum_server::bind_rustls(addr, config).serve(app.into_make_service()).await?;
        }
        (None, None) => {
            info!("Starting web server on http://{}", addr);
            let listener = tokio::net::TcpListener::bind(&addr).await?;
            axum::serve(listener, app).await?;
        }
        _ => anyhow::bail!(
            "auth.tls_cert_file and auth.tls_key_file must both be set or both be absent"
        ),
    }

    Ok(())
}
