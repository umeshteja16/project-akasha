//! Model-written conversation titles (the `title_conversation` job). Only a
//! title taken from the first question (`title_source = 'question'`) is ever
//! replaced, so a rename by the user always wins.

use sqlx::PgPool;
use uuid::Uuid;

/// The opening exchange a title is written from.
#[derive(Debug, Clone)]
pub struct Opening {
    pub question: String,
    pub answer: String,
}

/// The first question and its answer, if the conversation still has its
/// question-based title and an answered reply. `None` otherwise (gone,
/// renamed, already titled by the model, or nothing answered yet).
pub async fn opening(pool: &PgPool, conversation_id: Uuid) -> Result<Option<Opening>, sqlx::Error> {
    sqlx::query_as!(
        Opening,
        r#"SELECT q.content AS question, a.content AS answer
           FROM conversations c
           CROSS JOIN LATERAL (
               SELECT content, created_at FROM messages
               WHERE conversation_id = c.id AND role = 'user'
               ORDER BY created_at, id LIMIT 1) q
           CROSS JOIN LATERAL (
               SELECT content FROM messages
               WHERE conversation_id = c.id AND role = 'assistant' AND status = 'answered'
               ORDER BY created_at, id LIMIT 1) a
           WHERE c.id = $1 AND c.title_source = 'question'"#,
        conversation_id
    )
    .fetch_optional(pool)
    .await
}

/// Replace a question-based title. `false` if the user renamed the
/// conversation meanwhile (or it is gone, or was titled already).
pub async fn set_model_title(
    pool: &PgPool,
    conversation_id: Uuid,
    title: &str,
) -> Result<bool, sqlx::Error> {
    let res = sqlx::query!(
        "UPDATE conversations SET title = $2, title_source = 'model'
         WHERE id = $1 AND title_source = 'question'",
        conversation_id,
        title
    )
    .execute(pool)
    .await?;
    Ok(res.rows_affected() > 0)
}
