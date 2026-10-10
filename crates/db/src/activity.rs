//! The activity timeline and security audit log (`activity_events`).
//!
//! Rows belong to one user and are only ever read back by that user. Writers
//! record inside the transaction of the action when there is one (so an event
//! exists exactly when the change committed), otherwise best-effort.

use chrono::{DateTime, Utc};
use sqlx::{PgConnection, PgPool};
use uuid::Uuid;

/// Longest `subject` stored, in characters.
pub const SUBJECT_MAX: usize = 500;
const USER_AGENT_MAX: usize = 256;
const IP_MAX: usize = 64;
/// Rows deleted per statement when pruning.
const PRUNE_BATCH: i64 = 5_000;

/// The kind of searches in the timeline.
pub const SEARCH_KIND: &str = "search.performed";

/// A stored event.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct Event {
    pub id: Uuid,
    pub kind: String,
    pub category: String,
    pub subject: Option<String>,
    pub file_id: Option<Uuid>,
    pub collection_id: Option<Uuid>,
    pub conversation_id: Option<Uuid>,
    pub details: serde_json::Value,
    pub via: Option<String>,
    pub ip: Option<String>,
    pub user_agent: Option<String>,
    pub created_at: DateTime<Utc>,
}

/// An event to record. `kind` is `area.verb` (`file.renamed`); `category` one of
/// `files`, `search`, `chat`, `collections`, `security`.
#[derive(Debug, Clone)]
pub struct NewEvent<'a> {
    pub owner_id: Uuid,
    pub kind: &'a str,
    pub category: &'a str,
    pub subject: Option<&'a str>,
    pub file_id: Option<Uuid>,
    pub collection_id: Option<Uuid>,
    pub conversation_id: Option<Uuid>,
    pub details: serde_json::Value,
    pub via: Option<&'a str>,
    pub ip: Option<&'a str>,
    pub user_agent: Option<&'a str>,
}

impl<'a> NewEvent<'a> {
    /// An event with no links or details yet.
    pub fn new(owner_id: Uuid, kind: &'a str, category: &'a str) -> Self {
        Self {
            owner_id,
            kind,
            category,
            subject: None,
            file_id: None,
            collection_id: None,
            conversation_id: None,
            details: serde_json::json!({}),
            via: None,
            ip: None,
            user_agent: None,
        }
    }
}

fn cut(s: Option<&str>, max: usize) -> Option<String> {
    s.map(|s| s.chars().take(max).collect())
}

/// Record an event (use the action's transaction when it has one).
pub async fn record(conn: &mut PgConnection, ev: &NewEvent<'_>) -> Result<(), sqlx::Error> {
    sqlx::query!(
        r#"INSERT INTO activity_events
               (owner_id, kind, category, subject, file_id, collection_id, conversation_id,
                details, via, ip, user_agent)
           VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)"#,
        ev.owner_id,
        ev.kind,
        ev.category,
        cut(ev.subject, SUBJECT_MAX),
        ev.file_id,
        ev.collection_id,
        ev.conversation_id,
        ev.details,
        ev.via,
        cut(ev.ip, IP_MAX),
        cut(ev.user_agent, USER_AGENT_MAX),
    )
    .execute(conn)
    .await?;
    Ok(())
}

/// Record `ev` unless an event of the same kind about the same file (or, without
/// a file, any of that kind) was recorded within `window_secs`. Keeps repeated
/// opens and rate-limit hits from flooding the timeline. Returns whether it was
/// recorded.
pub async fn record_unless_recent(
    conn: &mut PgConnection,
    ev: &NewEvent<'_>,
    window_secs: f64,
) -> Result<bool, sqlx::Error> {
    let done = sqlx::query!(
        r#"INSERT INTO activity_events
               (owner_id, kind, category, subject, file_id, collection_id, conversation_id,
                details, via, ip, user_agent)
           SELECT $1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11
           WHERE NOT EXISTS (
               SELECT 1 FROM activity_events
               WHERE owner_id = $1 AND kind = $2
                 AND file_id IS NOT DISTINCT FROM $5
                 AND created_at > now() - make_interval(secs => $12)
           )"#,
        ev.owner_id,
        ev.kind,
        ev.category,
        cut(ev.subject, SUBJECT_MAX),
        ev.file_id,
        ev.collection_id,
        ev.conversation_id,
        ev.details,
        ev.via,
        cut(ev.ip, IP_MAX),
        cut(ev.user_agent, USER_AGENT_MAX),
        window_secs,
    )
    .execute(conn)
    .await?;
    Ok(done.rows_affected() > 0)
}

