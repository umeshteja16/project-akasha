//! Postgres access for Akasha.
//!
//! Schema changes live only in `migrations/` (create one with
//! `sqlx migrate add -r <name> --source crates/db/migrations`). Never alter the
//! schema from application code.

pub mod extraction;
pub mod files;
pub mod sessions;
pub mod users;

use std::time::Duration;

pub use sqlx::PgPool;
use sqlx::{migrate::Migrator, postgres::PgPoolOptions};

/// All migrations, embedded into the binary at compile time.
pub static MIGRATOR: Migrator = sqlx::migrate!("./migrations");

/// Open a connection pool. Fails fast if the database is unreachable.
pub async fn connect(database_url: &str, max_connections: u32) -> Result<PgPool, sqlx::Error> {
    PgPoolOptions::new()
        .max_connections(max_connections)
        .acquire_timeout(Duration::from_secs(5))
        .connect(database_url)
        .await
}

/// Apply pending migrations.
pub async fn migrate(pool: &PgPool) -> Result<(), sqlx::migrate::MigrateError> {
    MIGRATOR.run(pool).await?;
    tracing::info!("database migrations applied");
    Ok(())
}

/// Cheap liveness check used by the readiness probe.
pub async fn ping(pool: &PgPool) -> Result<(), sqlx::Error> {
    sqlx::query("SELECT 1").execute(pool).await.map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[sqlx::test(migrator = "MIGRATOR")]
    async fn migrations_apply_and_ping_succeeds(pool: PgPool) {
        ping(&pool).await.expect("ping");
        let (version,): (i64,) =
            sqlx::query_as("SELECT max(version) FROM _sqlx_migrations WHERE success")
                .fetch_one(&pool)
                .await
                .expect("migration table");
        assert!(version >= 1);
    }

    #[sqlx::test(migrator = "MIGRATOR")]
    async fn pgvector_is_available(pool: PgPool) {
        sqlx::query("CREATE TABLE vec_probe (embedding vector(3) NOT NULL)")
            .execute(&pool)
            .await
            .expect("create vector column");
        sqlx::query("INSERT INTO vec_probe VALUES ('[1,2,3]'), ('[3,2,1]')")
            .execute(&pool)
            .await
            .expect("insert vectors");
        let (distance,): (f64,) = sqlx::query_as(
            "SELECT (embedding <-> '[1,2,3]')::float8 FROM vec_probe ORDER BY embedding <-> '[1,2,3]' LIMIT 1",
        )
        .fetch_one(&pool)
        .await
        .expect("nearest neighbour");
        assert_eq!(distance, 0.0);
    }
}
