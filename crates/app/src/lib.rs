//! The Akasha server: HTTP routes, application state and process entry points.
//!
//! `main.rs` only parses the command line; everything testable lives here.

pub mod error;
pub mod routes;
pub mod state;
pub mod telemetry;

use std::time::Duration;

use akasha_core::Config;
use anyhow::Context;
use axum::{
    Router,
    http::{HeaderName, StatusCode},
};
use tower_http::{
    catch_panic::CatchPanicLayer,
    request_id::{MakeRequestUuid, PropagateRequestIdLayer, SetRequestIdLayer},
    timeout::TimeoutLayer,
    trace::TraceLayer,
};
use utoipa::OpenApi;

pub use state::AppState;

const REQUEST_ID: HeaderName = HeaderName::from_static("x-request-id");

/// Build the full HTTP application.
pub fn app(state: AppState) -> Router {
    routes::router()
        .with_state(state)
        .layer(CatchPanicLayer::new())
        .layer(TimeoutLayer::with_status_code(
            StatusCode::REQUEST_TIMEOUT,
            Duration::from_secs(30),
        ))
        .layer(TraceLayer::new_for_http())
        .layer(PropagateRequestIdLayer::new(REQUEST_ID))
        .layer(SetRequestIdLayer::new(REQUEST_ID, MakeRequestUuid))
}

/// The OpenAPI document, pretty-printed. The web client is generated from this.
pub fn openapi_json() -> anyhow::Result<String> {
    Ok(routes::ApiDoc::openapi().to_pretty_json()?)
}

pub async fn run_migrate(config: Config) -> anyhow::Result<()> {
    let pool = akasha_db::connect(&config.database_url, 1)
        .await
        .context("connecting to database")?;
    akasha_db::migrate(&pool).await?;
    Ok(())
}

pub async fn run_serve(config: Config) -> anyhow::Result<()> {
    let pool = akasha_db::connect(&config.database_url, config.db_max_connections)
        .await
        .context("connecting to database")?;
    akasha_db::migrate(&pool).await?;

    let listener = tokio::net::TcpListener::bind(&config.bind_addr)
        .await
        .with_context(|| format!("binding {}", config.bind_addr))?;
    tracing::info!(addr = %config.bind_addr, "listening");

    axum::serve(listener, app(AppState::new(pool)))
        .with_graceful_shutdown(shutdown_signal())
        .await?;
    tracing::info!("shut down cleanly");
    Ok(())
}

async fn shutdown_signal() {
    let ctrl_c = async {
        if let Err(err) = tokio::signal::ctrl_c().await {
            tracing::error!(%err, "failed to listen for ctrl-c");
        }
    };
    #[cfg(unix)]
    let terminate = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut sig) => {
                sig.recv().await;
            }
            Err(err) => tracing::error!(%err, "failed to listen for SIGTERM"),
        }
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        () = ctrl_c => {},
        () = terminate => {},
    }
    tracing::info!("shutdown signal received");
}