/// Record a search, if the owner keeps a search history. Typing refines one
/// query over several requests ("bud", "budget", "budget 2026"): when the
/// owner's latest search, less than two minutes old, is a prefix of this one (or
/// the other way round), it is updated instead of adding another row.
pub async fn record_search(
    pool: &PgPool,
    owner_id: Uuid,
    query: &str,
    details: serde_json::Value,
    via: &str,
) -> Result<(), sqlx::Error> {
    let query = cut(Some(query), SUBJECT_MAX).unwrap_or_default();
    sqlx::query!(
        r#"WITH allowed AS (
               SELECT 1 FROM users WHERE id = $1 AND record_search_history
           ), prev AS (
               SELECT id, subject FROM activity_events
               WHERE owner_id = $1 AND kind = $5
                 AND created_at > now() - interval '2 minutes'
               ORDER BY created_at DESC, id DESC LIMIT 1
           ), merge AS (
               SELECT prev.id FROM prev, allowed
               WHERE starts_with(lower($2), lower(prev.subject))
                  OR starts_with(lower(prev.subject), lower($2))
           ), upd AS (
               UPDATE activity_events a
               SET subject = $2, details = $3, via = $4, created_at = now()
               FROM merge WHERE a.id = merge.id
               RETURNING a.id
           )
           INSERT INTO activity_events (owner_id, kind, category, subject, details, via)
           SELECT $1, $5, 'search', $2, $3, $4
           WHERE EXISTS (SELECT 1 FROM allowed) AND NOT EXISTS (SELECT 1 FROM merge)"#,
        owner_id,
        query,
        details,
        via,
        SEARCH_KIND,
    )
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn keeps_search_history(pool: &PgPool, owner_id: Uuid) -> Result<bool, sqlx::Error> {
    Ok(sqlx::query_scalar!(
        "SELECT record_search_history FROM users WHERE id = $1",
        owner_id
    )
    .fetch_optional(pool)
    .await?
    .unwrap_or(false))
}

/// Turn the owner's search history on or off.
pub async fn set_search_history(
    pool: &PgPool,
    owner_id: Uuid,
    on: bool,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "UPDATE users SET record_search_history = $2 WHERE id = $1",
        owner_id,
        on
    )
    .execute(pool)
    .await?;
    Ok(())
}

/// Filters for [`list`]; empty means "any".
#[derive(Debug, Default, Clone)]
pub struct ListFilter {
    pub categories: Vec<String>,
    pub kinds: Vec<String>,
    pub file_id: Option<Uuid>,
    /// Keyset cursor: only events older than `(created_at, id)`.
    pub before: Option<(DateTime<Utc>, Uuid)>,
    pub limit: i64,
}

/// The owner's events, newest first.
pub async fn list(
    pool: &PgPool,
    owner_id: Uuid,
    filter: &ListFilter,
) -> Result<Vec<Event>, sqlx::Error> {
    let (before_at, before_id) = filter.before.unzip();
    sqlx::query_as!(
        Event,
        r#"SELECT id, kind, category, subject, file_id, collection_id, conversation_id,
                  details, via, ip, user_agent, created_at
           FROM activity_events
           WHERE owner_id = $1
             AND (cardinality($2::text[]) = 0 OR category = ANY($2))
             AND (cardinality($3::text[]) = 0 OR kind = ANY($3))
             AND ($4::uuid IS NULL OR file_id = $4)
             AND ($5::timestamptz IS NULL OR (created_at, id) < ($5, $6::uuid))
           ORDER BY created_at DESC, id DESC
           LIMIT $7"#,
        owner_id,
        &filter.categories,
        &filter.kinds,
        filter.file_id,
        before_at,
        before_id,
        filter.limit,
    )
    .fetch_all(pool)
    .await
}

/// Delete the owner's events in these categories (never `security`: the audit
/// log is kept until retention prunes it). Returns how many were deleted.
pub async fn clear(
    pool: &PgPool,
    owner_id: Uuid,
    categories: &[String],
) -> Result<u64, sqlx::Error> {
    let done = sqlx::query!(
        r#"DELETE FROM activity_events
           WHERE owner_id = $1 AND category <> 'security'
             AND (cardinality($2::text[]) = 0 OR category = ANY($2))"#,
        owner_id,
        categories,
    )
    .execute(pool)
    .await?;
    Ok(done.rows_affected())
}

/// Delete events older than `days` days, in batches. Returns how many.
pub async fn prune(pool: &PgPool, days: u32) -> Result<u64, sqlx::Error> {
    let mut total = 0;
    loop {
        let done = sqlx::query!(
            r#"DELETE FROM activity_events WHERE id IN (
                   SELECT id FROM activity_events
                   WHERE created_at < now() - make_interval(days => $1)
                   LIMIT $2
               )"#,
            i32::try_from(days).unwrap_or(i32::MAX),
            PRUNE_BATCH,
        )
        .execute(pool)
        .await?;
        total += done.rows_affected();
        if done.rows_affected() < PRUNE_BATCH as u64 {
            return Ok(total);
        }
    }
}

#[cfg(test)]
mod tests;
