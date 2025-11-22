/// Health check endpoint for monitoring and load balancers

use axum::{extract::State, http::StatusCode, response::IntoResponse, Json};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::SystemTime;

use super::AppState;

#[derive(Debug, Serialize, Deserialize)]
pub struct HealthResponse {
    pub status: String,
    pub version: String,
    pub uptime_seconds: u64,
    pub checks: HealthChecks,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct HealthChecks {
    pub device_connected: bool,
    pub ipc_available: bool,
    pub web_server: bool,
    pub last_status_age_ms: Option<u64>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ReadinessResponse {
    pub ready: bool,
    pub reason: Option<String>,
}

static START_TIME: once_cell::sync::Lazy<SystemTime> =
    once_cell::sync::Lazy::new(SystemTime::now);

/// Health check endpoint - always returns 200 if server is running
pub async fn health_handler() -> impl IntoResponse {
    (StatusCode::OK, Json(serde_json::json!({
        "status": "ok"
    })))
}

/// Detailed health check with system status
pub async fn health_detailed(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let uptime = START_TIME
        .elapsed()
        .unwrap_or_default()
        .as_secs();

    // Check last status update time
    let status = state.current_status.read().await;
    let last_update = if status.uptime_ms > 0 {
        Some(0) // In real impl, track last update timestamp
    } else {
        None
    };

    let health = HealthResponse {
        status: "healthy".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        uptime_seconds: uptime,
        checks: HealthChecks {
            device_connected: status.flags.usb_configured || status.uptime_ms > 0,
            ipc_available: true, // Would check shared memory
            web_server: true,
            last_status_age_ms: last_update,
        },
    };

    (StatusCode::OK, Json(health))
}

/// Readiness check for load balancers
/// Returns 200 only if system is ready to accept traffic
pub async fn readiness_handler(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let status = state.current_status.read().await;

    // Check if device is connected and responding
    let ready = status.flags.usb_configured || status.uptime_ms > 0;

    if ready {
        (
            StatusCode::OK,
            Json(ReadinessResponse {
                ready: true,
                reason: None,
            }),
        )
    } else {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ReadinessResponse {
                ready: false,
                reason: Some("Device not connected".to_string()),
            }),
        )
    }
}

/// Liveness check for Kubernetes/orchestration
/// Returns 200 if server process is alive
pub async fn liveness_handler() -> impl IntoResponse {
    (StatusCode::OK, "alive")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_health_response_serialization() {
        let health = HealthResponse {
            status: "healthy".to_string(),
            version: "1.0.0".to_string(),
            uptime_seconds: 123,
            checks: HealthChecks {
                device_connected: true,
                ipc_available: true,
                web_server: true,
                last_status_age_ms: Some(50),
            },
        };

        let json = serde_json::to_string(&health).unwrap();
        assert!(json.contains("healthy"));
        assert!(json.contains("1.0.0"));
    }

    #[test]
    fn test_readiness_response() {
        let ready = ReadinessResponse {
            ready: true,
            reason: None,
        };

        let json = serde_json::to_value(&ready).unwrap();
        assert_eq!(json["ready"], true);
        assert!(json["reason"].is_null());
    }
}
