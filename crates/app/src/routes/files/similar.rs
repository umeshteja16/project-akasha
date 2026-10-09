//! `GET /api/v1/files/{id}/similar`: your other files closest in meaning.

use akasha_search::SimilarFile;
use axum::extract::{Path, State};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use super::not_found;
use crate::{
    auth::AuthUser,
    error::{ApiError, ErrorBody},
    extract::{Json, Query},
    state::AppState,
};

const DEFAULT_LIMIT: usize = 5;

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct SimilarQuery {
    /// How many files, 1-20 (default 5).
    pub limit: Option<usize>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct SimilarFiles {
    /// Most similar first. Empty while the file is not embedded yet.
    pub items: Vec<SimilarFile>,
}

/// Files similar to one of yours, by embedding (mean of the file's chunk vectors).
#[utoipa::path(
    get, path = "/api/v1/files/{id}/similar", tag = "files",
    params(("id" = Uuid, Path, description = "File id"), SimilarQuery),
    responses(
        (status = 200, body = SimilarFiles),
        (status = 400, body = ErrorBody), (status = 401, body = ErrorBody),
        (status = 404, body = ErrorBody),
    ),
    security(("session_cookie" = []))
)]
pub async fn similar(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Query(query): Query<SimilarQuery>,
) -> Result<Json<SimilarFiles>, ApiError> {
    let limit = query.limit.unwrap_or(DEFAULT_LIMIT);
    let items = akasha_search::similar_files(&state.db, auth.user_id, id, limit)
        .await?
        .ok_or_else(not_found)?;
    Ok(Json(SimilarFiles { items }))
}
