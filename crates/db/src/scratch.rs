//! Throwaway databases for `akasha eval`: created next to the configured one,
//! migrated, used once and dropped. Nothing touches the configured database
//! itself, so the eval can run against a live server's Postgres safely.

use std::str::FromStr;

use sqlx::{
    Connection, PgConnection, PgPool,
    postgres::{PgConnectOptions, PgPoolOptions},
};
use uuid::Uuid;

/// A database that exists until [`TempDatabase::drop_now`] (call it on every path).
pub struct TempDatabase {
    admin: PgConnectOptions,
    name: String,
    pub pool: PgPool,
}

impl TempDatabase {
    /// Create `akasha_scratch_<random>` on the server of `database_url` (needs the
    /// CREATEDB privilege) and run the migrations in it.
    pub async fn create(database_url: &str) -> Result<Self, sqlx::Error> {
        let admin = PgConnectOptions::from_str(database_url)?;
        let name = format!("akasha_scratch_{}", Uuid::new_v4().simple());
        let mut conn = PgConnection::connect_with(&admin).await?;
        // Identifiers cannot be bound; `name` is generated above (hex only).
        sqlx::query(&format!("CREATE DATABASE {name}"))
            .execute(&mut conn)
            .await?;
        conn.close().await?;
        let pool = PgPoolOptions::new()
            .max_connections(8)
            .connect_with(admin.clone().database(&name))
            .await?;
        let db = Self { admin, name, pool };
        if let Err(err) = crate::MIGRATOR.run(&db.pool).await {
            db.drop_now().await?;
            return Err(err.into());
        }
        Ok(db)
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    /// Close the pool and drop the database.
    pub async fn drop_now(self) -> Result<(), sqlx::Error> {
        self.pool.close().await;
        let mut conn = PgConnection::connect_with(&self.admin).await?;
        sqlx::query(&format!(
            "DROP DATABASE IF EXISTS {} WITH (FORCE)",
            self.name
        ))
        .execute(&mut conn)
        .await?;
        conn.close().await
    }
}

/// How far processing got for one owner's files.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LibraryStats {
    pub files: i64,
    pub ready: i64,
    pub chunks: i64,
    pub embedded: i64,
}

pub async fn library_stats(pool: &PgPool, owner_id: Uuid) -> Result<LibraryStats, sqlx::Error> {
    let row = sqlx::query!(
        r#"SELECT
               (SELECT count(*) FROM files WHERE owner_id = $1) AS "files!",
               (SELECT count(*) FROM files WHERE owner_id = $1 AND status = 'ready') AS "ready!",
               (SELECT count(*) FROM file_chunks WHERE owner_id = $1) AS "chunks!",
               (SELECT count(*) FROM file_chunks
                WHERE owner_id = $1 AND embedding IS NOT NULL) AS "embedded!""#,
        owner_id
    )
    .fetch_one(pool)
    .await?;
    Ok(LibraryStats {
        files: row.files,
        ready: row.ready,
        chunks: row.chunks,
        embedded: row.embedded,
    })
}
