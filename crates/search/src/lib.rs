//! Hybrid retrieval over one owner's chunks (ADR 0010).
//!
//! A search runs up to three retrievers over `file_chunks`, all restricted to the
//! owner in SQL ([`akasha_db::search`]):
//!
//! - **keyword**: Postgres full-text search (`websearch_to_tsquery`, `ts_rank_cd`),
//! - **file name**: the same query against file names (a file is represented by
//!   its first chunk),
//! - **semantic**: the query embedded with the configured [`Embedder`] and
//!   matched by cosine distance (pgvector HNSW).
//!
//! Their ranked lists are fused with Reciprocal Rank Fusion ([`fusion`]), the top
//! of the fused list is optionally reordered by a cross-encoder [`Reranker`], and
//! each result gets a highlighted snippet ([`snippet`]).
//!
//! Two views of the same ranking: [`search_chunks`] (chunk-level, for grounded
//! chat and agents) and [`search_files`] (grouped by file, for the search page).
//! When the embedder is unavailable, semantic and hybrid searches degrade to
//! keyword search and say so in [`SearchMeta`] instead of failing.

mod engine;
pub mod fusion;
mod group;
mod rerank;
mod similar;
pub mod snippet;
pub mod suggest;
mod types;

use std::sync::Arc;

use akasha_ml::{Embedder, Reranker};

pub use akasha_db::search::ChunkFilter;
pub use engine::{search_chunks, search_files};
pub use similar::{MAX_SIMILAR, similar_files};
pub use types::{
    ChunkHit, ChunkMatch, ChunkResults, FileHit, FileInfo, FileResults, Highlight, Scores,
    SearchMeta, SearchMode, SearchRequest, SimilarFile, Snippet, Timings,
};

/// Longest accepted query, in characters.
pub const MAX_QUERY_CHARS: usize = 500;
/// Largest page.
pub const MAX_LIMIT: usize = 50;
/// Results beyond this position are never returned (`offset + limit` cap).
pub const MAX_WINDOW: usize = 200;
/// Candidates each retriever contributes, at least.
pub const CANDIDATES: usize = 50;
/// RRF constant (Cormack et al.): dampens the weight of top ranks.
pub const RRF_K: f64 = 60.0;
/// Fused results the reranker reorders.
pub const RERANK_TOP: usize = 30;
/// HNSW candidate list size per query (pgvector default is 40).
pub const EF_SEARCH: i64 = 100;

/// The models a search may use. `Err` carries why a model is unavailable (shown
/// to the user as a warning); a disabled reranker is `Ok(None)`.
#[derive(Clone)]
pub struct Models {
    pub embedder: Result<Arc<dyn Embedder>, String>,
    pub reranker: Result<Option<Arc<dyn Reranker>>, String>,
}

#[derive(Debug, thiserror::Error)]
pub enum SearchError {
    /// The request is malformed (empty or long query, bad page); a 400.
    #[error("{0}")]
    InvalidRequest(String),
    #[error(transparent)]
    Database(#[from] sqlx::Error),
}
