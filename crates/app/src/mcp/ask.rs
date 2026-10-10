//! The `ask` tool: a grounded answer with citations, in one response (the
//! same retrieval, refusal gate and prompt as chat, without storing a
//! conversation). Without a chat model it returns the passages.

use std::time::Duration;

use akasha_llm::StopReason;
use akasha_search::{ChunkFilter, SearchMode, SearchRequest};
use serde_json::json;

use super::{
    Caller,
    output::{ToolError, ToolResult, clip},
    tools::AskArgs,
};
use crate::{
    chat::{
        citations::{Source, cited},
        evidence::{self, Evidence},
        generate::is_not_found,
        prompt,
    },
    rate_limit,
    routes::{chat::types::MAX_QUESTION_CHARS, search::models},
    state::AppState,
};

/// Candidates retrieved per question (as in chat).
const RETRIEVE: usize = 20;
const MAX_FILE_IDS: usize = 100;
/// The model gets this long to answer.
const ANSWER_TIMEOUT: Duration = Duration::from_secs(90);
/// Characters of each passage returned without a model.
const PASSAGE_CHARS: usize = 1_200;

pub async fn ask(state: &AppState, caller: Caller, a: AskArgs) -> ToolResult {
    let question = a.question.trim();
    if question.is_empty() || question.chars().count() > MAX_QUESTION_CHARS {
        return Err(ToolError::new(format!(
            "question must be 1-{MAX_QUESTION_CHARS} characters"
        )));
    }
    let file_ids = a.file_ids.unwrap_or_default();
    if file_ids.len() > MAX_FILE_IDS {
        return Err(ToolError::new(format!("at most {MAX_FILE_IDS} file_ids")));
    }
    rate_limit::check_user(&state.chat_limiter, caller.user_id, "questions")?;
    let req = SearchRequest {
        query: question
            .chars()
            .take(akasha_search::MAX_QUERY_CHARS)
            .collect(),
        mode: SearchMode::Hybrid,
        filter: ChunkFilter {
            file_ids,
            ..ChunkFilter::default()
        },
        limit: RETRIEVE,
        offset: 0,
        rerank: true,
        include_weak: false,
    };
    let models = models(state).await;
    let res = akasha_search::search_chunks(&state.db, caller.user_id, &req, &models).await?;
    let reranker = match &models.reranker {
        Ok(Some(r)) if res.meta.reranked => Some(r.name()),
        _ => None,
    };
    let config = &state.config;
    let sources = match evidence::assess(&res.results, reranker, config.chat_min_rerank_score) {
        Evidence::Sufficient => {
            prompt::select_sources(&res.results, config.chat_context_chunks as usize)
        }
        Evidence::Nothing | Evidence::Weak { .. } => Vec::new(),
    };
    if sources.is_empty() {
        return Ok(json!({ "status": "not_found", "answer": prompt::NOT_FOUND, "citations": [] }));
    }
    let Some(llm) = state.llm.clone() else {
        return Ok(json!({
            "status": "no_llm",
            "answer": "No language model is configured on the server; these are the most \
                       relevant passages. Answer from them and cite them.",
            "passages": passages(&sources),
        }));
    };
    let request = prompt::answer_request(
        question,
        &[],
        &sources,
        config.llm_max_tokens,
        config.llm_temperature,
    );
    let completion = match tokio::time::timeout(ANSWER_TIMEOUT, llm.complete(&request)).await {
        Ok(Ok(c)) => c,
        Ok(Err(err)) => {
            tracing::warn!(%err, provider = llm.provider(), "MCP ask: generation failed");
            return Ok(fallback(&sources, "the language model failed"));
        }
        Err(_) => return Ok(fallback(&sources, "the language model timed out")),
    };
    if completion.stop == StopReason::Refusal || is_not_found(&completion.text) {
        return Ok(json!({ "status": "not_found", "answer": prompt::NOT_FOUND, "citations": [] }));
    }
    let citations = cited(&completion.text, &sources);
    Ok(json!({
        "status": "answered",
        "answer": clip(&completion.text, 12_000),
        "citations": citations,
        "model": format!("{}/{}", llm.provider(), llm.model()),
    }))
}

fn fallback(sources: &[Source], why: &str) -> serde_json::Value {
    json!({
        "status": "error",
        "answer": format!("{why}; these are the most relevant passages instead."),
        "passages": passages(sources),
    })
}

fn passages(sources: &[Source]) -> Vec<serde_json::Value> {
    sources
        .iter()
        .map(|s| {
            let c = &s.citation;
            json!({
                "n": c.n,
                "file_id": c.file_id,
                "file_name": c.file_name,
                "page": c.page,
                "chunk_id": c.chunk_id,
                "char_start": c.char_start,
                "char_end": c.char_end,
                "text": clip(s.text.trim(), PASSAGE_CHARS),
            })
        })
        .collect()
}
