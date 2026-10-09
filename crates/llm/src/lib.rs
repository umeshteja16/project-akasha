//! Chat models for grounded answers (ADR 0012).
//!
//! One async, streaming trait, [`ChatModel`], with thin `reqwest` implementations
//! of each provider's HTTP API (no vendor SDKs):
//!
//! - [`ollama`]: Ollama `/api/chat` (NDJSON). The offline default (ADR 0003).
//! - [`anthropic`]: the Claude Messages API (SSE).
//! - [`gemini`]: Gemini `streamGenerateContent` (SSE). The key goes in the
//!   `x-goog-api-key` header, never in the URL.
//! - [`openai`]: OpenAI-compatible `/chat/completions` (SSE): OpenAI, vLLM,
//!   LM Studio, llama.cpp server, OpenRouter, ...
//! - [`fake`]: a deterministic model for tests (never touches the network).
//!
//! Requests are retried on connection errors, 429 and 5xx with backoff, but only
//! before the response starts streaming. Cancelling a generation is dropping its
//! [`ChatStream`]: that closes the HTTP connection.

pub mod anthropic;
mod error;
pub mod fake;
pub mod gemini;
mod http;
pub mod local;
pub mod ollama;
pub mod openai;
mod options;
pub mod stream;

use std::pin::Pin;

use futures_util::{Stream, StreamExt, future::BoxFuture};
use serde::Serialize;

pub use error::LlmError;
pub use options::{ApiKey, LlmOptions, Provider, build};

/// Who said a message.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    User,
    Assistant,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Message {
    pub role: Role,
    pub content: String,
}

impl Message {
    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: Role::User,
            content: content.into(),
        }
    }

    pub fn assistant(content: impl Into<String>) -> Self {
        Self {
            role: Role::Assistant,
            content: content.into(),
        }
    }
}

/// One generation: a system prompt and the conversation so far (oldest first,
/// ending with a user message).
#[derive(Debug, Clone)]
pub struct ChatRequest {
    pub system: String,
    pub messages: Vec<Message>,
    /// Upper bound on generated tokens.
    pub max_tokens: u32,
    /// Sampling temperature where the provider accepts one (current Claude models
    /// do not: the Anthropic provider never sends it).
    pub temperature: Option<f32>,
    /// Ask for a single JSON object as the whole answer. Providers with a JSON
    /// mode that every server honours use it (Ollama `format: "json"`, Gemini
    /// `responseMimeType`); the others rely on the prompt, so callers must still
    /// parse defensively.
    pub json: bool,
}

/// Token counts as reported by the provider (`None` when it did not say).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct Usage {
    pub input_tokens: Option<u32>,
    pub output_tokens: Option<u32>,
}

/// Why generation stopped.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StopReason {
    /// The model finished its answer.
    EndTurn,
    /// `max_tokens` was reached; the answer is cut off.
    MaxTokens,
    /// The provider's safety system (or the model) declined; partial output
    /// should be discarded.
    Refusal,
    Other,
}

/// An item of a streamed generation.
#[derive(Debug, Clone, PartialEq)]
pub enum ChatEvent {
    /// The next piece of the answer.
    Delta(String),
    /// Generation finished; always the last event of a successful stream.
    Done { stop: StopReason, usage: Usage },
}

/// A streamed generation. Dropping it cancels the request.
pub type ChatStream = Pin<Box<dyn Stream<Item = Result<ChatEvent, LlmError>> + Send>>;

/// A whole generation, collected.
#[derive(Debug, Clone, PartialEq)]
pub struct Completion {
    pub text: String,
    pub stop: StopReason,
    pub usage: Usage,
}

/// A chat model behind some provider.
pub trait ChatModel: Send + Sync {
    /// `ollama`, `anthropic`, `gemini`, `openai` or `fake`.
    fn provider(&self) -> &'static str;

    /// The model id sent to the provider.
    fn model(&self) -> &str;

    /// Start a generation. Resolves once the provider accepted the request
    /// (after any retries); the answer then arrives on the stream.
    fn stream<'a>(&'a self, req: &'a ChatRequest) -> BoxFuture<'a, Result<ChatStream, LlmError>>;

    /// Generate the whole answer before returning (for short, internal calls).
    fn complete<'a>(&'a self, req: &'a ChatRequest) -> BoxFuture<'a, Result<Completion, LlmError>> {
        Box::pin(async move {
            let mut stream = self.stream(req).await?;
            let mut text = String::new();
            while let Some(event) = stream.next().await {
                match event? {
                    ChatEvent::Delta(d) => text.push_str(&d),
                    ChatEvent::Done { stop, usage } => return Ok(Completion { text, stop, usage }),
                }
            }
            Err(LlmError::Protocol(
                "the stream ended without finishing".into(),
            ))
        })
    }
}
