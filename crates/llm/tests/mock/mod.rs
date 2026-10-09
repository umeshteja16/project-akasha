//! A scripted HTTP server on 127.0.0.1 standing in for a provider: it records
//! each request and answers with the next scripted response, streaming the body
//! in the given chunks (with a pause between them so they arrive separately).

use std::{
    collections::VecDeque,
    convert::Infallible,
    sync::{Arc, Mutex},
    time::Duration,
};

use akasha_llm::{ApiKey, LlmOptions, Provider};
use axum::{
    Router,
    body::{Body, Bytes},
    extract::State,
    http::{HeaderMap, StatusCode, Uri},
    response::Response,
};
use futures_util::{StreamExt, stream};

#[derive(Debug, Clone)]
pub struct Recorded {
    pub uri: Uri,
    pub headers: HeaderMap,
    pub body: serde_json::Value,
}

pub struct Reply {
    pub status: u16,
    pub headers: Vec<(&'static str, &'static str)>,
    pub chunks: Vec<Vec<u8>>,
}

impl Reply {
    pub fn ok(chunks: Vec<Vec<u8>>) -> Self {
        Self {
            status: 200,
            headers: Vec::new(),
            chunks,
        }
    }

    pub fn status(status: u16, body: &str) -> Self {
        Self {
            status,
            headers: Vec::new(),
            chunks: vec![body.as_bytes().to_vec()],
        }
    }
}

/// Split `text` into pieces of `size` bytes (deliberately ignoring line and
/// character boundaries).
pub fn split(text: &str, size: usize) -> Vec<Vec<u8>> {
    text.as_bytes().chunks(size).map(<[u8]>::to_vec).collect()
}

#[derive(Clone, Default)]
struct Shared {
    replies: Arc<Mutex<VecDeque<Reply>>>,
    seen: Arc<Mutex<Vec<Recorded>>>,
}

pub struct Mock {
    pub url: String,
    shared: Shared,
}

impl Mock {
    pub async fn start(replies: Vec<Reply>) -> Self {
        let shared = Shared {
            replies: Arc::new(Mutex::new(replies.into())),
            seen: Arc::default(),
        };
        let app = Router::new().fallback(handle).with_state(shared.clone());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind");
        let addr = listener.local_addr().expect("addr");
        tokio::spawn(async move { axum::serve(listener, app).await });
        Self {
            url: format!("http://{addr}"),
            shared,
        }
    }

    pub fn requests(&self) -> Vec<Recorded> {
        self.shared.seen.lock().expect("lock").clone()
    }

    /// Options for `provider` pointing every base URL at this server, with
    /// fast retries.
    pub fn options(&self, provider: Provider, model: &str) -> LlmOptions {
        LlmOptions {
            provider,
            model: model.into(),
            ollama_url: self.url.clone(),
            anthropic_base_url: self.url.clone(),
            anthropic_api_key: ApiKey::new("sk-ant-test-key"),
            gemini_base_url: self.url.clone(),
            gemini_api_key: ApiKey::new("AIza-test-key"),
            openai_base_url: format!("{}/v1", self.url),
            openai_api_key: ApiKey::new("sk-openai-test"),
            retry_base: Duration::from_millis(5),
            read_timeout: Duration::from_secs(5),
            ..LlmOptions::default()
        }
    }
}

async fn handle(
    State(shared): State<Shared>,
    uri: Uri,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    shared.seen.lock().expect("lock").push(Recorded {
        uri,
        headers,
        body: serde_json::from_slice(&body).unwrap_or(serde_json::Value::Null),
    });
    let reply = shared.replies.lock().expect("lock").pop_front();
    let Some(reply) = reply else {
        return Response::builder()
            .status(StatusCode::GONE)
            .body(Body::from("no more scripted replies"))
            .expect("response");
    };
    let chunks = stream::iter(reply.chunks).then(|c| async move {
        tokio::time::sleep(Duration::from_millis(2)).await;
        Ok::<_, Infallible>(Bytes::from(c))
    });
    let mut res = Response::builder().status(reply.status);
    for (k, v) in reply.headers {
        res = res.header(k, v);
    }
    res.body(Body::from_stream(chunks)).expect("response")
}
