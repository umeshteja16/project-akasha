//! Search requests and results. Results serialise as the HTTP response bodies.

use akasha_db::search::{ChunkFilter, FileRow};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

/// Which retrievers a search uses.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum SearchMode {
    /// Full-text search over chunk text and file names.
    Keyword,
    /// Embedding similarity only.
    Semantic,
    /// Both, fused with Reciprocal Rank Fusion.
    #[default]
    Hybrid,
}

/// A search over one owner's files.
#[derive(Debug, Clone)]
pub struct SearchRequest {
    /// Free text; quotes, `or` and `-word` work as in web search engines.
    pub query: String,
    pub mode: SearchMode,
    pub filter: ChunkFilter,
    /// Page size, 1..=[`crate::MAX_LIMIT`].
    pub limit: usize,
    /// Results to skip; `offset + limit` is at most [`crate::MAX_WINDOW`].
    pub offset: usize,
    /// Reorder the top results with the reranker (when one is configured).
    pub rerank: bool,
}

/// Milliseconds spent per stage (stages that did not run are 0).
#[derive(Debug, Clone, Default, Serialize, ToSchema)]
pub struct Timings {
    /// Embedding the query.
    pub embed_ms: f64,
    /// Keyword and file-name queries (and the spelling suggestion, when made).
    pub keyword_ms: f64,
    /// Vector query.
    pub semantic_ms: f64,
    /// Loading result rows and snippets.
    pub fetch_ms: f64,
    pub rerank_ms: f64,
    pub total_ms: f64,
}

/// How a search ran.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct SearchMeta {
    /// The query as searched (trimmed).
    pub query: String,
    pub requested_mode: SearchMode,
    /// The mode that actually ran: `keyword` when semantic search was unavailable.
    pub mode: SearchMode,
    /// `true` when the search ran in a weaker mode than requested.
    pub degraded: bool,
    /// `true` when the top results were reordered by the reranker.
    pub reranked: bool,
    /// Why the search was degraded or not reranked, for display or debugging.
    pub warnings: Vec<String>,
    /// More results follow this page.
    pub has_more: bool,
    /// "Did you mean": the query with misspelt-looking words replaced by similar
    /// words from your documents. Only when keyword search found few matches.
    pub suggestion: Option<String>,
    pub timings: Timings,
}

/// A highlighted span of a snippet, `[start, end)` in Unicode characters.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
pub struct Highlight {
    pub start: u32,
    pub end: u32,
}

/// An excerpt of a chunk (plain text, never HTML) with the query terms marked.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
pub struct Snippet {
    /// Up to two fragments joined by " … "; the chunk's start when no term matched.
    pub text: String,
    pub highlights: Vec<Highlight>,
}

/// Why a chunk ranked where it did.
#[derive(Debug, Clone, Default, PartialEq, Serialize, ToSchema)]
pub struct Scores {
    /// Reciprocal Rank Fusion score (sum of `1 / (60 + rank)` over the retrievers).
    pub fused: f64,
    /// 1-based rank in the keyword list.
    pub keyword_rank: Option<u32>,
    /// `ts_rank_cd`, normalised to 0..1.
    pub keyword: Option<f32>,
    /// 1-based rank in the semantic list.
    pub semantic_rank: Option<u32>,
    /// Cosine similarity to the query.
    pub semantic: Option<f32>,
    /// 1-based rank in the file-name list.
    pub filename_rank: Option<u32>,
    /// Cross-encoder score (model specific scale), when reranked.
    pub rerank: Option<f32>,
}

/// A matching chunk, located for citation.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct ChunkMatch {
    pub chunk_id: i64,
    pub chunk_index: i32,
    /// 1-based PDF page; `null` for formats without pages.
    pub page: Option<i32>,
    /// `[char_start, char_end)` of the chunk in the file's extracted text (characters).
    pub char_start: i32,
    pub char_end: i32,
    pub snippet: Snippet,
    pub scores: Scores,
}

/// The file a result belongs to.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct FileInfo {
    pub id: Uuid,
    pub name: String,
    pub mime_type: String,
    pub size_bytes: i64,
    /// `pending`, `processing`, `ready` or `failed`.
    pub status: String,
    /// The user's own tags.
    pub tags: Vec<String>,
    /// Tags suggested by the language model.
    pub auto_tags: Vec<String>,
    /// Model-written description of the file, when there is one.
    pub summary: Option<String>,
    pub is_pinned: bool,
    pub created_at: DateTime<Utc>,
}

impl From<FileRow> for FileInfo {
    fn from(f: FileRow) -> Self {
        Self {
            id: f.id,
            name: f.original_name,
            mime_type: f.mime_type,
            size_bytes: f.size_bytes,
            status: f.status,
            tags: f.tags,
            auto_tags: f.auto_tags,
            summary: f.summary,
            is_pinned: f.is_pinned,
            created_at: f.created_at,
        }
    }
}

/// A chunk-level result: the chunk, its full text and its file.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct ChunkHit {
    #[serde(flatten)]
    pub chunk: ChunkMatch,
    /// The whole chunk text.
    pub text: String,
    pub file: FileInfo,
}

/// A file-level result: the file and its best matching chunks.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct FileHit {
    pub file: FileInfo,
    /// The best chunk's fused score.
    pub score: f64,
    /// Matching chunks of this file among the candidates.
    pub match_count: u32,
    /// Up to three best chunks, best first.
    pub matches: Vec<ChunkMatch>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct ChunkResults {
    #[serde(flatten)]
    pub meta: SearchMeta,
    /// Best first.
    pub results: Vec<ChunkHit>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct FileResults {
    #[serde(flatten)]
    pub meta: SearchMeta,
    /// Best first.
    pub results: Vec<FileHit>,
}

/// A file similar to another one.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct SimilarFile {
    pub file: FileInfo,
    /// Cosine similarity of its closest chunk to the source file's mean embedding.
    pub similarity: f32,
}
