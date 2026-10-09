//! One question → one streamed, stored answer.
//!
//! Runs in its own task and talks to the HTTP response through a channel: when
//! the client disconnects, the receiver is dropped, `tx.closed()` fires, the
//! model stream is dropped (closing the provider connection) and the partial
//! answer is stored as `cancelled`.

use std::time::Instant;

use akasha_db::chat::{Message as StoredMessage, NewMessage};
use akasha_llm::{ChatModel, StopReason, Usage};
use akasha_search::{ChunkFilter, MAX_QUERY_CHARS, Models, SearchMode, SearchRequest};
use axum::response::sse::Event;
use tokio::sync::mpsc;
use uuid::Uuid;

use super::{
    citations::Citation,
    events::{AnswerStatus, ChatDelta, ChatDone, ChatError, ChatSources, event},
    evidence::{self, Evidence},
    generate::generate,
    prompt,
};
use crate::{routes::search::models, state::AppState};

/// Candidates retrieved per question (sources are picked from these).
const RETRIEVE: usize = 20;
/// How long a follow-up rewrite may take before the plain question is searched.
const CONDENSE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(20);

pub type Tx = mpsc::Sender<Event>;

pub struct Turn {
    pub state: AppState,
    pub owner_id: Uuid,
    pub conversation_id: Uuid,
    pub user_message_id: Uuid,
    pub question: String,
    /// Earlier messages, oldest first (without this question).
    pub history: Vec<StoredMessage>,
    pub filter: ChunkFilter,
    pub started: Instant,
}

/// How the answer ended, ready to store.
pub(super) struct Outcome {
    pub status: AnswerStatus,
    pub content: String,
    pub citations: Vec<Citation>,
    pub model: Option<String>,
    pub usage: Usage,
    /// `(code, message)` for `error`.
    pub error: Option<(String, String)>,
}

impl Outcome {
    pub fn new(status: AnswerStatus, content: impl Into<String>) -> Self {
        Self {
            status,
            content: content.into(),
            citations: Vec::new(),
            model: None,
            usage: Usage::default(),
            error: None,
        }
    }

    pub fn failed(code: &str, message: impl Into<String>, partial: String) -> Self {
        let mut o = Self::new(AnswerStatus::Error, partial);
        o.error = Some((code.to_owned(), message.into()));
        o
    }
}

impl Turn {
    pub async fn run(self, tx: Tx) {
        let outcome = self.answer(&tx).await;
        let latency_ms = u64::try_from(self.started.elapsed().as_millis()).unwrap_or(u64::MAX);
        let message_id = self.store(&outcome, latency_ms).await;
        tracing::info!(
            status = outcome.status.as_str(),
            citations = outcome.citations.len(),
            latency_ms,
            output_tokens = outcome.usage.output_tokens,
            "chat answer"
        );
        let final_event = match (&outcome.error, message_id) {
            (Some((code, message)), id) => event(
                "error",
                &ChatError {
                    code: code.clone(),
                    message: message.clone(),
                    message_id: id,
                },
            ),
            (None, Some(message_id)) => event(
                "done",
                &ChatDone {
                    message_id,
                    status: outcome.status,
                    content: outcome.content,
                    citations: outcome.citations,
                    model: outcome.model,
                    usage: outcome.usage.into(),
                    latency_ms,
                },
            ),
            (None, None) => event(
                "error",
                &ChatError {
                    code: "internal".into(),
                    message: "the answer could not be saved".into(),
                    message_id: None,
                },
            ),
        };
        // The client may be gone (cancelled); nothing to do then.
        let _ = tx.send(final_event).await;
    }

