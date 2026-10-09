//! `/api/v1/conversations`: grounded chat conversations and their messages.
//! Every query is filtered by the signed-in owner; other users' conversations
//! are 404.

pub mod messages;
pub mod types;

use axum::{
    extract::{Path, State},
    http::StatusCode,
};
use uuid::Uuid;

use self::types::{
    ConversationList, ConversationResponse, CreateConversation, MessageList, PageQuery,
    UpdateConversation, clean_title,
};
use crate::{
    auth::AuthUser,
    error::{ApiError, ErrorBody},
    extract::{Json, Query},
    routes::cursor::{decode_cursor, encode_cursor},
    state::AppState,
};
use akasha_core::Error;
use akasha_db::chat;

const MAX_PAGE: i64 = 100;

pub(super) fn not_found() -> ApiError {
    Error::not_found("conversation not found").into()
}

fn page_size(limit: Option<i64>, default: i64) -> Result<i64, ApiError> {
    let limit = limit.unwrap_or(default);
    if (1..=MAX_PAGE).contains(&limit) {
        Ok(limit)
    } else {
        Err(Error::bad_request(format!("limit must be 1-{MAX_PAGE}")).into())
    }
}

/// Start a conversation.
#[utoipa::path(
    post, path = "/api/v1/conversations", tag = "chat", operation_id = "create_conversation",
    request_body = CreateConversation,
    responses(
        (status = 201, body = ConversationResponse),
        (status = 400, body = ErrorBody), (status = 401, body = ErrorBody),
    ),
    security(("session_cookie" = []))
)]
pub async fn create(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(body): Json<CreateConversation>,
) -> Result<(StatusCode, Json<ConversationResponse>), ApiError> {
    let title = clean_title(body.title.as_deref().unwrap_or(""), true)?;
    let conversation = chat::create_conversation(&state.db, auth.user_id, &title).await?;
    Ok((StatusCode::CREATED, Json(conversation.into())))
}

/// List your conversations, most recently active first.
#[utoipa::path(
    get, path = "/api/v1/conversations", tag = "chat", operation_id = "list_conversations",
    params(PageQuery),
    responses(
        (status = 200, body = ConversationList),
        (status = 400, body = ErrorBody), (status = 401, body = ErrorBody),
    ),
    security(("session_cookie" = []))
)]
pub async fn list(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(query): Query<PageQuery>,
) -> Result<Json<ConversationList>, ApiError> {
    let limit = page_size(query.limit, 30)?;
    let before = query.cursor.as_deref().map(decode_cursor).transpose()?;
    let mut rows = chat::list_conversations(&state.db, auth.user_id, before, limit + 1).await?;
    let has_more = rows.len() as i64 > limit;
    rows.truncate(usize::try_from(limit).unwrap_or(0));
    let next_cursor = has_more
        .then(|| rows.last().map(|c| encode_cursor(c.updated_at, c.id)))
        .flatten();
    Ok(Json(ConversationList {
        items: rows.into_iter().map(Into::into).collect(),
        next_cursor,
    }))
}

/// One of your conversations.
#[utoipa::path(
    get, path = "/api/v1/conversations/{id}", tag = "chat", operation_id = "get_conversation",
    params(("id" = Uuid, Path, description = "Conversation id")),
    responses(
        (status = 200, body = ConversationResponse),
        (status = 401, body = ErrorBody), (status = 404, body = ErrorBody),
    ),
    security(("session_cookie" = []))
)]
pub async fn get(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<ConversationResponse>, ApiError> {
    let conversation = chat::get_conversation(&state.db, auth.user_id, id)
        .await?
        .ok_or_else(not_found)?;
    Ok(Json(conversation.into()))
}

/// Rename a conversation.
#[utoipa::path(
    patch, path = "/api/v1/conversations/{id}", tag = "chat", operation_id = "update_conversation",
    params(("id" = Uuid, Path, description = "Conversation id")),
    request_body = UpdateConversation,
    responses(
        (status = 200, body = ConversationResponse),
        (status = 400, body = ErrorBody), (status = 401, body = ErrorBody),
        (status = 404, body = ErrorBody),
    ),
    security(("session_cookie" = []))
)]
pub async fn update(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<UpdateConversation>,
) -> Result<Json<ConversationResponse>, ApiError> {
    let title = clean_title(&body.title, false)?;
    let conversation = chat::rename_conversation(&state.db, auth.user_id, id, &title)
        .await?
        .ok_or_else(not_found)?;
    Ok(Json(conversation.into()))
}

/// Delete a conversation and its messages.
#[utoipa::path(
    delete, path = "/api/v1/conversations/{id}", tag = "chat", operation_id = "delete_conversation",
    params(("id" = Uuid, Path, description = "Conversation id")),
    responses(
        (status = 204, description = "Deleted"),
        (status = 401, body = ErrorBody), (status = 404, body = ErrorBody),
    ),
    security(("session_cookie" = []))
)]
pub async fn delete(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, ApiError> {
    if chat::delete_conversation(&state.db, auth.user_id, id).await? {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(not_found())
    }
}

/// The messages of a conversation, newest page first (oldest first within a page).
#[utoipa::path(
    get, path = "/api/v1/conversations/{id}/messages", tag = "chat",
    params(("id" = Uuid, Path, description = "Conversation id"), PageQuery),
    responses(
        (status = 200, body = MessageList),
        (status = 400, body = ErrorBody), (status = 401, body = ErrorBody),
        (status = 404, body = ErrorBody),
    ),
    security(("session_cookie" = []))
)]
pub async fn list_messages(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Query(query): Query<PageQuery>,
) -> Result<Json<MessageList>, ApiError> {
    let limit = page_size(query.limit, 50)?;
    let before = query.cursor.as_deref().map(decode_cursor).transpose()?;
    chat::get_conversation(&state.db, auth.user_id, id)
        .await?
        .ok_or_else(not_found)?;
    let mut rows = chat::list_messages(&state.db, auth.user_id, id, before, limit + 1).await?;
    let has_more = rows.len() as i64 > limit;
    rows.truncate(usize::try_from(limit).unwrap_or(0));
    let next_cursor = has_more
        .then(|| rows.last().map(|m| encode_cursor(m.created_at, m.id)))
        .flatten();
    rows.reverse();
    Ok(Json(MessageList {
        items: rows.into_iter().map(Into::into).collect(),
        next_cursor,
    }))
}
