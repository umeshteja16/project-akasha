//! `GET /api/v1/files/{id}/download`: stream a file's bytes.

use axum::{
    body::Body,
    extract::{Path, State},
    http::{HeaderValue, header},
    response::Response,
};
use uuid::Uuid;

use crate::{
    auth::AuthUser,
    error::{ApiError, ErrorBody},
    files::name,
    state::AppState,
};
use akasha_core::Error;
use akasha_storage::ContentHash;

/// Download a file's contents (always as an attachment, never rendered inline).
#[utoipa::path(
    get, path = "/api/v1/files/{id}/download", tag = "files",
    params(("id" = Uuid, Path, description = "File id")),
    responses(
        (status = 200, description = "The file's bytes, with its detected `Content-Type`",
         content_type = "application/octet-stream"),
        (status = 401, body = ErrorBody), (status = 404, body = ErrorBody),
    ),
    security(("session_cookie" = []))
)]
pub async fn download(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Response, ApiError> {
    let file = akasha_db::files::get(&state.db, auth.user_id, id)
        .await?
        .ok_or_else(|| Error::not_found("file not found"))?;
    let hash: ContentHash = file.content_hash.parse().map_err(Error::from)?;
    let blob = state.storage.get(&hash).await.map_err(Error::from)?;

    let content_type = if file.mime_type.starts_with("text/") {
        format!("{}; charset=utf-8", file.mime_type)
    } else {
        file.mime_type.clone()
    };
    let header_error = |_| Error::internal("invalid header value");
    let mut response = Response::new(Body::from_stream(blob.stream));
    let headers = response.headers_mut();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_str(&content_type).map_err(header_error)?,
    );
    headers.insert(header::CONTENT_LENGTH, HeaderValue::from(blob.size));
    headers.insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_str(&name::content_disposition(&file.original_name))
            .map_err(header_error)?,
    );
    headers.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    headers.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("private, no-store"),
    );
    // Even if a browser does render it, run it with no privileges.
    headers.insert(
        header::CONTENT_SECURITY_POLICY,
        HeaderValue::from_static("default-src 'none'; sandbox"),
    );
    Ok(response)
}
