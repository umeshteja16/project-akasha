//! The search pipeline: retrieve → fuse → load → rerank → page.

use std::{collections::HashMap, sync::Arc, time::Instant};

use akasha_db::{
    PgPool,
    search::{self, Candidate, ChunkRow},
};
use akasha_ml::Embedder;
use uuid::Uuid;

use crate::{
    CANDIDATES, EF_SEARCH, MAX_LIMIT, MAX_QUERY_CHARS, MAX_WINDOW, Models, RRF_K, SearchError,
    fusion::{self, Source},
    group, relevance,
    rerank::rerank,
    snippet, suggest,
    types::{
        ChunkHit, ChunkMatch, ChunkResults, FileInfo, FileResults, Scores, SearchMeta, SearchMode,
        SearchRequest, Timings,
    },
};

/// Most candidates per retriever (deep pages of file-grouped results).
const MAX_POOL: usize = 300;

/// A loaded chunk with its scores, in ranked order.
pub(crate) struct Ranked {
    pub row: ChunkRow,
    pub scores: Scores,
    /// Below the relevance floor ([`crate::relevance`]).
    pub weak: bool,
}

impl Ranked {
    pub fn to_match(&self) -> ChunkMatch {
        ChunkMatch {
            chunk_id: self.row.id,
            chunk_index: self.row.chunk_index,
            page: self.row.page,
            start_ms: self.row.start_ms,
            end_ms: self.row.end_ms,
            char_start: self.row.char_start,
            char_end: self.row.char_end,
            snippet: snippet::parse(&self.row.headline),
            scores: self.scores.clone(),
            loosely_related: self.weak,
        }
    }

    pub fn file(&self) -> FileInfo {
        let r = &self.row;
        FileInfo {
            id: r.file_id,
            name: r.file_name.clone(),
            mime_type: r.mime_type.clone(),
            size_bytes: r.size_bytes,
            status: r.status.clone(),
            tags: r.tags.clone(),
            auto_tags: r.auto_tags.clone(),
            summary: r.summary.clone(),
            is_pinned: r.is_pinned,
            created_at: r.created_at,
        }
    }
}

/// Chunk-level results (grounded chat, agents): one entry per chunk.
pub async fn search_chunks(
    pool: &PgPool,
    owner_id: Uuid,
    req: &SearchRequest,
    models: &Models,
) -> Result<ChunkResults, SearchError> {
    let started = Instant::now();
    let window = validate(req)?;
    let (mut meta, mut ranked) = retrieve(pool, owner_id, req, models, window).await?;
    if !req.include_weak {
        let before = ranked.len();
        ranked.retain(|r| !r.weak);
        meta.loosely_related = count(before - ranked.len());
    }
    meta.has_more = ranked.len() > window && window < MAX_WINDOW;
    let results = ranked
        .into_iter()
        .skip(req.offset)
        .take(req.limit)
        .map(|r| ChunkHit {
            chunk: r.to_match(),
            file: r.file(),
            text: r.row.text,
        })
        .collect();
    meta.timings.total_ms = ms(started);
    Ok(ChunkResults { meta, results })
}

/// File-level results (the search page): chunks grouped by file, ranked by
/// each file's best chunk.
pub async fn search_files(
    pool: &PgPool,
    owner_id: Uuid,
    req: &SearchRequest,
    models: &Models,
) -> Result<FileResults, SearchError> {
    let started = Instant::now();
    let window = validate(req)?;
    // Files usually match with several chunks: fetch more candidates.
    let (mut meta, ranked) = retrieve(pool, owner_id, req, models, window * 3).await?;
    let mut files = group::by_file(&ranked);
    if !req.include_weak {
        let before = files.len();
        files.retain(|f| !f.loosely_related);
        meta.loosely_related = count(before - files.len());
    }
    meta.has_more = files.len() > window && window < MAX_WINDOW;
    let results = files.into_iter().skip(req.offset).take(req.limit).collect();
    meta.timings.total_ms = ms(started);
    Ok(FileResults { meta, results })
}

/// Checks the request; returns `offset + limit`.
fn validate(req: &SearchRequest) -> Result<usize, SearchError> {
    let invalid = |m: String| Err(SearchError::InvalidRequest(m));
    let chars = req.query.trim().chars().count();
    if chars == 0 {
        return invalid("query must not be empty".into());
    }
    if chars > MAX_QUERY_CHARS {
        return invalid(format!(
            "query must be at most {MAX_QUERY_CHARS} characters"
        ));
    }
    if !(1..=MAX_LIMIT).contains(&req.limit) {
        return invalid(format!("limit must be 1-{MAX_LIMIT}"));
    }
    let window = req.offset.saturating_add(req.limit);
    if window > MAX_WINDOW {
        return invalid(format!(
            "only the first {MAX_WINDOW} results can be paged through"
        ));
    }
    Ok(window)
}

enum Semantic {
    Skipped,
    Done(Vec<Candidate>),
    /// The query could not be embedded (reason).
    Failed(String),
}

