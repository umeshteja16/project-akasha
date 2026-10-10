//! Collections: named, owner-scoped groups of files (many-to-many).
//!
//! Every query takes the owner's id: another user's collection is simply not
//! found. `collection_files` references both sides by `(id, owner_id)`, so the
//! schema itself refuses to put someone else's file into a collection.

use chrono::{DateTime, Utc};
use sqlx::{PgConnection, PgPool};
use uuid::Uuid;

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct Collection {
    pub id: Uuid,
    pub owner_id: Uuid,
    pub name: String,
    pub description: String,
    pub color: String,
    pub icon: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub file_count: i64,
}

/// A collection as referenced from a file (id, name and look).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CollectionRef {
    pub id: Uuid,
    pub name: String,
    pub color: String,
    pub icon: String,
}

/// A file added to or removed from a collection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileName {
    pub id: Uuid,
    pub name: String,
}

pub struct NewCollection<'a> {
    pub owner_id: Uuid,
    pub name: &'a str,
    pub description: &'a str,
    pub color: &'a str,
    pub icon: &'a str,
}

/// What [`update`] changes; `None` leaves a field as it is.
#[derive(Debug, Default, Clone, Copy)]
pub struct CollectionChanges<'a> {
    pub name: Option<&'a str>,
    pub description: Option<&'a str>,
    pub color: Option<&'a str>,
    pub icon: Option<&'a str>,
}

/// Result of [`create`] and [`update`].
#[derive(Debug)]
pub enum Saved {
    Ok(Collection),
    /// The owner already has a collection with this name (any case).
    NameTaken,
    /// [`update`] only: no such collection.
    NotFound,
}

fn is_name_conflict(err: &sqlx::Error) -> bool {
    matches!(err, sqlx::Error::Database(db)
        if db.constraint() == Some("collections_owner_name_key"))
}

pub async fn create(
    conn: &mut PgConnection,
    new: &NewCollection<'_>,
) -> Result<Saved, sqlx::Error> {
    let row = sqlx::query_as!(
        Collection,
        r#"INSERT INTO collections (owner_id, name, description, color, icon)
           VALUES ($1, $2, $3, $4, $5)
           RETURNING id, owner_id, name, description, color, icon, created_at, updated_at,
                     0::int8 AS "file_count!""#,
        new.owner_id,
        new.name,
        new.description,
        new.color,
        new.icon,
    )
    .fetch_one(conn)
    .await;
    match row {
        Ok(c) => Ok(Saved::Ok(c)),
        Err(e) if is_name_conflict(&e) => Ok(Saved::NameTaken),
        Err(e) => Err(e),
    }
}

/// The owner's collections, by name.
pub async fn list(pool: &PgPool, owner_id: Uuid) -> Result<Vec<Collection>, sqlx::Error> {
    sqlx::query_as!(
        Collection,
        r#"SELECT c.id, c.owner_id, c.name, c.description, c.color, c.icon, c.created_at,
                  c.updated_at,
                  (SELECT count(*) FROM collection_files cf WHERE cf.collection_id = c.id)
                      AS "file_count!"
           FROM collections c WHERE c.owner_id = $1
           ORDER BY lower(c.name), c.id"#,
        owner_id,
    )
    .fetch_all(pool)
    .await
}

pub async fn count(pool: &PgPool, owner_id: Uuid) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar!(
        r#"SELECT count(*) AS "n!" FROM collections WHERE owner_id = $1"#,
        owner_id
    )
    .fetch_one(pool)
    .await
}

pub async fn get<'e>(
    db: impl sqlx::PgExecutor<'e>,
    owner_id: Uuid,
    id: Uuid,
) -> Result<Option<Collection>, sqlx::Error> {
    sqlx::query_as!(
        Collection,
        r#"SELECT c.id, c.owner_id, c.name, c.description, c.color, c.icon, c.created_at,
                  c.updated_at,
                  (SELECT count(*) FROM collection_files cf WHERE cf.collection_id = c.id)
                      AS "file_count!"
           FROM collections c WHERE c.owner_id = $1 AND c.id = $2"#,
        owner_id,
        id,
    )
    .fetch_optional(db)
    .await
}

/// One of the owner's collections by exact name (any case), for agents.
pub async fn find_by_name(
    pool: &PgPool,
    owner_id: Uuid,
    name: &str,
) -> Result<Option<Collection>, sqlx::Error> {
    sqlx::query_as!(
        Collection,
        r#"SELECT c.id, c.owner_id, c.name, c.description, c.color, c.icon, c.created_at,
                  c.updated_at,
                  (SELECT count(*) FROM collection_files cf WHERE cf.collection_id = c.id)
                      AS "file_count!"
           FROM collections c WHERE c.owner_id = $1 AND lower(c.name) = lower($2)"#,
        owner_id,
        name,
    )
    .fetch_optional(pool)
    .await
}

