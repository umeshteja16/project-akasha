use akasha_core::LogFormat;
use tracing_subscriber::{EnvFilter, fmt, prelude::*};

/// Install the global tracing subscriber. Filter with `RUST_LOG` (default `info`).
pub fn init(format: LogFormat) {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info,tower_http=info,sqlx=warn"));
    let registry = tracing_subscriber::registry().with(filter);
    match format {
        LogFormat::Pretty => registry.with(fmt::layer()).init(),
        LogFormat::Json => registry.with(fmt::layer().json()).init(),
    }
}
