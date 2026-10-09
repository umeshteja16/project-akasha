//! `POST /api/v1/conversations/{id}/messages`: ask a question, get a grounded
//! answer streamed as Server-Sent Events.

use std::{convert::Infallible, time::Instant};

use akasha_search::ChunkFilter;
use axum::{
    extract::{Path, State},
    response::sse::{Event, KeepAlive, Sse},
};
use futures_util::{Stream, stream};
use tokio::sync::mpsc;
use uuid::Uuid;

use super::{
    not_found,
    types::{MAX_QUESTION_CHARS, PostMessage, auto_title, check_scope},
};
use crate::{
    auth::AuthUser,
    chat::turn::Turn,
    error::{ApiError, ErrorBody},
    extract::Json,
    rate_limit,
    routes::files::types::normalize_tags,
    state::AppState,
};
use akasha_core::Error;
use akasha_db::chat::{self, NewMessage};

/// Events buffered between the answer task and a slow client.
const BUFFER: usize = 64;

/// Ask a question about your files.
///
/// The answer streams as `text/event-stream`: one `sources` event (the
/// numbered passages the answer may cite, see `ChatSources`), `delta` events
/// with pieces of text (`ChatDelta`), then `done` (`ChatDone`: the stored
/// message id, status, final text, citations and token usage) or `error`
/// (`ChatError`). Answers cite sources as `[n]`.
///
/// When the best passage is too weak, the answer is "I couldn't find this in
/// your files." without asking the model (`status: refused`); with no model
/// configured, the passages are returned with `status: no_llm`. Closing the
/// connection stops generation. Limited per user (`AKASHA_CHAT_RATE_PER_MINUTE`).
#[utoipa::path(
    post, path = "/api/v1/conversations/{id}/messages", tag = "chat", operation_id = "post_message",
    params(("id" = Uuid, Path, description = "Conversation id")),
    request_body = PostMessage,
    responses(
        (status = 200, description = "Server-Sent Events: `sources`, `delta`*, then `done` or `error`",
         content_type = "text/event-stream", body = String),
        (status = 400, body = ErrorBody), (status = 401, body = ErrorBody),
        (status = 404, body = ErrorBody), (status = 429, body = ErrorBody),
    ),
    security(("session_cookie" = []))
)]
pub async fn post(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<PostMessage>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, ApiError> {
    let started = Instant::now();
    rate_limit::check_user(&state.chat_limiter, auth.user_id, "questions")?;
    let question = body.content.trim().to_owned();
    if question.is_empty() || question.chars().count() > MAX_QUESTION_CHARS {
        return Err(Error::bad_request(format!(
            "content must be 1-{MAX_QUESTION_CHARS} characters"
        ))
        .into());
    }
    let conversation = chat::get_conversation(&state.db, auth.user_id, id)
        .await?
        .ok_or_else(not_found)?;
    let filter = filter(&body, conversation.file_ids)?;
    let history_len = i64::from(state.config.chat_history_messages);
    let history = chat::history(&state.db, auth.user_id, id, history_len).await?;

    let mut tx = state.db.begin().await?;
    let stored = chat::insert_message(
        &mut tx,
        &NewMessage {
            conversation_id: id,
            owner_id: auth.user_id,
            role: "user",
            content: &question,
            status: "answered",
            citations: serde_json::json!([]),
            model: None,
            input_tokens: None,
            output_tokens: None,
            latency_ms: None,
        },
    )
    .await?
    .ok_or_else(not_found)?;
    chat::set_title_if_empty(&mut tx, auth.user_id, id, &auto_title(&question)).await?;
    tx.commit().await?;

    let turn = Turn {
        state: state.clone(),
        owner_id: auth.user_id,
        conversation_id: id,
        user_message_id: stored.id,
        question,
        history,
        filter,
        started,
    };
    let (events_tx, events_rx) = mpsc::channel(BUFFER);
    tokio::spawn(turn.run(events_tx));
    let events = stream::unfold(events_rx, |mut rx| async move {
        rx.recv().await.map(|event| (Ok(event), rx))
    });
    Ok(Sse::new(events).keep_alive(KeepAlive::default()))
}

/// The question's own scope, else the conversation's.
fn filter(body: &PostMessage, scope: Vec<uuid::Uuid>) -> Result<ChunkFilter, Error> {
    let file_ids = body.file_ids.clone().unwrap_or(scope);
    check_scope(&file_ids)?;
    Ok(ChunkFilter {
        mime_patterns: body
            .file_type
            .map(|t| t.mime_patterns())
            .unwrap_or_default(),
        tags: normalize_tags(body.tags.as_deref().unwrap_or_default())?,
        file_ids,
        ..ChunkFilter::default()
    })
}
