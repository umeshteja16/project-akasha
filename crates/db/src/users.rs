//! User accounts.

use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct User {
    pub id: Uuid,
    pub email: String,
    pub password_hash: String,
    pub display_name: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Outcome of [`create`]: the email may already be registered.
#[derive(Debug)]
pub enum Created {
    Ok(User),
    EmailTaken,
}

pub async fn create(
    pool: &PgPool,
    email: &str,
    password_hash: &str,
    display_name: Option<&str>,
) -> Result<Created, sqlx::Error> {
    let user = sqlx::query_as!(
        User,
        r#"INSERT INTO users (email, password_hash, display_name)
           VALUES ($1, $2, $3)
           ON CONFLICT (email) DO NOTHING
           RETURNING id, email AS "email: String", password_hash, display_name,
                     created_at, updated_at"#,
        email as &str,
        password_hash,
        display_name,
    )
    .fetch_optional(pool)
    .await?;
    Ok(user.map_or(Created::EmailTaken, Created::Ok))
}

pub async fn find_by_email(pool: &PgPool, email: &str) -> Result<Option<User>, sqlx::Error> {
    sqlx::query_as!(
        User,
        r#"SELECT id, email AS "email: String", password_hash, display_name,
                  created_at, updated_at
           FROM users WHERE email = $1::text::citext"#,
        email,
    )
    .fetch_optional(pool)
    .await
}

pub async fn find_by_id(pool: &PgPool, id: Uuid) -> Result<Option<User>, sqlx::Error> {
    sqlx::query_as!(
        User,
        r#"SELECT id, email AS "email: String", password_hash, display_name,
                  created_at, updated_at
           FROM users WHERE id = $1"#,
        id,
    )
    .fetch_optional(pool)
    .await
}

pub async fn update_display_name(
    pool: &PgPool,
    id: Uuid,
    display_name: Option<&str>,
) -> Result<Option<User>, sqlx::Error> {
    sqlx::query_as!(
        User,
        r#"UPDATE users SET display_name = $2 WHERE id = $1
           RETURNING id, email AS "email: String", password_hash, display_name,
                     created_at, updated_at"#,
        id,
        display_name,
    )
    .fetch_optional(pool)
    .await
}

pub async fn update_password_hash(
    pool: &PgPool,
    id: Uuid,
    password_hash: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "UPDATE users SET password_hash = $2 WHERE id = $1",
        id,
        password_hash
    )
    .execute(pool)
    .await?;
    Ok(())
}

/// Deletes the user; sessions go with it (`ON DELETE CASCADE`).
pub async fn delete<'e>(db: impl sqlx::PgExecutor<'e>, id: Uuid) -> Result<(), sqlx::Error> {
    sqlx::query!("DELETE FROM users WHERE id = $1", id)
        .execute(db)
        .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::MIGRATOR;

    #[sqlx::test(migrator = "MIGRATOR")]
    async fn email_is_unique_case_insensitively(pool: PgPool) {
        let first = create(&pool, "Ada@Example.com", "h", None)
            .await
            .expect("create");
        assert!(matches!(first, Created::Ok(_)));

        let again = create(&pool, "ada@example.COM", "h", None)
            .await
            .expect("create");
        assert!(matches!(again, Created::EmailTaken));

        let found = find_by_email(&pool, "ADA@example.com").await.expect("find");
        assert_eq!(found.map(|u| u.email).as_deref(), Some("Ada@Example.com"));
    }

    #[sqlx::test(migrator = "MIGRATOR")]
    async fn update_and_delete(pool: PgPool) {
        let Created::Ok(user) = create(&pool, "a@b.c", "h", None).await.expect("create") else {
            panic!("expected new user");
        };
        let updated = update_display_name(&pool, user.id, Some("Ada"))
            .await
            .expect("update")
            .expect("exists");
        assert_eq!(updated.display_name.as_deref(), Some("Ada"));
        assert!(updated.updated_at >= user.updated_at);

        delete(&pool, user.id).await.expect("delete");
        assert!(find_by_id(&pool, user.id).await.expect("find").is_none());
    }
}
