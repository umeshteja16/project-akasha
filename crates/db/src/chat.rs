//! Conversations and their messages (grounded chat). Every query takes the
//! owner's id: another user's conversation is simply not found.

use chrono::{DateTime, Utc};
use sqlx::{PgConnection, PgPool};
use uuid::Uuid;

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct Conversation {
    pub id: Uuid,
    pub owner_id: Uuid,
    pub title: String,
    /// Files the conversation answers from by default; empty: all files.
    pub file_ids: Vec<Uuid>,
    /// The collection the conversation answers from, if any (combined with
    /// `file_ids` when both are set).
    pub collection_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct Message {
    pub id: Uuid,
    pub conversation_id: Uuid,
    pub role: String,
    pub content: String,
    pub status: String,
    pub citations: serde_json::Value,
    pub model: Option<String>,
    pub input_tokens: Option<i32>,
    pub output_tokens: Option<i32>,
    pub latency_ms: Option<i32>,
    pub created_at: DateTime<Utc>,
}

pub struct NewMessage<'a> {
    pub conversation_id: Uuid,
    pub owner_id: Uuid,
    /// `user` or `assistant`.
    pub role: &'a str,
    pub content: &'a str,
    /// `answered`, `refused`, `no_llm`, `cancelled` or `error`.
    pub status: &'a str,
    pub citations: serde_json::Value,
    pub model: Option<&'a str>,
    pub input_tokens: Option<i32>,
    pub output_tokens: Option<i32>,
    pub latency_ms: Option<i32>,
}

/// Start a conversation. A `collection_id` that is not the owner's is dropped.
pub async fn create_conversation(
    pool: &PgPool,
    owner_id: Uuid,
    title: &str,
    file_ids: &[Uuid],
    collection_id: Option<Uuid>,
) -> Result<Conversation, sqlx::Error> {
    sqlx::query_as!(
        Conversation,
        "INSERT INTO conversations (owner_id, title, file_ids, collection_id)
         VALUES ($1, $2, ARRAY(SELECT f.id FROM files f
                               WHERE f.owner_id = $1 AND f.id = ANY($3::uuid[])
                               ORDER BY array_position($3::uuid[], f.id)),
                 (SELECT c.id FROM collections c WHERE c.owner_id = $1 AND c.id = $4))
         RETURNING id, owner_id, title, file_ids, collection_id, created_at, updated_at",
        owner_id,
        title,
        file_ids,
        collection_id
    )
    .fetch_one(pool)
    .await
}

/// Most recently active first. `before` is a keyset cursor `(updated_at, id)`.
pub async fn list_conversations(
    pool: &PgPool,
    owner_id: Uuid,
    before: Option<(DateTime<Utc>, Uuid)>,
    limit: i64,
) -> Result<Vec<Conversation>, sqlx::Error> {
    let (before_at, before_id) = before.unzip();
    sqlx::query_as!(
        Conversation,
        "SELECT id, owner_id, title, file_ids, collection_id, created_at, updated_at FROM conversations
         WHERE owner_id = $1
           AND ($2::timestamptz IS NULL OR (updated_at, id) < ($2, $3::uuid))
         ORDER BY updated_at DESC, id DESC
         LIMIT $4",
        owner_id,
        before_at,
        before_id,
        limit
    )
    .fetch_all(pool)
    .await
}

pub async fn get_conversation(
    pool: &PgPool,
    owner_id: Uuid,
    id: Uuid,
) -> Result<Option<Conversation>, sqlx::Error> {
    sqlx::query_as!(
        Conversation,
        "SELECT id, owner_id, title, file_ids, collection_id, created_at, updated_at FROM conversations
         WHERE id = $1 AND owner_id = $2",
        id,
        owner_id
    )
    .fetch_optional(pool)
    .await
}

pub async fn rename_conversation(
    conn: &mut PgConnection,
    owner_id: Uuid,
    id: Uuid,
    title: &str,
) -> Result<Option<Conversation>, sqlx::Error> {
    sqlx::query_as!(
        Conversation,
        "UPDATE conversations SET title = $3, title_source = 'user' WHERE id = $1 AND owner_id = $2
         RETURNING id, owner_id, title, file_ids, collection_id, created_at, updated_at",
        id,
        owner_id,
        title
    )
    .fetch_optional(conn)
    .await
}

/// Set the files a conversation answers from (empty: all). Ids that are not
/// the owner's files are dropped; the order is kept.
pub async fn set_conversation_scope(
    conn: &mut PgConnection,
    owner_id: Uuid,
    id: Uuid,
    file_ids: &[Uuid],
) -> Result<Option<Conversation>, sqlx::Error> {
    sqlx::query_as!(
        Conversation,
        "UPDATE conversations
         SET file_ids = ARRAY(SELECT f.id FROM files f
                              WHERE f.owner_id = $2 AND f.id = ANY($3::uuid[])
                              ORDER BY array_position($3::uuid[], f.id))
         WHERE id = $1 AND owner_id = $2
         RETURNING id, owner_id, title, file_ids, collection_id, created_at, updated_at",
        id,
        owner_id,
        file_ids
    )
    .fetch_optional(conn)
    .await
}

