//! `GET /api/v1/files/{id}/download`: stream a file's bytes.

use axum::{
    body::Body,
    extract::{Path, State},
    http::{HeaderValue, header},
    response::Response,
};
use serde::Deserialize;
use utoipa::IntoParams;
use uuid::Uuid;

use crate::{
    auth::AuthUser,
    error::{ApiError, ErrorBody},
    extract::Query,
    files::name,
    state::AppState,
};
use akasha_core::Error;
use akasha_storage::ContentHash;

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct DownloadQuery {
    /// Ask to view the file in the browser (`Content-Disposition: inline`) instead
    /// of saving it. Honoured only for PDFs, raster images, audio, video and plain
    /// text; other types are always attachments. Inline responses may be framed by
    /// this site only.
    pub inline: Option<bool>,
}

/// Types a browser may show inline. Never HTML, SVG or anything scriptable.
fn viewable_inline(mime: &str) -> bool {
    matches!(
        mime,
        "application/pdf" | "image/png" | "image/jpeg" | "image/webp" | "image/gif" | "text/plain"
    ) || mime.starts_with("audio/")
        || mime.starts_with("video/")
}

/// The response's `Content-Security-Policy`. Attachments and inline media run
/// sandboxed with no privileges. Browsers' built-in PDF viewers refuse to load in
/// a sandboxed document (and Chrome's in one that forbids plugins), so inline PDFs
/// only restrict framing; `nosniff` keeps them PDFs.
fn content_security_policy(inline: bool, mime: &str) -> &'static str {
    match (inline, mime == "application/pdf") {
        (false, _) => "default-src 'none'; sandbox",
        (true, false) => "default-src 'none'; frame-ancestors 'self'; sandbox",
        (true, true) => "frame-ancestors 'self'",
    }
}

/// Download a file's contents: an attachment unless `inline=true` for a viewable type.
#[utoipa::path(
    get, path = "/api/v1/files/{id}/download", tag = "files",
    params(("id" = Uuid, Path, description = "File id"), DownloadQuery),
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
    Query(query): Query<DownloadQuery>,
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
    let inline = query.inline == Some(true) && viewable_inline(&file.mime_type);
    let disposition = name::content_disposition(&file.original_name);
    let disposition = if inline {
        disposition.replacen("attachment", "inline", 1)
    } else {
        disposition
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
        HeaderValue::from_str(&disposition).map_err(header_error)?,
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
        HeaderValue::from_static(content_security_policy(inline, &file.mime_type)),
    );
    if inline {
        // The app shows previews in a same-origin frame.
        headers.insert(
            header::X_FRAME_OPTIONS,
            HeaderValue::from_static("SAMEORIGIN"),
        );
    }
    Ok(response)
}
