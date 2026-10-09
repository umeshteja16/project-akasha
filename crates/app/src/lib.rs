//! The Akasha server: HTTP routes, application state and process entry points.
//!
//! `main.rs` only parses the command line; everything testable lives here.

pub mod auth;
pub mod error;
pub mod extract;
pub mod files;
pub mod jobs;
pub mod rate_limit;
pub mod routes;
pub mod state;
pub mod telemetry;

use std::net::SocketAddr;

use akasha_core::Config;
use anyhow::Context;
use axum::{Router, http::HeaderName};
use tower_http::{
    catch_panic::CatchPanicLayer,
    request_id::{MakeRequestUuid, PropagateRequestIdLayer, SetRequestIdLayer},
    trace::TraceLayer,
};
use utoipa::OpenApi;

pub use state::AppState;

const REQUEST_ID: HeaderName = HeaderName::from_static("x-request-id");

/// Build the full HTTP application.
pub fn app(state: AppState) -> Router {
    routes::router(&state)
        .with_state(state)
        .layer(CatchPanicLayer::new())
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

/// Serve the HTTP API; with `with_worker`, also run the background worker in this
/// process (single-box installs). Both stop on SIGINT/SIGTERM: the server drains
/// requests, the worker finishes in-flight jobs within its grace period.
pub async fn run_serve(config: Config, with_worker: bool) -> anyhow::Result<()> {
    let pool = akasha_db::connect(&config.database_url, config.db_max_connections)
        .await
        .context("connecting to database")?;
    akasha_db::migrate(&pool).await?;
    let storage = akasha_storage::Storage::from_config(&config).context("opening storage")?;

    let listener = tokio::net::TcpListener::bind(&config.bind_addr)
        .await
        .with_context(|| format!("binding {}", config.bind_addr))?;
    tracing::info!(addr = %config.bind_addr, with_worker, "listening");

    let state = AppState::new(pool, config, storage);
    rate_limit::spawn_cleanup(state.auth_limiter.clone());
    let stop = shutdown_trigger();

    let worker = if with_worker {
        let worker = jobs::worker(jobs::JobContext::from(&state), &state.config)?;
        Some(tokio::spawn(worker.run(wait_for(stop.clone()))))
    } else {
        tracing::warn!("no worker in this process: run `akasha worker` (or `serve --with-worker`)");
        None
    };

    // Connect info gives handlers and the rate limiter the client's address.
    let service = app(state).into_make_service_with_connect_info::<SocketAddr>();
    let served = axum::serve(listener, service)
        .with_graceful_shutdown(wait_for(stop.clone()))
        .await;
    if let Some(worker) = worker {
        // The server may have failed without a signal; stop the worker either way.
        let _ = stop.send(true);
        worker.await.context("worker task")??;
    }
    served?;
    tracing::info!("shut down cleanly");
    Ok(())
}

/// Run only the background worker (scale it separately from the API).
pub async fn run_worker(config: Config) -> anyhow::Result<()> {
    let pool = akasha_db::connect(&config.database_url, config.db_max_connections)
        .await
        .context("connecting to database")?;
    akasha_db::migrate(&pool).await?;
    let storage = akasha_storage::Storage::from_config(&config).context("opening storage")?;
    let ctx = jobs::JobContext::new(pool, storage, &config);
    let stop = shutdown_trigger();
    jobs::worker(ctx, &config)?.run(wait_for(stop)).await?;
    tracing::info!("shut down cleanly");
    Ok(())
}

/// A flag set once a shutdown signal arrives.
fn shutdown_trigger() -> tokio::sync::watch::Sender<bool> {
    let (tx, _) = tokio::sync::watch::channel(false);
    let signal = tx.clone();
    tokio::spawn(async move {
        shutdown_signal().await;
        let _ = signal.send(true);
    });
    tx
}

/// Resolves once the flag is set.
async fn wait_for(stop: tokio::sync::watch::Sender<bool>) {
    let mut rx = stop.subscribe();
    let _ = rx.wait_for(|stopped| *stopped).await;
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
