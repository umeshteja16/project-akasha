//! Serving the web UI (SPA fallback, caching, security headers) and `/api/v1/meta`.
//! None of this needs a database.

use std::{sync::Arc, time::Duration};

use akasha::{AppState, app, web::WebAssets};
use akasha_core::Config;
use akasha_storage::Storage;
use axum::{
    body::Body,
    http::{HeaderMap, Method, Request, StatusCode, header},
};
use http_body_util::BodyExt;
use serde_json::Value;
use sqlx::postgres::PgPoolOptions;
use tower::ServiceExt;

const INDEX: &str = "<!doctype html><title>Akasha</title><div id=root></div>";

fn state(config: Config, web: WebAssets) -> AppState {
    let pool = PgPoolOptions::new()
        .acquire_timeout(Duration::from_millis(200))
        .connect_lazy("postgres://nobody@127.0.0.1:1/none")
        .expect("lazy pool");
    let mut state = AppState::with_llm(pool, config, Storage::in_memory(), None);
    state.web = Arc::new(web);
    state
}

fn ui() -> WebAssets {
    WebAssets::from_files([
        ("index.html", INDEX.as_bytes().to_vec()),
        ("assets/index-abc123.js", b"console.log(1)".to_vec()),
        ("theme-init.js", b"/* theme */".to_vec()),
    ])
}

struct Reply {
    status: StatusCode,
    headers: HeaderMap,
    body: Vec<u8>,
}

impl Reply {
    fn header(&self, name: header::HeaderName) -> &str {
        self.headers
            .get(name)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
    }

    fn json(&self) -> Value {
        serde_json::from_slice(&self.body).expect("json body")
    }
}

async fn send(state: AppState, method: Method, path: &str, headers: &[(&str, &str)]) -> Reply {
    let mut request = Request::builder().method(method).uri(path);
    for (name, value) in headers {
        request = request.header(*name, *value);
    }
    let response = app(state)
        .oneshot(request.body(Body::empty()).expect("request"))
        .await
        .expect("response");
    let status = response.status();
    let headers = response.headers().clone();
    let body = response
        .into_body()
        .collect()
        .await
        .expect("body")
        .to_bytes()
        .to_vec();
    Reply {
        status,
        headers,
        body,
    }
}

async fn get(state: AppState, path: &str) -> Reply {
    send(state, Method::GET, path, &[]).await
}

#[tokio::test]
async fn root_and_client_routes_serve_index_without_caching() {
    for path in ["/", "/library", "/settings/account", "/chat/3f2c"] {
        let reply = get(state(Config::default(), ui()), path).await;
        assert_eq!(reply.status, StatusCode::OK, "{path}");
        assert_eq!(reply.body, INDEX.as_bytes(), "{path}");
        assert!(reply.header(header::CONTENT_TYPE).starts_with("text/html"));
        assert_eq!(reply.header(header::CACHE_CONTROL), "no-cache");
        assert!(!reply.header(header::ETAG).is_empty());
    }
}

#[tokio::test]
async fn hashed_assets_are_immutable() {
    let reply = get(state(Config::default(), ui()), "/assets/index-abc123.js").await;
    assert_eq!(reply.status, StatusCode::OK);
    assert_eq!(reply.body, b"console.log(1)");
    assert!(reply.header(header::CONTENT_TYPE).contains("javascript"));
    assert_eq!(
        reply.header(header::CACHE_CONTROL),
        "public, max-age=31536000, immutable"
    );

    let other = get(state(Config::default(), ui()), "/theme-init.js").await;
    assert_eq!(other.status, StatusCode::OK);
    assert_eq!(other.header(header::CACHE_CONTROL), "no-cache");
}

#[tokio::test]
async fn etag_revalidation_returns_304() {
    let first = get(state(Config::default(), ui()), "/").await;
    let etag = first.header(header::ETAG).to_owned();
    let again = send(
        state(Config::default(), ui()),
        Method::GET,
        "/library",
        &[("if-none-match", &etag)],
    )
    .await;
    assert_eq!(again.status, StatusCode::NOT_MODIFIED);
    assert!(again.body.is_empty());
}

#[tokio::test]
async fn missing_files_and_api_paths_stay_json_404() {
    for path in [
        "/assets/gone-123.js",
        "/favicon.ico",
        "/api/v1/nope",
        "/api",
        "/../etc/passwd",
    ] {
        let reply = get(state(Config::default(), ui()), path).await;
        assert_eq!(reply.status, StatusCode::NOT_FOUND, "{path}");
        assert_eq!(reply.json()["error"]["code"], "not_found", "{path}");
    }
    let post = send(
        state(Config::default(), ui()),
        Method::POST,
        "/library",
        &[],
    )
    .await;
    assert_eq!(post.status, StatusCode::NOT_FOUND);
    assert_eq!(post.json()["error"]["code"], "not_found");
}

#[tokio::test]
async fn without_a_ui_every_unknown_path_is_json_404() {
    let reply = get(state(Config::default(), WebAssets::none()), "/library").await;
    assert_eq!(reply.status, StatusCode::NOT_FOUND);
    assert_eq!(reply.json()["error"]["code"], "not_found");
}

#[tokio::test]
async fn security_headers_on_ui_and_api_responses() {
    for path in ["/", "/healthz", "/api/v1/nope"] {
        let reply = get(state(Config::default(), ui()), path).await;
        let csp = reply.header(header::CONTENT_SECURITY_POLICY);
        assert!(csp.contains("default-src 'self'"), "{path}: {csp}");
        assert!(csp.contains("script-src 'self';"), "{path}: {csp}");
        assert!(csp.contains("frame-ancestors 'none'"), "{path}: {csp}");
        assert_eq!(reply.header(header::X_FRAME_OPTIONS), "DENY", "{path}");
        assert_eq!(reply.header(header::X_CONTENT_TYPE_OPTIONS), "nosniff");
        assert_eq!(reply.header(header::REFERRER_POLICY), "same-origin");
    }
}

#[tokio::test]
async fn meta_is_public_and_reflects_registration() {
    let open = get(state(Config::default(), WebAssets::none()), "/api/v1/meta").await;
    assert_eq!(open.status, StatusCode::OK);
    let body = open.json();
    assert_eq!(body["allow_registration"], true);
    assert_eq!(body["chat_model"], false);
    assert_eq!(body["version"], env!("CARGO_PKG_VERSION"));

    let closed = Config {
        allow_registration: false,
        ..Config::default()
    };
    let reply = get(state(closed, WebAssets::none()), "/api/v1/meta").await;
    assert_eq!(reply.json()["allow_registration"], false);
}
