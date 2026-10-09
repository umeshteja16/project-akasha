//! `GET /api/v1/files/{id}/thumbnail`: a small preview of an image file.

use axum::{
    body::Body,
    extract::{Path, State},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::Response,
};
use uuid::Uuid;

use crate::{
    auth::AuthUser,
    error::{ApiError, ErrorBody},
    jobs::thumbnail::THUMBNAIL_SIZE,
    state::AppState,
};
use akasha_core::Error;
use akasha_storage::ContentHash;

/// A file's contents never change, so its thumbnail can be cached for long.
const CACHE_CONTROL: &str = "private, max-age=604800";

/// A preview of an image file, at most 256 px on its longer side (JPEG, or PNG
/// for images with transparency).
///
/// Generated in the background after upload: `404` until it exists, and always
/// `404` for files that have no preview (PDFs, text, media, images too large or
/// damaged to decode; clients show a type icon instead). Send the `ETag` back in
/// `If-None-Match` to get `304 Not Modified`.
#[utoipa::path(
    get, path = "/api/v1/files/{id}/thumbnail", tag = "files",
    params(("id" = Uuid, Path, description = "File id")),
    responses(
        (status = 200, description = "The thumbnail", content_type = "image/jpeg"),
        (status = 304, description = "Not modified (matching `If-None-Match`)"),
        (status = 401, body = ErrorBody), (status = 404, body = ErrorBody),
    ),
    security(("session_cookie" = []))
)]
pub async fn thumbnail(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    let file = akasha_db::files::get(&state.db, auth.user_id, id)
        .await?
        .ok_or_else(|| Error::not_found("file not found"))?;
    let unavailable = || Error::not_found("no thumbnail for this file");
    if !file.mime_type.starts_with("image/") {
        return Err(unavailable().into());
    }
    let hash: ContentHash = file.content_hash.parse().map_err(Error::from)?;
    let etag = format!("\"{}-{THUMBNAIL_SIZE}\"", hash.to_hex());
    let etag = HeaderValue::from_str(&etag).map_err(|_| Error::internal("invalid etag"))?;
    if if_none_match(&headers, &etag) {
        // The thumbnail exists only if one was ever served with this tag.
        let mut response = Response::new(Body::empty());
        *response.status_mut() = StatusCode::NOT_MODIFIED;
        cache_headers(response.headers_mut(), etag);
        return Ok(response);
    }
    let bytes = state
        .storage
        .get_thumbnail(&hash, THUMBNAIL_SIZE)
        .await
        .map_err(Error::from)?
        .ok_or_else(unavailable)?;
    let content_type = if bytes.starts_with(b"\x89PNG") {
        "image/png"
    } else {
        "image/jpeg"
    };
    let mut response = Response::new(Body::from(bytes));
    let headers = response.headers_mut();
    headers.insert(header::CONTENT_TYPE, HeaderValue::from_static(content_type));
    headers.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    cache_headers(headers, etag);
    Ok(response)
}

fn cache_headers(headers: &mut HeaderMap, etag: HeaderValue) {
    headers.insert(header::ETAG, etag);
    headers.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static(CACHE_CONTROL),
    );
}

/// `If-None-Match` lists our tag (or `*`). Weak comparison, as RFC 9110 asks.
fn if_none_match(headers: &HeaderMap, etag: &HeaderValue) -> bool {
    let ours = etag.to_str().unwrap_or_default();
    headers
        .get_all(header::IF_NONE_MATCH)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|v| v.split(','))
        .map(|t| t.trim().trim_start_matches("W/"))
        .any(|t| t == "*" || t == ours)
}
