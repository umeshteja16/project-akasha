//! Seeding helpers: users, files and chunks inserted directly, with vectors from
//! the deterministic hash embedder.
#![allow(dead_code)] // each test file uses a subset

use std::sync::Arc;

use akasha_ml::{Embedder, Reranker, catalog, fake};
use akasha_search::{ChunkFilter, Models, SearchMode, SearchRequest};
use sqlx::PgPool;
use uuid::Uuid;

pub fn embedder() -> Arc<dyn Embedder> {
    Arc::new(fake::HashEmbedder::new(
        catalog::embed_model(catalog::HASH_EMBED_MODEL).expect("model"),
    ))
}

/// Hash embedder, no reranker.
pub fn models() -> Models {
    Models {
        embedder: Ok(embedder()),
        reranker: Ok(None),
    }
}

/// Hash embedder and the word-overlap reranker.
pub fn models_with_rerank() -> Models {
    let reranker: Arc<dyn Reranker> = Arc::new(fake::OverlapReranker);
    Models {
        embedder: Ok(embedder()),
        reranker: Ok(Some(reranker)),
    }
}

pub fn request(query: &str, mode: SearchMode) -> SearchRequest {
    SearchRequest {
        query: query.into(),
        mode,
        filter: ChunkFilter::default(),
        limit: 10,
        offset: 0,
        rerank: false,
    }
}

pub async fn user(pool: &PgPool, email: &str) -> Uuid {
    sqlx::query_scalar("INSERT INTO users (email, password_hash) VALUES ($1, 'x') RETURNING id")
        .bind(email)
        .fetch_one(pool)
        .await
        .expect("user")
}

/// A ready file with one chunk per text, all embedded.
pub async fn file(pool: &PgPool, owner: Uuid, name: &str, mime: &str, chunks: &[&str]) -> Uuid {
    let id = file_without_vectors(pool, owner, name, mime, chunks).await;
    let vectors = embedder().embed_documents(chunks).expect("embed");
    for (i, v) in vectors.iter().enumerate() {
        sqlx::query(
            "UPDATE file_chunks SET embedding = $3::real[]::vector
             WHERE file_id = $1 AND chunk_index = $2",
        )
        .bind(id)
        .bind(i32::try_from(i).expect("index"))
        .bind(v)
        .execute(pool)
        .await
        .expect("embedding");
    }
    id
}

/// A file whose chunks have no vectors yet (embedding pending).
pub async fn file_without_vectors(
    pool: &PgPool,
    owner: Uuid,
    name: &str,
    mime: &str,
    chunks: &[&str],
) -> Uuid {
    let hash = format!("{:0>64}", format!("{:x}", Uuid::new_v4().as_u128()));
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO files (owner_id, original_name, content_hash, mime_type, size_bytes, status)
         VALUES ($1, $2, $3, $4, 100, 'ready') RETURNING id",
    )
    .bind(owner)
    .bind(name)
    .bind(hash)
    .bind(mime)
    .fetch_one(pool)
    .await
    .expect("file");
    let mut offset = 0i32;
    for (i, text) in chunks.iter().enumerate() {
        let len = i32::try_from(text.chars().count()).expect("len");
        sqlx::query(
            "INSERT INTO file_chunks (file_id, owner_id, chunk_index, page, char_start, char_end, text)
             VALUES ($1, $2, $3, $4, $5, $6, $7)",
        )
        .bind(id)
        .bind(owner)
        .bind(i32::try_from(i).expect("index"))
        .bind((mime == "application/pdf").then_some(i32::try_from(i + 1).expect("page")))
        .bind(offset)
        .bind(offset + len)
        .bind(*text)
        .execute(pool)
        .await
        .expect("chunk");
        offset += len + 1;
    }
    id
}
