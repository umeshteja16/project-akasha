//! The Server-Sent Events of a chat answer, in order:
//! `sources` → `delta`* → `done`, or `error` at any point after `sources`.

use akasha_llm::Usage;
use axum::response::sse::Event;
use serde::Serialize;
use utoipa::ToSchema;
use uuid::Uuid;

use super::citations::Citation;

/// How an answer ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum AnswerStatus {
    /// The model answered from the sources.
    Answered,
    /// The files do not contain the answer (weak evidence, or the model or its
    /// provider declined); the answer is the fixed "not found" sentence.
    Refused,
    /// No language model is configured: the sources are the answer.
    NoLlm,
    /// The client disconnected; generation stopped.
    Cancelled,
    /// Generation failed part-way.
    Error,
}

impl AnswerStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Answered => "answered",
            Self::Refused => "refused",
            Self::NoLlm => "no_llm",
            Self::Cancelled => "cancelled",
            Self::Error => "error",
        }
    }
}

/// `event: sources`: the passages the answer may cite (empty when refused).
#[derive(Debug, Serialize, ToSchema)]
pub struct ChatSources {
    pub conversation_id: Uuid,
    /// The stored question.
    pub user_message_id: Uuid,
    /// The query that was searched (the question, or a standalone rewrite of a
    /// follow-up).
    pub search_query: String,
    pub sources: Vec<Citation>,
}

/// `event: delta`: the next piece of the answer text.
#[derive(Debug, Serialize, ToSchema)]
pub struct ChatDelta {
    pub text: String,
}

/// Token counts reported by the provider.
#[derive(Debug, Clone, Copy, Default, Serialize, ToSchema)]
pub struct TokenUsage {
    pub input_tokens: Option<u32>,
    pub output_tokens: Option<u32>,
}

impl From<Usage> for TokenUsage {
    fn from(u: Usage) -> Self {
        Self {
            input_tokens: u.input_tokens,
            output_tokens: u.output_tokens,
        }
    }
}

/// `event: done`: the stored answer.
#[derive(Debug, Serialize, ToSchema)]
pub struct ChatDone {
    pub message_id: Uuid,
    pub status: AnswerStatus,
    /// The final answer text. Use it instead of the concatenated deltas: a
    /// refusal replaces partial output.
    pub content: String,
    /// The sources the answer cites, in order of first citation (for `no_llm`:
    /// every source).
    pub citations: Vec<Citation>,
    /// `provider/model` that wrote the answer; `null` when none did.
    pub model: Option<String>,
    pub usage: TokenUsage,
    pub latency_ms: u64,
}

/// `event: error`: the answer failed. Partial output is stored with status `error`.
#[derive(Debug, Serialize, ToSchema)]
pub struct ChatError {
    /// `llm_unavailable`, `llm_rate_limited`, `llm_error`, `llm_misconfigured`
    /// or `internal`.
    pub code: String,
    pub message: String,
    /// The stored (partial) answer, if it could be saved.
    pub message_id: Option<Uuid>,
}

pub fn event(name: &str, data: &impl Serialize) -> Event {
    Event::default()
        .event(name)
        .json_data(data)
        .unwrap_or_else(|_| {
            Event::default()
                .event("error")
                .data("{\"code\":\"internal\"}")
        })
}