/// Set (or clear) the collection a conversation answers from. A collection that
/// is not the owner's clears it (callers check first).
pub async fn set_conversation_collection(
    conn: &mut PgConnection,
    owner_id: Uuid,
    id: Uuid,
    collection_id: Option<Uuid>,
) -> Result<Option<Conversation>, sqlx::Error> {
    sqlx::query_as!(
        Conversation,
        "UPDATE conversations
         SET collection_id = (SELECT c.id FROM collections c
                              WHERE c.owner_id = $2 AND c.id = $3)
         WHERE id = $1 AND owner_id = $2
         RETURNING id, owner_id, title, file_ids, collection_id, created_at, updated_at",
        id,
        owner_id,
        collection_id
    )
    .fetch_optional(conn)
    .await
}

/// Deletes the conversation and its messages; `false` if there was none.
pub async fn delete_conversation(
    pool: &PgPool,
    owner_id: Uuid,
    id: Uuid,
) -> Result<bool, sqlx::Error> {
    let res = sqlx::query!(
        "DELETE FROM conversations WHERE id = $1 AND owner_id = $2",
        id,
        owner_id
    )
    .execute(pool)
    .await?;
    Ok(res.rows_affected() > 0)
}

/// Give an untitled conversation its first title, from the question (a rename
/// by the user wins; a model-written title may replace it later, see [`titles`]).
pub async fn set_title_if_empty(
    conn: &mut PgConnection,
    owner_id: Uuid,
    id: Uuid,
    title: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "UPDATE conversations SET title = $3, title_source = 'question'
         WHERE id = $1 AND owner_id = $2 AND title = ''",
        id,
        owner_id,
        title
    )
    .execute(conn)
    .await?;
    Ok(())
}

/// Append a message and mark the conversation as active. `None` if the
/// conversation does not exist (any more) for this owner.
pub async fn insert_message(
    conn: &mut PgConnection,
    m: &NewMessage<'_>,
) -> Result<Option<Message>, sqlx::Error> {
    let touched = sqlx::query!(
        "UPDATE conversations SET updated_at = now() WHERE id = $1 AND owner_id = $2",
        m.conversation_id,
        m.owner_id
    )
    .execute(&mut *conn)
    .await?;
    if touched.rows_affected() == 0 {
        return Ok(None);
    }
    sqlx::query_as!(
        Message,
        "INSERT INTO messages (conversation_id, owner_id, role, content, status, citations,
                               model, input_tokens, output_tokens, latency_ms)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
         RETURNING id, conversation_id, role, content, status, citations, model,
                   input_tokens, output_tokens, latency_ms, created_at",
        m.conversation_id,
        m.owner_id,
        m.role,
        m.content,
        m.status,
        m.citations,
        m.model,
        m.input_tokens,
        m.output_tokens,
        m.latency_ms
    )
    .fetch_one(conn)
    .await
    .map(Some)
}

/// A page of messages, newest first. `before` is a keyset cursor `(created_at, id)`.
pub async fn list_messages(
    pool: &PgPool,
    owner_id: Uuid,
    conversation_id: Uuid,
    before: Option<(DateTime<Utc>, Uuid)>,
    limit: i64,
) -> Result<Vec<Message>, sqlx::Error> {
    let (before_at, before_id) = before.unzip();
    sqlx::query_as!(
        Message,
        "SELECT id, conversation_id, role, content, status, citations, model,
                input_tokens, output_tokens, latency_ms, created_at
         FROM messages
         WHERE conversation_id = $1 AND owner_id = $2
           AND ($3::timestamptz IS NULL OR (created_at, id) < ($3, $4::uuid))
         ORDER BY created_at DESC, id DESC
         LIMIT $5",
        conversation_id,
        owner_id,
        before_at,
        before_id,
        limit
    )
    .fetch_all(pool)
    .await
}

/// The last `limit` turns worth showing a model, oldest first: questions and
/// answered replies (refusals, errors and cancelled replies are left out).
pub async fn history(
    pool: &PgPool,
    owner_id: Uuid,
    conversation_id: Uuid,
    limit: i64,
) -> Result<Vec<Message>, sqlx::Error> {
    let mut rows = sqlx::query_as!(
        Message,
        "SELECT id, conversation_id, role, content, status, citations, model,
                input_tokens, output_tokens, latency_ms, created_at
         FROM messages
         WHERE conversation_id = $1 AND owner_id = $2
           AND (role = 'user' OR status = 'answered')
         ORDER BY created_at DESC, id DESC
         LIMIT $3",
        conversation_id,
        owner_id,
        limit
    )
    .fetch_all(pool)
    .await?;
    rows.reverse();
    Ok(rows)
}

pub mod titles;