/// Does the owner have this collection?
pub async fn exists(pool: &PgPool, owner_id: Uuid, id: Uuid) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar!(
        r#"SELECT EXISTS (SELECT 1 FROM collections WHERE owner_id = $1 AND id = $2) AS "e!""#,
        owner_id,
        id
    )
    .fetch_one(pool)
    .await
}

pub async fn update(
    conn: &mut PgConnection,
    owner_id: Uuid,
    id: Uuid,
    changes: CollectionChanges<'_>,
) -> Result<Saved, sqlx::Error> {
    let row = sqlx::query_as!(
        Collection,
        r#"UPDATE collections c SET
               name = COALESCE($3, name),
               description = COALESCE($4, description),
               color = COALESCE($5, color),
               icon = COALESCE($6, icon)
           WHERE c.owner_id = $1 AND c.id = $2
           RETURNING c.id, c.owner_id, c.name, c.description, c.color, c.icon, c.created_at,
                     c.updated_at,
                     (SELECT count(*) FROM collection_files cf WHERE cf.collection_id = c.id)
                         AS "file_count!""#,
        owner_id,
        id,
        changes.name,
        changes.description,
        changes.color,
        changes.icon,
    )
    .fetch_optional(conn)
    .await;
    match row {
        Ok(Some(c)) => Ok(Saved::Ok(c)),
        Ok(None) => Ok(Saved::NotFound),
        Err(e) if is_name_conflict(&e) => Ok(Saved::NameTaken),
        Err(e) => Err(e),
    }
}

/// Delete a collection (its files stay). Returns its name if it existed.
pub async fn delete(
    conn: &mut PgConnection,
    owner_id: Uuid,
    id: Uuid,
) -> Result<Option<String>, sqlx::Error> {
    sqlx::query_scalar!(
        "DELETE FROM collections WHERE owner_id = $1 AND id = $2 RETURNING name",
        owner_id,
        id
    )
    .fetch_optional(conn)
    .await
}

/// Add the owner's files to one of their collections. Ids that are not the
/// owner's files, and files already in it, are skipped. Returns the files added.
pub async fn add_files(
    conn: &mut PgConnection,
    owner_id: Uuid,
    id: Uuid,
    file_ids: &[Uuid],
) -> Result<Vec<FileName>, sqlx::Error> {
    let added = sqlx::query_as!(
        FileName,
        r#"WITH ins AS (
               INSERT INTO collection_files (collection_id, file_id, owner_id)
               SELECT c.id, f.id, $1
               FROM collections c
               JOIN files f ON f.owner_id = $1 AND f.id = ANY($3::uuid[])
               WHERE c.owner_id = $1 AND c.id = $2
               ON CONFLICT DO NOTHING
               RETURNING file_id
           )
           SELECT f.id, f.original_name AS name
           FROM ins JOIN files f ON f.id = ins.file_id
           ORDER BY array_position($3::uuid[], f.id)"#,
        owner_id,
        id,
        file_ids,
    )
    .fetch_all(&mut *conn)
    .await?;
    if !added.is_empty() {
        touch(conn, owner_id, id).await?;
    }
    Ok(added)
}

/// Take files out of a collection (the files stay). Returns the files removed.
pub async fn remove_files(
    conn: &mut PgConnection,
    owner_id: Uuid,
    id: Uuid,
    file_ids: &[Uuid],
) -> Result<Vec<FileName>, sqlx::Error> {
    let removed = sqlx::query_as!(
        FileName,
        r#"WITH del AS (
               DELETE FROM collection_files
               WHERE owner_id = $1 AND collection_id = $2 AND file_id = ANY($3::uuid[])
               RETURNING file_id
           )
           SELECT f.id, f.original_name AS name
           FROM del JOIN files f ON f.id = del.file_id AND f.owner_id = $1
           ORDER BY array_position($3::uuid[], f.id)"#,
        owner_id,
        id,
        file_ids,
    )
    .fetch_all(&mut *conn)
    .await?;
    if !removed.is_empty() {
        touch(conn, owner_id, id).await?;
    }
    Ok(removed)
}

async fn touch(conn: &mut PgConnection, owner_id: Uuid, id: Uuid) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "UPDATE collections SET updated_at = now() WHERE owner_id = $1 AND id = $2",
        owner_id,
        id
    )
    .execute(conn)
    .await?;
    Ok(())
}

/// The collections a file is in, by name.
pub async fn of_file(
    pool: &PgPool,
    owner_id: Uuid,
    file_id: Uuid,
) -> Result<Vec<CollectionRef>, sqlx::Error> {
    sqlx::query_as!(
        CollectionRef,
        r#"SELECT c.id, c.name, c.color, c.icon
           FROM collection_files cf
           JOIN collections c ON c.id = cf.collection_id AND c.owner_id = $1
           WHERE cf.owner_id = $1 AND cf.file_id = $2
           ORDER BY lower(c.name), c.id"#,
        owner_id,
        file_id,
    )
    .fetch_all(pool)
    .await
}

#[cfg(test)]
mod tests;
