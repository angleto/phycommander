use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

/// Initialize logging and telemetry
pub fn init() {
    tracing_subscriber::registry()
        .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| {
            "physerver=debug,tower_http=debug,axum=debug".into()
        }))
        .with(tracing_subscriber::fmt::layer())
        .init();
}

/// Initialize with JSON output (for production)
pub fn init_json() {
    tracing_subscriber::registry()
        .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| {
            "physerver=info,tower_http=info".into()
        }))
        .with(tracing_subscriber::fmt::layer().json())
        .init();
}
