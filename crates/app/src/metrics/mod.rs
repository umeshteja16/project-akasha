//! Prometheus metrics (`AKASHA_METRICS_ENABLED=true`).
//!
//! Exposure (ADR 0018): with `AKASHA_METRICS_BIND_ADDR` the scrape endpoint is served
//! on its own listener (`GET /metrics`, no auth: keep that port on a private network);
//! otherwise `GET /metrics` on the main port requires `Authorization: Bearer
//! <AKASHA_METRICS_TOKEN>`, and `serve` refuses to start without one. Worker-only
//! processes expose metrics only on the separate listener.
//!
//! Instrumentation uses the `metrics` facade, which is a no-op until [`install`] sets
//! the recorder, so code records unconditionally:
//!
//! | metric | labels |
//! |---|---|
//! | `akasha_http_requests_total`, `akasha_http_request_duration_seconds` | `method`, `route` (the route template), `status` |
//! | `akasha_jobs_finished_total` | `kind`, `outcome` (`succeeded`/`retry`/`dead`) |
//! | `akasha_job_duration_seconds`, `akasha_job_wait_seconds` | `kind` |
//! | `akasha_jobs` (gauge), `akasha_jobs_oldest_due_seconds` | `kind`, `state` |
//! | `akasha_ingest_duration_seconds` | `stage` (`extract`, `embed`, `transcribe`) |
//! | `akasha_search_duration_seconds` | `stage` (`keyword`, `embed`, `semantic`, `fetch`, `rerank`, `total`) |
//! | `akasha_llm_requests_total`, `akasha_llm_tokens_total`, `akasha_llm_duration_seconds` | `provider`, `model`, `outcome` / `direction` |
//! | `akasha_model_ready` | `role` (`embedder`, `reranker`) |
//! | `akasha_build_info` | `version`, `cpu_build` |

pub mod llm;

use std::{
    sync::{Arc, OnceLock},
    time::{Duration, Instant},
};

use akasha_core::Config;
use akasha_db::PgPool;
use anyhow::Context;
use axum::{
    Router,
    extract::{MatchedPath, Request, State},
    http::{HeaderMap, StatusCode, header},
    middleware::Next,
    response::{IntoResponse, Response},
    routing::get,
};
use metrics::{counter, gauge, histogram};
use metrics_exporter_prometheus::{PrometheusBuilder, PrometheusHandle};

use crate::jobs::ml::{MlProvider, ModelState};

static HANDLE: OnceLock<PrometheusHandle> = OnceLock::new();

/// Latency buckets (seconds) for every histogram: 5 ms to 10 minutes.
const BUCKETS: &[f64] = &[
    0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0, 30.0, 60.0, 120.0, 300.0, 600.0,
];
/// How often queue and model gauges are refreshed.
const COLLECT_EVERY: Duration = Duration::from_secs(15);

/// Check the metrics settings for `serve` (a public port needs a token).
pub fn check_config(config: &Config) -> anyhow::Result<()> {
    if config.metrics_enabled
        && config.metrics_bind_addr.trim().is_empty()
        && config
            .metrics_token
            .as_ref()
            .is_none_or(|t| t.expose().trim().is_empty())
    {
        anyhow::bail!(
            "AKASHA_METRICS_ENABLED needs AKASHA_METRICS_BIND_ADDR (a private port) or \
             AKASHA_METRICS_TOKEN (to protect /metrics on the main port)"
        );
    }
    Ok(())
}

/// Install the Prometheus recorder (once per process). `None` when metrics are off.
pub fn install(config: &Config) -> anyhow::Result<Option<PrometheusHandle>> {
    if !config.metrics_enabled {
        return Ok(None);
    }
    if HANDLE.get().is_none() {
        let handle = PrometheusBuilder::new()
            .set_buckets(BUCKETS)
            .context("metric buckets")?
            .install_recorder()
            .context("installing the metrics recorder")?;
        let _ = HANDLE.set(handle);
        describe();
    }
    gauge!(
        "akasha_build_info",
        "version" => env!("CARGO_PKG_VERSION"),
        "cpu_build" => crate::cpu::build_variant()
    )
    .set(1.0);
    Ok(HANDLE.get().cloned())
}

fn describe() {
    use metrics::{describe_counter, describe_gauge, describe_histogram};
    describe_counter!(
        "akasha_http_requests_total",
        "HTTP requests by route and status"
    );
    describe_histogram!(
        "akasha_http_request_duration_seconds",
        metrics::Unit::Seconds,
        "Time to the response headers"
    );
    describe_counter!(
        "akasha_jobs_finished_total",
        "Job attempts by kind and outcome"
    );
    describe_histogram!(
        "akasha_job_duration_seconds",
        metrics::Unit::Seconds,
        "Job run time"
    );
    describe_histogram!(
        "akasha_job_wait_seconds",
        metrics::Unit::Seconds,
        "Time from a job being due to a worker starting it"
    );
    describe_gauge!("akasha_jobs", "Jobs in the queue by kind and state");
    describe_gauge!(
        "akasha_jobs_oldest_due_seconds",
        metrics::Unit::Seconds,
        "Age of the oldest due job still waiting"
    );
    describe_histogram!(
        "akasha_ingest_duration_seconds",
        metrics::Unit::Seconds,
        "Extraction, embedding and transcription time per file"
    );
    describe_histogram!(
        "akasha_search_duration_seconds",
        metrics::Unit::Seconds,
        "Search time by stage"
    );
    describe_counter!(
        "akasha_llm_requests_total",
        "Language model calls by outcome"
    );
    describe_counter!(
        "akasha_llm_tokens_total",
        "Tokens reported by the language model"
    );
    describe_histogram!(
        "akasha_llm_duration_seconds",
        metrics::Unit::Seconds,
        "Language model call time (whole answer)"
    );
    describe_gauge!("akasha_model_ready", "1 when the model is loaded");
}

