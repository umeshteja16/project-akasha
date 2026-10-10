//! Listing a user's files: filters, four sort orders, keyset pagination.
//!
//! Every order ends in `id` so the order is total and a cursor (the sort key plus
//! the id of the last row of a page) picks up exactly where the page ended.
//! The four orders are separate checked queries (the macros need literal SQL).

use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

use super::File;

/// Sort order of [`list`].
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum ListOrder {
    /// `created_at` descending.
    #[default]
    Newest,
    /// `created_at` ascending.
    Oldest,
    /// `lower(original_name)` ascending (A to Z).
    Name,
    /// `size_bytes` descending.
    Largest,
    /// `last_opened_at` descending; only files that were ever opened.
    Opened,
}

/// The sort key of the last row seen, matching the order it came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ListKey {
    /// For [`ListOrder::Newest`] and [`ListOrder::Oldest`].
    Created(DateTime<Utc>),
    /// For [`ListOrder::Name`]: the file's name as stored (lowercased in SQL, so
    /// the comparison uses exactly the database's `lower()`).
    Name(String),
    /// For [`ListOrder::Largest`].
    Size(i64),
    /// For [`ListOrder::Opened`].
    Opened(DateTime<Utc>),
}

impl ListKey {
    /// The cursor key of `file` under `order`.
    pub fn of(file: &File, order: ListOrder) -> Self {
        match order {
            ListOrder::Newest | ListOrder::Oldest => Self::Created(file.created_at),
            ListOrder::Name => Self::Name(file.original_name.clone()),
            ListOrder::Largest => Self::Size(file.size_bytes),
            ListOrder::Opened => Self::Opened(file.last_opened_at.unwrap_or(file.created_at)),
        }
    }
}

/// Filters for [`list`]. `None` / empty means "any".
#[derive(Debug, Default, Clone)]
pub struct ListFilter {
    pub status: Option<String>,
    pub pinned: Option<bool>,
    /// Matches the user's tags and the model-suggested ones.
    pub tag: Option<String>,
    /// `LIKE` patterns on `mime_type`; a file matches if any pattern does.
    pub mime_patterns: Vec<String>,
    /// Only files in this collection (the caller checks that it is the owner's).
    pub collection_id: Option<Uuid>,
    pub order: ListOrder,
    /// Keyset cursor: only rows after `(key, id)` in `order`. A key of the wrong
    /// kind for the order is ignored (the caller validates cursors).
    pub after: Option<(ListKey, Uuid)>,
    pub limit: i64,
}

