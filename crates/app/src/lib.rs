//! The Akasha server: HTTP routes, application state and process entry points.
//!
//! `main.rs` only parses the command line; everything testable lives here.

pub mod admin;
pub mod auth;
pub mod chat;
pub mod enrich;
pub mod error;
pub mod eval;
pub mod extract;
pub mod files;
pub mod jobs;
pub mod llm;
pub mod mcp;
pub mod rate_limit;
pub mod routes;
pub mod state;
pub mod telemetry;
pub mod web;

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
    web::security_headers(routes::router(&state).with_state(state))
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
    admin::check_embedding_model(&pool, &config).await?;
    let llm = llm::build(&config).context("configuring the chat model")?;
    let storage = akasha_storage::Storage::from_config(&config).context("opening storage")?;

    let listener = tokio::net::TcpListener::bind(&config.bind_addr)
        .await
        .with_context(|| format!("binding {}", config.bind_addr))?;
    tracing::info!(addr = %config.bind_addr, with_worker, "listening");

    let state = AppState::with_llm(pool, config, storage, llm);
    rate_limit::spawn_cleanup(
        state.auth_limiter.clone(),
        vec![state.search_limiter.clone(), state.chat_limiter.clone()],
    );
    let stop = shutdown_trigger();

    warm_up_search(&state);
    let worker = if with_worker {
        let ctx = jobs::JobContext::from(&state);
        warm_up(&ctx);
        let worker = jobs::worker(ctx, &state.config)?;
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
    admin::check_embedding_model(&pool, &config).await?;
    let storage = akasha_storage::Storage::from_config(&config).context("opening storage")?;
    let llm = llm::build(&config).context("configuring the language model")?;
    let ctx = jobs::JobContext::new(pool, storage, &config).with_llm(llm);
    warm_up(&ctx);
    let stop = shutdown_trigger();
    jobs::worker(ctx, &config)?.run(wait_for(stop)).await?;
    tracing::info!("shut down cleanly");
    Ok(())
}

/// Load the embedding model in the background at start, so its first download
/// happens now (not in the first upload's job) and problems show up in the log early.
fn warm_up(ctx: &jobs::JobContext) {
    let ml = std::sync::Arc::clone(&ctx.ml);
    tokio::spawn(async move {
        if let Err(err) = ml.embedder().await {
            tracing::warn!(%err, "embedding model unavailable; embed jobs will retry");
        }
    });
}

/// Start loading the search models (embedder and reranker) so the first searches
/// do not run degraded while they load.
fn warm_up_search(state: &AppState) {
    let ml = std::sync::Arc::clone(&state.ml);
    tokio::spawn(async move {
        if let Err(err) = ml.reranker().await {
            tracing::warn!(%err, "reranker unavailable; search results will not be reranked");
        }
    });
    let ml = std::sync::Arc::clone(&state.ml);
    tokio::spawn(async move {
        if let Err(err) = ml.embedder().await {
            tracing::warn!(%err, "embedding model unavailable; search falls back to keywords");
        }
    });
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
