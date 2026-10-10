//! `/api/v1/search`: hybrid search over the signed-in user's files
//! (`akasha-search`, ADR 0010). Rate-limited per user.

pub mod params;

use std::time::Duration;

use akasha_search::{ChunkResults, FileResults, Models};
use axum::extract::State;

use self::params::SearchQuery;
use crate::{
    auth::AuthUser,
    error::{ApiError, ErrorBody},
    extract::{Json, Query},
    rate_limit,
    state::AppState,
};

/// How long a search waits for a model that is not loaded yet before running
/// without it (the load continues in the background).
const MODEL_WAIT: Duration = Duration::from_secs(5);

/// Search your files; results are grouped by file with their best passages.
///
/// When semantic search is unavailable (model not installed or still loading),
/// the search runs as keyword search and the response says so (`degraded`,
/// `warnings`) instead of failing.
#[utoipa::path(
    get, path = "/api/v1/search", tag = "search",
    params(SearchQuery),
    responses(
        (status = 200, body = FileResults),
        (status = 400, body = ErrorBody), (status = 401, body = ErrorBody),
        (status = 429, body = ErrorBody),
    ),
    security(("session_cookie" = []))
)]
pub async fn search(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(query): Query<SearchQuery>,
) -> Result<Json<FileResults>, ApiError> {
    rate_limit::check_user_audited(&state.db, &state.search_limiter, (&auth).into(), "searches")?;
    let req = query.into_request()?;
    check_collection(&state, &auth, &req).await?;
    let models = models(&state).await;
    let res = akasha_search::search_files(&state.db, auth.user_id, &req, &models).await?;
    log(&res.meta, res.results.len());
    if req.offset == 0 {
        record(&state, &auth, &req, res.results.len(), res.meta.has_more).await;
    }
    Ok(Json(res))
}

/// Search your files passage by passage (chunk-level results with full text,
/// for agents and citations).
#[utoipa::path(
    get, path = "/api/v1/search/chunks", tag = "search",
    params(SearchQuery),
    responses(
        (status = 200, body = ChunkResults),
        (status = 400, body = ErrorBody), (status = 401, body = ErrorBody),
        (status = 429, body = ErrorBody),
    ),
    security(("session_cookie" = []))
)]
pub async fn search_chunks(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(query): Query<SearchQuery>,
) -> Result<Json<ChunkResults>, ApiError> {
    rate_limit::check_user_audited(&state.db, &state.search_limiter, (&auth).into(), "searches")?;
    let req = query.into_request()?;
    check_collection(&state, &auth, &req).await?;
    let models = models(&state).await;
    let res = akasha_search::search_chunks(&state.db, auth.user_id, &req, &models).await?;
    log(&res.meta, res.results.len());
    Ok(Json(res))
}

/// The configured models, or why each is unavailable. Details go to the log;
/// users see a short reason.
/// A `collection_id` must be one of the caller's collections (else 404).
async fn check_collection(
    state: &AppState,
    auth: &AuthUser,
    req: &akasha_search::SearchRequest,
) -> Result<(), ApiError> {
    match req.filter.collection_id {
        Some(id) => crate::routes::collections::ensure_owned(state, auth.user_id, id).await,
        None => Ok(()),
    }
}

/// A first-page search goes into the activity timeline (unless the user turned
/// search history off; typing refines one entry). Agents' chunk searches and
/// later pages are not recorded.
async fn record(
    state: &AppState,
    auth: &AuthUser,
    req: &akasha_search::SearchRequest,
    results: usize,
    more: bool,
) {
    let actor = crate::activity::Actor::from(auth);
    let details = serde_json::json!({
        "results": results,
        "more": more,
        "mode": req.mode,
        "collection_id": req.filter.collection_id,
    });
    if let Err(e) =
        akasha_db::activity::record_search(&state.db, auth.user_id, &req.query, details, actor.via)
            .await
    {
        tracing::warn!(error = %e, "could not record a search");
    }
}

pub async fn models(state: &AppState) -> Models {
    let (embedder, reranker) = tokio::join!(
        state.ml.embedder_within(MODEL_WAIT),
        state.ml.reranker_within(MODEL_WAIT),
    );
    let embedder = match embedder {
        Ok(Some(e)) => Ok(e),
        Ok(None) => Err("the embedding model is still loading".to_owned()),
        Err(err) => {
            tracing::warn!(%err, "embedding model unavailable for search");
            Err("the embedding model could not be loaded".to_owned())
        }
    };
    let reranker = match reranker {
        Ok(Some(r)) => Ok(r),
        Ok(None) => Err("the reranker is still loading".to_owned()),
        Err(err) => {
            tracing::warn!(%err, "reranker unavailable for search");
            Err("the reranker could not be loaded".to_owned())
        }
    };
    Models {
        embedder,
        reranker,
        floor: akasha_search::RelevanceFloor {
            min_similarity: state.config.search_min_similarity,
            min_rerank_score: state.config.search_min_rerank_score,
        },
    }
}

/// The query itself is not logged (it may be private).
fn log(meta: &akasha_search::SearchMeta, results: usize) {
    tracing::info!(
        mode = ?meta.mode,
        degraded = meta.degraded,
        reranked = meta.reranked,
        results,
        query_chars = meta.query.chars().count(),
        total_ms = meta.timings.total_ms,
        "search"
    );
}