/// Runs the retrievers, fuses, loads and reranks. Returns everything ranked.
async fn retrieve(
    pool: &PgPool,
    owner_id: Uuid,
    req: &SearchRequest,
    models: &Models,
    window: usize,
) -> Result<(SearchMeta, Vec<Ranked>), SearchError> {
    let query = req.query.trim();
    let limit = i64::try_from(window.clamp(CANDIDATES, MAX_POOL)).unwrap_or(50);
    let mut meta = SearchMeta {
        query: query.to_owned(),
        requested_mode: req.mode,
        mode: req.mode,
        degraded: false,
        reranked: false,
        warnings: Vec::new(),
        has_more: false,
        suggestion: None,
        loosely_related: 0,
        timings: Timings::default(),
    };

    let embedder = match (req.mode, &models.embedder) {
        (SearchMode::Keyword, _) => None,
        (_, Ok(e)) => Some(Arc::clone(e)),
        (_, Err(reason)) => {
            degrade(&mut meta, reason);
            None
        }
    };
    let filter = &req.filter;
    let run_keyword = meta.mode != SearchMode::Semantic;
    let mut timings = Timings::default();
    let keyword = async {
        if !run_keyword {
            return Ok((None, 0.0));
        }
        let t = Instant::now();
        let lists = keyword_lists(pool, owner_id, query, filter, limit).await?;
        Ok::<_, sqlx::Error>((Some(lists), ms(t)))
    };
    let semantic = async {
        let Some(embedder) = embedder else {
            return Ok((Semantic::Skipped, 0.0, 0.0));
        };
        let t = Instant::now();
        let vector = match embed(embedder, query).await {
            Ok(v) => v,
            Err(reason) => return Ok((Semantic::Failed(reason), ms(t), 0.0)),
        };
        let embed_ms = ms(t);
        let t = Instant::now();
        let list = search::semantic(pool, owner_id, &vector, filter, limit, EF_SEARCH).await?;
        Ok::<_, sqlx::Error>((Semantic::Done(list), embed_ms, ms(t)))
    };
    let (keyword, semantic) = tokio::join!(keyword, semantic);
    let (mut keyword, keyword_ms) = keyword?;
    let (semantic, embed_ms, semantic_ms) = semantic?;
    timings.keyword_ms = keyword_ms;
    timings.embed_ms = embed_ms;
    timings.semantic_ms = semantic_ms;

    let semantic = match semantic {
        Semantic::Done(list) => list,
        Semantic::Skipped => Vec::new(),
        Semantic::Failed(reason) => {
            degrade(&mut meta, &reason);
            if keyword.is_none() {
                let t = Instant::now();
                keyword = Some(keyword_lists(pool, owner_id, query, filter, limit).await?);
                timings.keyword_ms = ms(t);
            }
            Vec::new()
        }
    };
    let ran_keyword = keyword.is_some();
    let (keyword, names) = keyword.unwrap_or_default();
    if ran_keyword && keyword.len() + names.len() < suggest::SUGGEST_BELOW {
        let t = Instant::now();
        meta.suggestion = match suggest::suggest(pool, owner_id, query).await {
            Ok(s) => s,
            Err(err) => {
                tracing::warn!(%err, "spelling suggestion failed");
                None
            }
        };
        timings.keyword_ms += ms(t);
    }
    let fused = fusion::rrf(
        &[
            (Source::Keyword, &keyword),
            (Source::Filename, &names),
            (Source::Semantic, &semantic),
        ],
        RRF_K,
    );

    let t = Instant::now();
    let ids: Vec<i64> = fused.iter().map(|f| f.chunk_id).collect();
    let mut rows: HashMap<i64, ChunkRow> = search::chunks(pool, owner_id, &ids, query)
        .await?
        .into_iter()
        .map(|r| (r.id, r))
        .collect();
    // A chunk deleted since retrieval simply drops out.
    let mut ranked: Vec<Ranked> = fused
        .into_iter()
        .filter_map(|f| {
            rows.remove(&f.chunk_id).map(|row| Ranked {
                row,
                scores: f.scores,
                weak: false,
            })
        })
        .collect();
    timings.fetch_ms = ms(t);

    if req.rerank {
        let t = Instant::now();
        rerank(query, models, &mut ranked, &mut meta).await;
        timings.rerank_ms = ms(t);
    }
    relevance::apply(models, &mut ranked);
    meta.timings = timings;
    Ok((meta, ranked))
}

async fn keyword_lists(
    pool: &PgPool,
    owner_id: Uuid,
    query: &str,
    filter: &search::ChunkFilter,
    limit: i64,
) -> Result<(Vec<Candidate>, Vec<Candidate>), sqlx::Error> {
    tokio::try_join!(
        search::keyword(pool, owner_id, query, filter, limit),
        search::filename(pool, owner_id, query, filter, limit),
    )
}

fn degrade(meta: &mut SearchMeta, reason: &str) {
    tracing::warn!(reason, "semantic search unavailable; using keyword search");
    meta.mode = SearchMode::Keyword;
    meta.degraded = true;
    meta.warnings.push(format!(
        "semantic search is unavailable ({reason}); showing keyword results"
    ));
}

async fn embed(embedder: Arc<dyn Embedder>, query: &str) -> Result<Vec<f32>, String> {
    let query = query.to_owned();
    match tokio::task::spawn_blocking(move || embedder.embed_query(&query)).await {
        Ok(Ok(v)) => Ok(v),
        Ok(Err(err)) => Err(err.to_string()),
        Err(err) => Err(format!("embedding panicked: {err}")),
    }
}

fn count(n: usize) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
}

fn ms(since: Instant) -> f64 {
    since.elapsed().as_secs_f64() * 1000.0
}
