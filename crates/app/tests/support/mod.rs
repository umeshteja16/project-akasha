//! Shared helpers for the file HTTP tests.
#![allow(dead_code)] // each test crate uses a different subset

pub mod chat;
pub mod llm;
pub mod mcp;

use std::net::SocketAddr;

use akasha::{AppState, app, jobs::JobContext};
use akasha_core::Config;
use akasha_storage::{ContentHash, Storage};
use axum::{
    Extension, Router,
    body::Body,
    extract::ConnectInfo,
    http::{HeaderMap, Request, StatusCode, header},
};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use sqlx::PgPool;
use tower::ServiceExt;

pub const PW: &str = "correct horse battery";
pub const BOUNDARY: &str = "akasha-test-boundary-7d1f";
pub const PDF: &[u8] = b"%PDF-1.4\n1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n";

/// Defaults, minus anything that would reach the network.
pub fn test_config() -> Config {
    Config {
        ocr_enabled: false,
        // Deterministic "tone" transcriber: real decoding, no speech model.
        transcribe_enabled: true,
        whisper_model: akasha_media::FAKE_MODEL.into(),
        // Deterministic built-in models: no downloads, no ONNX Runtime.
        embed_model: akasha_ml::catalog::HASH_EMBED_MODEL.into(),
        rerank_model: akasha_ml::catalog::OVERLAP_RERANK_MODEL.into(),
        ml_models_url: String::new(),
        // Deterministic chat model; tests never call a real provider.
        llm_provider: akasha_core::LlmProvider::Fake,
        chat_rate_per_minute: 0,
        ..Config::default()
    }
}

pub struct TestApp {
    pub router: Router,
    pub storage: Storage,
    pub db: PgPool,
    /// The app's language model; background jobs run with it too.
    pub llm: Option<std::sync::Arc<dyn akasha_llm::ChatModel>>,
}

pub struct Reply {
    pub status: StatusCode,
    pub headers: HeaderMap,
    pub bytes: Vec<u8>,
}

impl Reply {
    pub fn json(&self) -> Value {
        if self.bytes.is_empty() {
            Value::Null
        } else {
            serde_json::from_slice(&self.bytes).expect("json body")
        }
    }

    pub fn code(&self) -> Value {
        self.json()["error"]["code"].clone()
    }
}

pub fn multipart(filename: &str, bytes: &[u8]) -> Vec<u8> {
    let mut body = format!(
        "--{BOUNDARY}\r\nContent-Disposition: form-data; name=\"note\"\r\n\r\nignored\r\n\
         --{BOUNDARY}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"{filename}\"\r\n\
         Content-Type: application/octet-stream\r\n\r\n"
    )
    .into_bytes();
    body.extend_from_slice(bytes);
    body.extend_from_slice(format!("\r\n--{BOUNDARY}--\r\n").as_bytes());
    body
}

impl TestApp {
    /// OCR is off and models are the built-in fakes: tests never download.
    pub fn new(pool: PgPool) -> Self {
        Self::with_config(pool, test_config())
    }

    pub fn with_config(pool: PgPool, config: Config) -> Self {
        let storage = Storage::in_memory();
        Self::with_state(AppState::new(pool, config, storage))
    }

    /// With a specific chat model (`None`: no model).
    pub fn with_llm(
        pool: PgPool,
        config: Config,
        llm: Option<std::sync::Arc<dyn akasha_llm::ChatModel>>,
    ) -> Self {
        Self::with_state(AppState::with_llm(pool, config, Storage::in_memory(), llm))
    }

    pub fn with_state(state: AppState) -> Self {
        let (pool, storage, llm) = (state.db.clone(), state.storage.clone(), state.llm.clone());
        let addr = SocketAddr::from(([127, 0, 0, 1], 40000));
        let router = app(state).layer(Extension(ConnectInfo(addr)));
        Self {
            router,
            storage,
            db: pool,
            llm,
        }
    }

    pub async fn request(&self, req: Request<Body>) -> Reply {
        let res = self.router.clone().oneshot(req).await.expect("response");
        let status = res.status();
        let headers = res.headers().clone();
        let bytes = res.into_body().collect().await.expect("body").to_bytes();
        Reply {
            status,
            headers,
            bytes: bytes.to_vec(),
        }
    }

    pub async fn send(&self, method: &str, path: &str, cookie: &str, body: Option<Value>) -> Reply {
        let req = Request::builder()
            .method(method)
            .uri(path)
            .header(header::COOKIE, cookie);
        let req = match body {
            Some(json) => req
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(json.to_string())),
            None => req.body(Body::empty()),
        };
        self.request(req.expect("request")).await
    }

    pub async fn upload(&self, cookie: &str, filename: &str, bytes: &[u8]) -> Reply {
        let req = Request::post("/api/v1/files")
            .header(header::COOKIE, cookie)
            .header(
                header::CONTENT_TYPE,
                format!("multipart/form-data; boundary={BOUNDARY}"),
            )
            .body(Body::from(multipart(filename, bytes)))
            .expect("request");
        self.request(req).await
    }

    /// Register and return the session cookie.
    pub async fn user(&self, email: &str) -> String {
        let req = Request::post("/api/v1/auth/register")
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(
                json!({ "email": email, "password": PW }).to_string(),
            ))
            .expect("request");
        let reply = self.request(req).await;
        assert_eq!(reply.status, StatusCode::CREATED);
        let raw = reply.headers[header::SET_COOKIE].to_str().expect("cookie");
        raw.split(';').next().expect("pair").to_owned()
    }

    /// Run every due background job (as a worker would); returns how many ran.
    pub async fn run_jobs(&self) -> usize {
        self.run_jobs_with(&test_config()).await
    }

    /// [`Self::run_jobs`] with a worker built from `config` (and the app's
    /// language model).
    pub async fn run_jobs_with(&self, config: &Config) -> usize {
        let config = config.clone();
        let ctx = JobContext::new(self.db.clone(), self.storage.clone(), &config)
            .with_llm(self.llm.clone());
        akasha::jobs::worker(ctx, &config)
            .expect("worker")
            .run_until_idle()
            .await
            .expect("run jobs")
    }

    pub async fn blob_exists(&self, hex: &str) -> bool {
        let hash: ContentHash = hex.parse().expect("hash");
        self.storage.exists(&hash).await.expect("exists")
    }
}
