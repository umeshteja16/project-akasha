//! Login sessions. Callers pass the SHA-256 of the cookie token, never the token itself.

use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

/// A live session joined with its user's identity.
#[derive(Debug, Clone)]
pub struct ActiveSession {
    pub session_id: Uuid,
    pub user_id: Uuid,
    pub expires_at: DateTime<Utc>,
}

pub async fn create(
    pool: &PgPool,
    user_id: Uuid,
    token_hash: &[u8],
    expires_at: DateTime<Utc>,
    user_agent: Option<&str>,
) -> Result<Uuid, sqlx::Error> {
    sqlx::query_scalar!(
        r#"INSERT INTO sessions (user_id, token_hash, expires_at, user_agent)
           VALUES ($1, $2, $3, $4) RETURNING id"#,
        user_id,
        token_hash,
        expires_at,
        user_agent,
    )
    .fetch_one(pool)
    .await
}

/// Look up an unexpired session and mark it as seen.
pub async fn touch(pool: &PgPool, token_hash: &[u8]) -> Result<Option<ActiveSession>, sqlx::Error> {
    sqlx::query_as!(
        ActiveSession,
        r#"UPDATE sessions SET last_seen_at = now()
           WHERE token_hash = $1 AND expires_at > now()
           RETURNING id AS session_id, user_id, expires_at"#,
        token_hash,
    )
    .fetch_optional(pool)
    .await
}

pub async fn delete(pool: &PgPool, session_id: Uuid) -> Result<(), sqlx::Error> {
    sqlx::query!("DELETE FROM sessions WHERE id = $1", session_id)
        .execute(pool)
        .await?;
    Ok(())
}

/// Revoke every session of a user except `keep` (e.g. after a password change).
pub async fn delete_others(pool: &PgPool, user_id: Uuid, keep: Uuid) -> Result<u64, sqlx::Error> {
    let result = sqlx::query!(
        "DELETE FROM sessions WHERE user_id = $1 AND id <> $2",
        user_id,
        keep
    )
    .execute(pool)
    .await?;
    Ok(result.rows_affected())
}

/// Remove expired sessions. Returns how many were deleted.
pub async fn delete_expired(pool: &PgPool) -> Result<u64, sqlx::Error> {
    let result = sqlx::query!("DELETE FROM sessions WHERE expires_at <= now()")
        .execute(pool)
        .await?;
    Ok(result.rows_affected())
}

#[cfg(test)]
mod tests {
    use chrono::Duration;

    use super::*;
    use crate::{MIGRATOR, users};

    async fn user(pool: &PgPool) -> Uuid {
        match users::create(pool, "s@t.u", "h", None).await.expect("user") {
            users::Created::Ok(u) => u.id,
            users::Created::EmailTaken => panic!("fresh db"),
        }
    }

    #[sqlx::test(migrator = "MIGRATOR")]
    async fn expired_sessions_are_invisible_and_pruned(pool: PgPool) {
        let uid = user(&pool).await;
        let past = Utc::now() - Duration::minutes(1);
        let future = Utc::now() + Duration::days(1);
        create(&pool, uid, b"old", past, None).await.expect("old");
        let live = create(&pool, uid, b"new", future, Some("ua"))
            .await
            .expect("new");

        assert!(touch(&pool, b"old").await.expect("touch").is_none());
        let found = touch(&pool, b"new").await.expect("touch").expect("live");
        assert_eq!((found.session_id, found.user_id), (live, uid));

        assert_eq!(delete_expired(&pool).await.expect("prune"), 1);
    }

    #[sqlx::test(migrator = "MIGRATOR")]
    async fn delete_others_keeps_current(pool: PgPool) {
        let uid = user(&pool).await;
        let exp = Utc::now() + Duration::days(1);
        let keep = create(&pool, uid, b"a", exp, None).await.expect("a");
        create(&pool, uid, b"b", exp, None).await.expect("b");
        create(&pool, uid, b"c", exp, None).await.expect("c");

        assert_eq!(delete_others(&pool, uid, keep).await.expect("revoke"), 2);
        assert!(touch(&pool, b"a").await.expect("touch").is_some());
    }
}
