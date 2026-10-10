//! Personal API tokens. Callers pass the SHA-256 of the token, never the token.

use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

/// A token as listed to its owner (never the secret).
#[derive(Debug, Clone)]
pub struct ApiToken {
    pub id: Uuid,
    pub name: String,
    pub prefix: String,
    pub scopes: Vec<String>,
    pub created_at: DateTime<Utc>,
    pub last_used_at: Option<DateTime<Utc>>,
    pub expires_at: Option<DateTime<Utc>>,
    pub revoked_at: Option<DateTime<Utc>>,
}

/// A usable token: whose it is and what it may do.
#[derive(Debug, Clone)]
pub struct TokenGrant {
    pub token_id: Uuid,
    pub owner_id: Uuid,
    pub scopes: Vec<String>,
}

pub struct NewToken<'a> {
    pub owner_id: Uuid,
    pub name: &'a str,
    pub token_hash: &'a [u8],
    pub prefix: &'a str,
    pub scopes: &'a [String],
    pub expires_at: Option<DateTime<Utc>>,
}

pub async fn create<'e>(
    db: impl sqlx::PgExecutor<'e>,
    new: &NewToken<'_>,
) -> Result<ApiToken, sqlx::Error> {
    sqlx::query_as!(
        ApiToken,
        r#"INSERT INTO api_tokens (owner_id, name, token_hash, prefix, scopes, expires_at)
           VALUES ($1, $2, $3, $4, $5, $6)
           RETURNING id, name, prefix, scopes, created_at, last_used_at, expires_at,
                     revoked_at"#,
        new.owner_id,
        new.name,
        new.token_hash,
        new.prefix,
        new.scopes,
        new.expires_at,
    )
    .fetch_one(db)
    .await
}

/// The owner's tokens, newest first.
pub async fn list(pool: &PgPool, owner_id: Uuid) -> Result<Vec<ApiToken>, sqlx::Error> {
    sqlx::query_as!(
        ApiToken,
        r#"SELECT id, name, prefix, scopes, created_at, last_used_at, expires_at, revoked_at
           FROM api_tokens WHERE owner_id = $1
           ORDER BY created_at DESC, id DESC"#,
        owner_id,
    )
    .fetch_all(pool)
    .await
}

/// Tokens that still work (not revoked, not expired).
pub async fn count_active(pool: &PgPool, owner_id: Uuid) -> Result<i64, sqlx::Error> {
    let n = sqlx::query_scalar!(
        r#"SELECT count(*) AS "n!" FROM api_tokens
           WHERE owner_id = $1 AND revoked_at IS NULL
             AND (expires_at IS NULL OR expires_at > now())"#,
        owner_id,
    )
    .fetch_one(pool)
    .await?;
    Ok(n)
}

/// Revoke one of the owner's tokens (idempotent). `None`: no such token;
/// otherwise its name and whether this call revoked it (`false`: it already was).
pub async fn revoke<'e>(
    db: impl sqlx::PgExecutor<'e>,
    owner_id: Uuid,
    id: Uuid,
) -> Result<Option<(String, bool)>, sqlx::Error> {
    let row = sqlx::query!(
        r#"UPDATE api_tokens t SET revoked_at = coalesce(t.revoked_at, now())
           FROM (SELECT id, revoked_at FROM api_tokens
                 WHERE owner_id = $1 AND id = $2 FOR UPDATE) old
           WHERE t.id = old.id
           RETURNING t.name, old.revoked_at IS NULL AS "newly!""#,
        owner_id,
        id,
    )
    .fetch_optional(db)
    .await?;
    Ok(row.map(|r| (r.name, r.newly)))
}

/// Look up a usable token by hash. `last_used_at` is refreshed at most once a
/// minute, so busy clients do not write on every request.
pub async fn authenticate(
    pool: &PgPool,
    token_hash: &[u8],
) -> Result<Option<TokenGrant>, sqlx::Error> {
    sqlx::query_as!(
        TokenGrant,
        r#"WITH t AS (
               SELECT id, owner_id, scopes, last_used_at FROM api_tokens
               WHERE token_hash = $1 AND revoked_at IS NULL
                 AND (expires_at IS NULL OR expires_at > now())
           ), touched AS (
               UPDATE api_tokens SET last_used_at = now()
               WHERE id IN (SELECT id FROM t
                            WHERE last_used_at IS NULL
                               OR last_used_at < now() - interval '1 minute')
           )
           SELECT id AS "token_id!", owner_id AS "owner_id!", scopes AS "scopes!" FROM t"#,
        token_hash,
    )
    .fetch_optional(pool)
    .await
}

#[cfg(test)]
mod tests {
    use chrono::Duration;

    use super::*;
    use crate::{MIGRATOR, users};

    async fn user(pool: &PgPool, email: &str) -> Uuid {
        match users::create(pool, email, "h", None).await.expect("user") {
            users::Created::Ok(u) => u.id,
            users::Created::EmailTaken => panic!("fresh db"),
        }
    }

    fn new<'a>(owner: Uuid, hash: &'a [u8], scopes: &'a [String]) -> NewToken<'a> {
        NewToken {
            owner_id: owner,
            name: "laptop",
            token_hash: hash,
            prefix: "akasha_pat_abcd",
            scopes,
            expires_at: None,
        }
    }

    #[sqlx::test(migrator = "MIGRATOR")]
    async fn tokens_authenticate_until_revoked_or_expired(pool: PgPool) {
        let a = user(&pool, "a@t.u").await;
        let b = user(&pool, "b@t.u").await;
        let read = vec!["read".to_owned()];
        let t = create(&pool, &new(a, b"one", &read)).await.expect("create");
        let grant = authenticate(&pool, b"one")
            .await
            .expect("auth")
            .expect("ok");
        assert_eq!((grant.token_id, grant.owner_id), (t.id, a));
        assert_eq!(grant.scopes, read);
        assert!(
            list(&pool, a).await.expect("list")[0]
                .last_used_at
                .is_some()
        );

        // Someone else cannot revoke it.
        assert!(revoke(&pool, b, t.id).await.expect("revoke").is_none());
        assert_eq!(
            revoke(&pool, a, t.id).await.expect("revoke"),
            Some(("laptop".to_owned(), true))
        );
        assert_eq!(
            revoke(&pool, a, t.id).await.expect("again"),
            Some(("laptop".to_owned(), false))
        );
        assert!(authenticate(&pool, b"one").await.expect("auth").is_none());
        assert_eq!(count_active(&pool, a).await.expect("count"), 0);

        let mut expired = new(a, b"two", &read);
        expired.expires_at = Some(Utc::now() - Duration::minutes(1));
        create(&pool, &expired).await.expect("create");
        assert!(authenticate(&pool, b"two").await.expect("auth").is_none());
    }

    #[sqlx::test(migrator = "MIGRATOR")]
    async fn scopes_are_constrained(pool: PgPool) {
        let a = user(&pool, "a@t.u").await;
        let bad = vec!["admin".to_owned()];
        assert!(create(&pool, &new(a, b"x", &bad)).await.is_err());
        let none: Vec<String> = Vec::new();
        assert!(create(&pool, &new(a, b"y", &none)).await.is_err());
    }
}
