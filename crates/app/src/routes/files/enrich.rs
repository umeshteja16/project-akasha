//! `POST /api/v1/files/{id}/enrich`: write the summary and suggested tags again.

use axum::{
    extract::{Path, State},
    http::StatusCode,
};
use uuid::Uuid;

use super::{not_found, processing};
use crate::{
    auth::AuthUser,
    error::{ApiError, ErrorBody},
    extract::Json,
    jobs::kinds::EnrichFile,
    rate_limit,
    state::AppState,
};
use akasha_core::Error;
use akasha_db::files;

/// Ask the language model again for the file's summary and suggested tags
/// (e.g. after a failure or a model change). Your own tags are never changed.
/// The result shows up on the file once the background job ran. Needs a
/// configured language model; limited per user.
#[utoipa::path(
    post, path = "/api/v1/files/{id}/enrich", tag = "files",
    params(("id" = Uuid, Path, description = "File id")),
    responses(
        (status = 202, description = "Enrichment queued", body = processing::FileDetail),
        (status = 401, body = ErrorBody), (status = 404, body = ErrorBody),
        (status = 409, description = "The file is not ready yet", body = ErrorBody),
        (status = 429, body = ErrorBody),
        (status = 503, description = "No language model is configured", body = ErrorBody),
    ),
    security(("session_cookie" = []))
)]
pub async fn enrich(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<(StatusCode, Json<processing::FileDetail>), ApiError> {
    let file = files::get(&state.db, auth.user_id, id)
        .await?
        .ok_or_else(not_found)?;
    if state.llm.is_none() {
        return Err(Error::unavailable("no language model is configured").into());
    }
    if file.status != "ready" {
        return Err(Error::conflict("the file is still being processed (or failed)").into());
    }
    rate_limit::check_user(&state.enrich_limiter, auth.user_id, "enrichments")?;
    let mut tx = state.db.begin().await?;
    akasha_jobs::enqueue(
        &mut tx,
        &EnrichFile {
            file_id: id,
            force: true,
        },
    )
    .await?;
    tx.commit().await?;
    let processing = processing::latest(&state.db, id).await?;
    Ok((
        StatusCode::ACCEPTED,
        Json(processing::FileDetail {
            file: file.into(),
            processing,
        }),
    ))
}
