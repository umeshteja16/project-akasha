//! `/api/v1/files`: upload, list, read, update, delete and download files.
//! Every query is filtered by the signed-in owner; other users' files are 404.

pub mod download;
pub mod types;
pub mod upload;

use axum::{
    extract::{Path, State},
    http::StatusCode,
};
use uuid::Uuid;

use self::types::{
    BulkDeleteRequest, BulkDeleteResponse, FileList, FileResponse, ListQuery, UpdateFileRequest,
    decode_cursor, encode_cursor, normalize_tags,
};
use crate::{
    auth::AuthUser,
    error::{ApiError, ErrorBody},
    extract::{Json, Query},
    files::{name, store},
    state::AppState,
};
use akasha_core::Error;
use akasha_db::files::{self, ListFilter};

const DEFAULT_PAGE: i64 = 50;
const MAX_PAGE: i64 = 200;
const MAX_BULK: usize = 100;

fn not_found() -> ApiError {
    Error::not_found("file not found").into()
}

/// List your files, newest first.
#[utoipa::path(
    get, path = "/api/v1/files", tag = "files",
    params(ListQuery),
    responses(
        (status = 200, body = FileList),
        (status = 400, body = ErrorBody), (status = 401, body = ErrorBody),
    ),
    security(("session_cookie" = []))
)]
pub async fn list(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(query): Query<ListQuery>,
) -> Result<Json<FileList>, ApiError> {
    let limit = query.limit.unwrap_or(DEFAULT_PAGE);
    if !(1..=MAX_PAGE).contains(&limit) {
        return Err(Error::bad_request(format!("limit must be 1-{MAX_PAGE}")).into());
    }
    let filter = ListFilter {
        status: query.status.map(|s| s.as_str().to_owned()),
        pinned: query.pinned,
        tag: query.tag.map(|t| t.trim().to_lowercase()),
        mime_patterns: query
            .category
            .map(|c| c.mime_patterns())
            .unwrap_or_default(),
        before: query.cursor.as_deref().map(decode_cursor).transpose()?,
        // One extra row tells us whether there is a next page.
        limit: limit + 1,
    };
    let mut rows = files::list(&state.db, auth.user_id, &filter).await?;
    let has_more = rows.len() as i64 > limit;
    rows.truncate(usize::try_from(limit).unwrap_or(0));
    let next_cursor = has_more
        .then(|| rows.last().map(|f| encode_cursor(f.created_at, f.id)))
        .flatten();
    Ok(Json(FileList {
        items: rows.into_iter().map(Into::into).collect(),
        next_cursor,
    }))
}

/// One of your files.
#[utoipa::path(
    get, path = "/api/v1/files/{id}", tag = "files",
    params(("id" = Uuid, Path, description = "File id")),
    responses(
        (status = 200, body = FileResponse),
        (status = 401, body = ErrorBody), (status = 404, body = ErrorBody),
    ),
    security(("session_cookie" = []))
)]
pub async fn get(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<FileResponse>, ApiError> {
    let file = files::get(&state.db, auth.user_id, id)
        .await?
        .ok_or_else(not_found)?;
    Ok(Json(file.into()))
}

/// Rename, pin/unpin or retag a file.
#[utoipa::path(
    patch, path = "/api/v1/files/{id}", tag = "files",
    params(("id" = Uuid, Path, description = "File id")),
    request_body = UpdateFileRequest,
    responses(
        (status = 200, body = FileResponse),
        (status = 400, body = ErrorBody), (status = 401, body = ErrorBody),
        (status = 404, body = ErrorBody),
    ),
    security(("session_cookie" = []))
)]
pub async fn update(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdateFileRequest>,
) -> Result<Json<FileResponse>, ApiError> {
    if req.name.is_none() && req.is_pinned.is_none() && req.tags.is_none() {
        return Err(Error::bad_request("nothing to update").into());
    }
    let new_name = req.name.as_deref().map(name::validate_rename).transpose()?;
    let tags = req.tags.as_deref().map(normalize_tags).transpose()?;
    let file = files::update(
        &state.db,
        auth.user_id,
        id,
        new_name.as_deref(),
        req.is_pinned,
        tags.as_deref(),
    )
    .await?
    .ok_or_else(not_found)?;
    Ok(Json(file.into()))
}

/// Delete a file. Its contents are removed once no other file uses them.
#[utoipa::path(
    delete, path = "/api/v1/files/{id}", tag = "files",
    params(("id" = Uuid, Path, description = "File id")),
    responses(
        (status = 204, description = "Deleted"),
        (status = 401, body = ErrorBody), (status = 404, body = ErrorBody),
    ),
    security(("session_cookie" = []))
)]
pub async fn delete(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, ApiError> {
    if store::delete(&state, auth.user_id, id).await? {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(not_found())
    }
}

/// Delete several files at once.
#[utoipa::path(
    post, path = "/api/v1/files/bulk-delete", tag = "files",
    request_body = BulkDeleteRequest,
    responses(
        (status = 200, body = BulkDeleteResponse),
        (status = 400, body = ErrorBody), (status = 401, body = ErrorBody),
    ),
    security(("session_cookie" = []))
)]
pub async fn bulk_delete(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(req): Json<BulkDeleteRequest>,
) -> Result<Json<BulkDeleteResponse>, ApiError> {
    if req.ids.len() > MAX_BULK {
        return Err(Error::bad_request(format!("at most {MAX_BULK} ids per request")).into());
    }
    let mut deleted = Vec::with_capacity(req.ids.len());
    for id in req.ids {
        if !deleted.contains(&id) && store::delete(&state, auth.user_id, id).await? {
            deleted.push(id);
        }
    }
    Ok(Json(BulkDeleteResponse { deleted }))
}
