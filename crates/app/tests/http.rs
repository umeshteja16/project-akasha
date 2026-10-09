//! HTTP-level tests that need no database.

use std::time::Duration;

use akasha::{AppState, app};
use akasha_core::Config;
use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use serde_json::Value;
use sqlx::postgres::PgPoolOptions;
use tower::ServiceExt;

/// A pool pointing at nothing: any query fails quickly.
fn unreachable_state() -> AppState {
    let pool = PgPoolOptions::new()
        .acquire_timeout(Duration::from_millis(200))
        .connect_lazy("postgres://nobody@127.0.0.1:1/none")
        .expect("lazy pool");
    AppState::new(pool, Config::default())
}

async fn get(path: &str) -> (StatusCode, Option<String>, Value) {
    let response = app(unreachable_state())
        .oneshot(Request::get(path).body(Body::empty()).expect("request"))
        .await
        .expect("response");
    let status = response.status();
    let request_id = response
        .headers()
        .get("x-request-id")
        .and_then(|v| v.to_str().ok())
        .map(str::to_owned);
    let bytes = response
        .into_body()
        .collect()
        .await
        .expect("body")
        .to_bytes();
    (
        status,
        request_id,
        serde_json::from_slice(&bytes).expect("json"),
    )
}

#[tokio::test]
async fn healthz_is_ok_without_database() {
    let (status, request_id, body) = get("/healthz").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["status"], "ok");
    assert!(request_id.is_some(), "every response carries x-request-id");
}

#[tokio::test]
async fn readyz_reports_unavailable_when_database_is_down() {
    let (status, _, body) = get("/readyz").await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(body["error"]["code"], "unavailable");
}

#[tokio::test]
async fn unknown_routes_use_the_error_shape() {
    let (status, _, body) = get("/nope").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["error"]["code"], "not_found");
}

#[tokio::test]
async fn openapi_document_lists_health_routes() {
    let (status, _, body) = get("/api/openapi.json").await;
    assert_eq!(status, StatusCode::OK);
    assert!(body["paths"]["/healthz"].is_object());
    assert!(body["paths"]["/readyz"].is_object());
}