/// One page of the owner's files.
pub async fn list(
    pool: &PgPool,
    owner_id: Uuid,
    filter: &ListFilter,
) -> Result<Vec<File>, sqlx::Error> {
    let after_id = filter.after.as_ref().map(|(_, id)| *id);
    let key = filter.after.as_ref().map(|(key, _)| key);
    let after_ts = match key {
        Some(ListKey::Created(ts)) => Some(*ts),
        _ => None,
    };
    let after_name = match key {
        Some(ListKey::Name(name)) => Some(name.as_str()),
        _ => None,
    };
    let after_size = match key {
        Some(ListKey::Size(size)) => Some(*size),
        _ => None,
    };
    let after_opened = match key {
        Some(ListKey::Opened(ts)) => Some(*ts),
        _ => None,
    };
    let f = filter;
    match f.order {
        ListOrder::Newest => {
            sqlx::query_as!(
                File,
                r#"SELECT * FROM files
                   WHERE owner_id = $1
                     AND ($2::text IS NULL OR status = $2)
                     AND ($3::bool IS NULL OR is_pinned = $3)
                     AND ($4::text IS NULL OR tags @> ARRAY[$4::text] OR auto_tags @> ARRAY[$4::text])
                     AND (cardinality($5::text[]) = 0 OR mime_type LIKE ANY($5))
                     AND ($9::uuid IS NULL OR EXISTS (
                         SELECT 1 FROM collection_files cf
                         WHERE cf.owner_id = $1 AND cf.collection_id = $9 AND cf.file_id = files.id))
                     AND ($6::timestamptz IS NULL OR (created_at, id) < ($6, $7::uuid))
                   ORDER BY created_at DESC, id DESC
                   LIMIT $8"#,
                owner_id,
                f.status,
                f.pinned,
                f.tag,
                &f.mime_patterns,
                after_ts,
                after_id,
                f.limit,
                f.collection_id,
            )
            .fetch_all(pool)
            .await
        }
        ListOrder::Oldest => {
            sqlx::query_as!(
                File,
                r#"SELECT * FROM files
                   WHERE owner_id = $1
                     AND ($2::text IS NULL OR status = $2)
                     AND ($3::bool IS NULL OR is_pinned = $3)
                     AND ($4::text IS NULL OR tags @> ARRAY[$4::text] OR auto_tags @> ARRAY[$4::text])
                     AND (cardinality($5::text[]) = 0 OR mime_type LIKE ANY($5))
                     AND ($9::uuid IS NULL OR EXISTS (
                         SELECT 1 FROM collection_files cf
                         WHERE cf.owner_id = $1 AND cf.collection_id = $9 AND cf.file_id = files.id))
                     AND ($6::timestamptz IS NULL OR (created_at, id) > ($6, $7::uuid))
                   ORDER BY created_at ASC, id ASC
                   LIMIT $8"#,
                owner_id,
                f.status,
                f.pinned,
                f.tag,
                &f.mime_patterns,
                after_ts,
                after_id,
                f.limit,
                f.collection_id,
            )
            .fetch_all(pool)
            .await
        }
        ListOrder::Name => {
            sqlx::query_as!(
                File,
                r#"SELECT * FROM files
                   WHERE owner_id = $1
                     AND ($2::text IS NULL OR status = $2)
                     AND ($3::bool IS NULL OR is_pinned = $3)
                     AND ($4::text IS NULL OR tags @> ARRAY[$4::text] OR auto_tags @> ARRAY[$4::text])
                     AND (cardinality($5::text[]) = 0 OR mime_type LIKE ANY($5))
                     AND ($9::uuid IS NULL OR EXISTS (
                         SELECT 1 FROM collection_files cf
                         WHERE cf.owner_id = $1 AND cf.collection_id = $9 AND cf.file_id = files.id))
                     AND ($6::text IS NULL OR (lower(original_name), id) > (lower($6), $7::uuid))
                   ORDER BY lower(original_name) ASC, id ASC
                   LIMIT $8"#,
                owner_id,
                f.status,
                f.pinned,
                f.tag,
                &f.mime_patterns,
                after_name,
                after_id,
                f.limit,
                f.collection_id,
            )
            .fetch_all(pool)
            .await
        }
        ListOrder::Largest => {
            sqlx::query_as!(
                File,
                r#"SELECT * FROM files
                   WHERE owner_id = $1
                     AND ($2::text IS NULL OR status = $2)
                     AND ($3::bool IS NULL OR is_pinned = $3)
                     AND ($4::text IS NULL OR tags @> ARRAY[$4::text] OR auto_tags @> ARRAY[$4::text])
                     AND (cardinality($5::text[]) = 0 OR mime_type LIKE ANY($5))
                     AND ($9::uuid IS NULL OR EXISTS (
                         SELECT 1 FROM collection_files cf
                         WHERE cf.owner_id = $1 AND cf.collection_id = $9 AND cf.file_id = files.id))
                     AND ($6::int8 IS NULL OR (size_bytes, id) < ($6, $7::uuid))
                   ORDER BY size_bytes DESC, id DESC
                   LIMIT $8"#,
                owner_id,
                f.status,
                f.pinned,
                f.tag,
                &f.mime_patterns,
                after_size,
                after_id,
                f.limit,
                f.collection_id,
            )
            .fetch_all(pool)
            .await
        }
        ListOrder::Opened => {
            sqlx::query_as!(
                File,
                r#"SELECT * FROM files
                   WHERE owner_id = $1 AND last_opened_at IS NOT NULL
                     AND ($2::text IS NULL OR status = $2)
                     AND ($3::bool IS NULL OR is_pinned = $3)
                     AND ($4::text IS NULL OR tags @> ARRAY[$4::text] OR auto_tags @> ARRAY[$4::text])
                     AND (cardinality($5::text[]) = 0 OR mime_type LIKE ANY($5))
                     AND ($9::uuid IS NULL OR EXISTS (
                         SELECT 1 FROM collection_files cf
                         WHERE cf.owner_id = $1 AND cf.collection_id = $9 AND cf.file_id = files.id))
                     AND ($6::timestamptz IS NULL OR (last_opened_at, id) < ($6, $7::uuid))
                   ORDER BY last_opened_at DESC, id DESC
                   LIMIT $8"#,
                owner_id,
                f.status,
                f.pinned,
                f.tag,
                &f.mime_patterns,
                after_opened,
                after_id,
                f.limit,
                f.collection_id,
            )
            .fetch_all(pool)
            .await
        }
    }
}

/// A tag and how many of the owner's files carry it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TagCount {
    pub tag: String,
    /// Files with it as one of the user's own tags.
    pub user_files: i64,
    /// Files with it only as a model-suggested tag.
    pub auto_files: i64,
}

/// Every tag the owner's files carry (own and suggested), most used first.
pub async fn tag_counts(pool: &PgPool, owner_id: Uuid) -> Result<Vec<TagCount>, sqlx::Error> {
    let rows = sqlx::query!(
        r#"SELECT t.tag AS "tag!",
                  count(*) FILTER (WHERE t.own) AS "user_files!",
                  count(*) FILTER (WHERE NOT t.own) AS "auto_files!"
           FROM (
               SELECT DISTINCT f.id, u.tag, u.own
               FROM files f,
                    LATERAL (
                        SELECT tag, true AS own FROM unnest(f.tags) AS tag
                        UNION ALL
                        SELECT tag, false FROM unnest(f.auto_tags) AS tag
                        WHERE NOT (tag = ANY(f.tags))
                    ) u
               WHERE f.owner_id = $1
           ) t
           GROUP BY t.tag
           ORDER BY count(*) DESC, t.tag ASC"#,
        owner_id,
    )
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| TagCount {
            tag: r.tag,
            user_files: r.user_files,
            auto_files: r.auto_files,
        })
        .collect())
}
