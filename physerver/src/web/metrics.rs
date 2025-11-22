/// Prometheus-compatible metrics endpoint

use axum::{extract::State, response::IntoResponse};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use super::AppState;

// Global metrics counters
static COMMANDS_SENT: AtomicU64 = AtomicU64::new(0);
static STATUS_RECEIVED: AtomicU64 = AtomicU64::new(0);
static ERRORS_TOTAL: AtomicU64 = AtomicU64::new(0);
static API_REQUESTS: AtomicU64 = AtomicU64::new(0);
static WEBSOCKET_CONNECTIONS: AtomicU64 = AtomicU64::new(0);

pub fn increment_commands() {
    COMMANDS_SENT.fetch_add(1, Ordering::Relaxed);
}

pub fn increment_status() {
    STATUS_RECEIVED.fetch_add(1, Ordering::Relaxed);
}

pub fn increment_errors() {
    ERRORS_TOTAL.fetch_add(1, Ordering::Relaxed);
}

pub fn increment_api_requests() {
    API_REQUESTS.fetch_add(1, Ordering::Relaxed);
}

pub fn increment_websocket_connections() {
    WEBSOCKET_CONNECTIONS.fetch_add(1, Ordering::Relaxed);
}

pub fn decrement_websocket_connections() {
    WEBSOCKET_CONNECTIONS.fetch_sub(1, Ordering::Relaxed);
}

/// Prometheus-compatible metrics endpoint
pub async fn metrics_handler(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let status = state.current_status.read().await;

    let metrics = format!(
        r#"# HELP physerver_commands_total Total number of commands sent to device
# TYPE physerver_commands_total counter
physerver_commands_total {}

# HELP physerver_status_received_total Total number of status messages received
# TYPE physerver_status_received_total counter
physerver_status_received_total {}

# HELP physerver_errors_total Total number of communication errors
# TYPE physerver_errors_total counter
physerver_errors_total {}

# HELP physerver_device_errors_total Total errors reported by device
# TYPE physerver_device_errors_total counter
physerver_device_errors_total {}

# HELP physerver_api_requests_total Total HTTP API requests
# TYPE physerver_api_requests_total counter
physerver_api_requests_total {}

# HELP physerver_websocket_connections Current WebSocket connections
# TYPE physerver_websocket_connections gauge
physerver_websocket_connections {}

# HELP physerver_device_loop_time_microseconds Device loop time in microseconds
# TYPE physerver_device_loop_time_microseconds gauge
physerver_device_loop_time_microseconds {}

# HELP physerver_device_uptime_milliseconds Device uptime in milliseconds
# TYPE physerver_device_uptime_milliseconds gauge
physerver_device_uptime_milliseconds {}

# HELP physerver_adc_value ADC channel readings
# TYPE physerver_adc_value gauge
physerver_adc_value{{channel="0"}} {}
physerver_adc_value{{channel="1"}} {}
physerver_adc_value{{channel="2"}} {}
physerver_adc_value{{channel="3"}} {}
physerver_adc_value{{channel="4"}} {}
physerver_adc_value{{channel="5"}} {}
physerver_adc_value{{channel="6"}} {}
physerver_adc_value{{channel="7"}} {}

# HELP physerver_device_connected Device connection status (1=connected, 0=disconnected)
# TYPE physerver_device_connected gauge
physerver_device_connected {}
"#,
        COMMANDS_SENT.load(Ordering::Relaxed),
        STATUS_RECEIVED.load(Ordering::Relaxed),
        ERRORS_TOTAL.load(Ordering::Relaxed),
        status.error_count,
        API_REQUESTS.load(Ordering::Relaxed),
        WEBSOCKET_CONNECTIONS.load(Ordering::Relaxed),
        status.loop_time_us,
        status.uptime_ms,
        status.adc[0],
        status.adc[1],
        status.adc[2],
        status.adc[3],
        status.adc[4],
        status.adc[5],
        status.adc[6],
        status.adc[7],
        if status.flags.usb_configured { 1 } else { 0 },
    );

    (
        [("Content-Type", "text/plain; version=0.0.4")],
        metrics,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metric_counters() {
        increment_commands();
        increment_status();
        increment_errors();
        increment_api_requests();

        assert!(COMMANDS_SENT.load(Ordering::Relaxed) > 0);
        assert!(STATUS_RECEIVED.load(Ordering::Relaxed) > 0);
        assert!(ERRORS_TOTAL.load(Ordering::Relaxed) > 0);
        assert!(API_REQUESTS.load(Ordering::Relaxed) > 0);
    }

    #[test]
    fn test_websocket_counter() {
        let initial = WEBSOCKET_CONNECTIONS.load(Ordering::Relaxed);
        increment_websocket_connections();
        assert_eq!(
            WEBSOCKET_CONNECTIONS.load(Ordering::Relaxed),
            initial + 1
        );
        decrement_websocket_connections();
        assert_eq!(WEBSOCKET_CONNECTIONS.load(Ordering::Relaxed), initial);
    }
}