/// Middleware: count and time requests by route template (never the raw path).
pub async fn track(req: Request, next: Next) -> Response {
    let started = Instant::now();
    let method = req.method().as_str().to_owned();
    let route = req
        .extensions()
        .get::<MatchedPath>()
        .map_or_else(|| "unmatched".to_owned(), |p| p.as_str().to_owned());
    let res = next.run(req).await;
    let status = res.status().as_u16().to_string();
    counter!(
        "akasha_http_requests_total",
        "method" => method.clone(),
        "route" => route.clone(),
        "status" => status
    )
    .increment(1);
    histogram!(
        "akasha_http_request_duration_seconds",
        "method" => method,
        "route" => route
    )
    .record(started.elapsed().as_secs_f64());
    res
}

/// `GET /metrics` on the main port: bearer token required.
pub async fn scrape_with_token(State(token): State<Arc<str>>, headers: HeaderMap) -> Response {
    let given = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .unwrap_or_default();
    if !constant_time_eq(given.as_bytes(), token.as_bytes()) {
        return (
            StatusCode::UNAUTHORIZED,
            [(header::WWW_AUTHENTICATE, "Bearer")],
            "metrics need the AKASHA_METRICS_TOKEN bearer token\n",
        )
            .into_response();
    }
    scrape().await
}

async fn scrape() -> Response {
    match HANDLE.get() {
        Some(handle) => (
            [(header::CONTENT_TYPE, "text/plain; version=0.0.4")],
            handle.render(),
        )
            .into_response(),
        None => (StatusCode::NOT_FOUND, "metrics are disabled\n").into_response(),
    }
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() || a.is_empty() {
        return false;
    }
    a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

/// The main router's `/metrics` route, when metrics go through the main port.
pub fn main_port_route(config: &Config) -> Option<Router> {
    if !config.metrics_enabled || !config.metrics_bind_addr.trim().is_empty() {
        return None;
    }
    let token: Arc<str> = Arc::from(config.metrics_token.as_ref()?.expose().trim());
    Some(
        Router::new()
            .route("/metrics", get(scrape_with_token))
            .with_state(token),
    )
}

/// Serve `/metrics` on `AKASHA_METRICS_BIND_ADDR` until `stop` resolves (if set).
pub async fn spawn_listener(
    config: &Config,
    stop: impl std::future::Future<Output = ()> + Send + 'static,
) -> anyhow::Result<()> {
    let addr = config.metrics_bind_addr.trim();
    if !config.metrics_enabled || addr.is_empty() {
        return Ok(());
    }
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .with_context(|| format!("binding the metrics listener {addr}"))?;
    tracing::info!(%addr, "serving Prometheus metrics at /metrics");
    let app = Router::new().route("/metrics", get(scrape));
    tokio::spawn(async move {
        if let Err(err) = axum::serve(listener, app)
            .with_graceful_shutdown(stop)
            .await
        {
            tracing::error!(%err, "metrics listener failed");
        }
    });
    Ok(())
}

/// Refresh the queue and model gauges every few seconds (no-op when metrics are off).
pub fn spawn_collector(db: PgPool, ml: Option<Arc<MlProvider>>) {
    let Some(handle) = HANDLE.get() else { return };
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(COLLECT_EVERY);
        loop {
            tick.tick().await;
            collect(&db, ml.as_deref()).await;
            handle.run_upkeep();
        }
    });
}

async fn collect(db: &PgPool, ml: Option<&MlProvider>) {
    match akasha_jobs::queue::depth(db).await {
        Ok(depth) => {
            for row in depth.counts {
                gauge!("akasha_jobs", "kind" => row.kind, "state" => row.status)
                    .set(row.count as f64);
            }
            gauge!("akasha_jobs_oldest_due_seconds").set(depth.oldest_due_secs);
        }
        Err(err) => tracing::debug!(%err, "could not read queue depth for metrics"),
    }
    if let Some(ml) = ml {
        let ready = |s: ModelState| if s == ModelState::Ready { 1.0 } else { 0.0 };
        gauge!("akasha_model_ready", "role" => "embedder").set(ready(ml.embedder_state()));
        gauge!("akasha_model_ready", "role" => "reranker").set(ready(ml.reranker_state()));
    }
}

/// Record one stage of file processing.
pub fn ingest(stage: &'static str, started: Instant) {
    histogram!("akasha_ingest_duration_seconds", "stage" => stage)
        .record(started.elapsed().as_secs_f64());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokens_compare_exactly() {
        assert!(constant_time_eq(b"secret", b"secret"));
        assert!(!constant_time_eq(b"secret", b"secreT"));
        assert!(!constant_time_eq(b"secret", b"secret2"));
        assert!(!constant_time_eq(b"", b""));
    }

    #[test]
    fn a_public_metrics_port_needs_a_token() {
        let mut config = Config {
            metrics_enabled: true,
            ..Config::default()
        };
        assert!(check_config(&config).is_err());
        config.metrics_bind_addr = "127.0.0.1:9090".into();
        assert!(check_config(&config).is_ok());
        config.metrics_enabled = false;
        config.metrics_bind_addr = String::new();
        assert!(check_config(&config).is_ok());
    }
}