    async fn answer(&self, tx: &Tx) -> Outcome {
        let config = &self.state.config;
        let llm = self.state.llm.clone();
        let query = self.search_query(llm.as_deref()).await;
        let models = models(&self.state).await;
        let req = SearchRequest {
            query: query.clone(),
            mode: SearchMode::Hybrid,
            filter: self.filter.clone(),
            limit: RETRIEVE,
            offset: 0,
            rerank: true,
        };
        let res = match akasha_search::search_chunks(&self.state.db, self.owner_id, &req, &models)
            .await
        {
            Ok(res) => res,
            Err(err) => {
                tracing::error!(%err, "chat retrieval failed");
                return Outcome::failed("internal", "searching your files failed", String::new());
            }
        };
        let reranker = reranker_name(&models, res.meta.reranked);
        let verdict = evidence::assess(&res.results, reranker, config.chat_min_rerank_score);
        let sources = match verdict {
            Evidence::Sufficient => {
                prompt::select_sources(&res.results, config.chat_context_chunks as usize)
            }
            Evidence::Nothing | Evidence::Weak { .. } => {
                tracing::info!(?verdict, "chat refused: weak evidence");
                Vec::new()
            }
        };
        let sources_event = ChatSources {
            conversation_id: self.conversation_id,
            user_message_id: self.user_message_id,
            search_query: query,
            sources: sources.iter().map(|s| s.citation.clone()).collect(),
        };
        if tx.send(event("sources", &sources_event)).await.is_err() {
            return Outcome::new(AnswerStatus::Cancelled, "");
        }
        if sources.is_empty() {
            return self
                .fixed(tx, AnswerStatus::Refused, prompt::NOT_FOUND, Vec::new())
                .await;
        }
        let Some(llm) = llm else {
            let all = sources.iter().map(|s| s.citation.clone()).collect();
            return self
                .fixed(tx, AnswerStatus::NoLlm, prompt::NO_LLM, all)
                .await;
        };
        let request = prompt::answer_request(
            &self.question,
            &self.history,
            &sources,
            config.llm_max_tokens,
            config.llm_temperature,
        );
        let mut outcome = generate(llm.as_ref(), &request, &sources, tx).await;
        outcome.model = Some(format!("{}/{}", llm.provider(), llm.model()));
        outcome
    }

    /// Answer with fixed text (no model call).
    async fn fixed(
        &self,
        tx: &Tx,
        status: AnswerStatus,
        text: &str,
        citations: Vec<Citation>,
    ) -> Outcome {
        let delta = ChatDelta { text: text.into() };
        if tx.send(event("delta", &delta)).await.is_err() {
            return Outcome::new(AnswerStatus::Cancelled, "");
        }
        let mut o = Outcome::new(status, text);
        o.citations = citations;
        o
    }

    /// The question, or for a follow-up a standalone rewrite (by the model when
    /// there is one, else the previous question plus this one).
    async fn search_query(&self, llm: Option<&dyn ChatModel>) -> String {
        let question = self.question.trim();
        let Some(previous) = self.history.iter().rev().find(|m| m.role == "user") else {
            return clip(question);
        };
        if let Some(llm) = llm.filter(|_| self.state.config.chat_condense_question) {
            let req = prompt::condense_request(question, &self.history);
            match tokio::time::timeout(CONDENSE_TIMEOUT, llm.complete(&req)).await {
                Ok(Ok(c)) if c.stop != StopReason::Refusal => {
                    let line = c.text.lines().find(|l| !l.trim().is_empty()).unwrap_or("");
                    let line = line.trim().trim_matches('"').trim();
                    if !line.is_empty() {
                        return clip(line);
                    }
                }
                Ok(Ok(_)) => {}
                Ok(Err(err)) => tracing::warn!(%err, "rewriting the follow-up failed"),
                Err(_) => tracing::warn!("rewriting the follow-up timed out"),
            }
        }
        clip(&format!("{} {question}", previous.content.trim()))
    }

    async fn store(&self, o: &Outcome, latency_ms: u64) -> Option<Uuid> {
        let citations = serde_json::to_value(&o.citations).unwrap_or_default();
        let new = NewMessage {
            conversation_id: self.conversation_id,
            owner_id: self.owner_id,
            role: "assistant",
            content: &o.content,
            status: o.status.as_str(),
            citations,
            model: o.model.as_deref(),
            input_tokens: o.usage.input_tokens.and_then(|n| i32::try_from(n).ok()),
            output_tokens: o.usage.output_tokens.and_then(|n| i32::try_from(n).ok()),
            latency_ms: i32::try_from(latency_ms).ok(),
        };
        let mut conn = match self.state.db.acquire().await {
            Ok(conn) => conn,
            Err(err) => {
                tracing::error!(%err, "storing a chat answer failed");
                return None;
            }
        };
        match akasha_db::chat::insert_message(&mut conn, &new).await {
            Ok(m) => m.map(|m| m.id),
            Err(err) => {
                tracing::error!(%err, "storing a chat answer failed");
                None
            }
        }
    }
}

fn reranker_name(models: &Models, reranked: bool) -> Option<&'static str> {
    match &models.reranker {
        Ok(Some(r)) if reranked => Some(r.name()),
        _ => None,
    }
}

fn clip(text: &str) -> String {
    text.chars().take(MAX_QUERY_CHARS).collect()
}
