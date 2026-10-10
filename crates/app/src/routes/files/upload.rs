//! `POST /api/v1/files`: streaming multipart upload.

use axum::{
    extract::{
        Multipart, State,
        multipart::{MultipartError, MultipartRejection},
    },
    http::StatusCode,
};
use futures_util::TryStreamExt;

use super::types::{FileResponse, UploadForm};
use crate::{
    auth::AuthUser,
    error::{ApiError, ErrorBody},
    extract::Json,
    files::{
        name,
        receive::receive,
        store::{self, Saved},
    },
    state::AppState,
};
use akasha_core::Error;

/// The multipart field that carries the file.
const FILE_FIELD: &str = "file";

/// Upload a file.
///
/// The body is streamed to storage, never held in memory. The type is detected from
/// the bytes and must be on the allow-list; it must also agree with the filename's
/// extension. Re-uploading bytes you already have returns the existing file with
/// `200` instead of `201`. New files start as `pending` with an extraction job queued.
#[utoipa::path(
    post, path = "/api/v1/files", tag = "files",
    request_body(content = UploadForm, content_type = "multipart/form-data"),
    responses(
        (status = 201, description = "Stored", body = FileResponse),
        (status = 200, description = "Already uploaded: the existing file", body = FileResponse),
        (status = 400, body = ErrorBody), (status = 401, body = ErrorBody),
        (status = 413, description = "`payload_too_large` or `quota_exceeded`", body = ErrorBody),
        (status = 415, description = "`unsupported_media_type`", body = ErrorBody),
    ),
    security(("session_cookie" = []))
)]
pub async fn upload(
    State(state): State<AppState>,
    auth: AuthUser,
    multipart: Result<Multipart, MultipartRejection>,
) -> Result<(StatusCode, Json<FileResponse>), ApiError> {
    let mut multipart = multipart.map_err(|r| Error::bad_request(r.body_text()))?;

    // The quota is checked once the hash is known (in `store::save`, under a lock),
    // so re-uploading bytes you already have never counts against it.
    let limit = state.config.max_upload_bytes();

    while let Some(field) = multipart.next_field().await.map_err(multipart_error)? {
        if field.name() != Some(FILE_FIELD) {
            continue;
        }
        let name = name::sanitize(field.file_name().unwrap_or_default());
        let body = field.map_err(multipart_error);
        let (blob, detected) = receive(&state.storage, &name, limit, body).await?;
        // `save` enqueues the extraction job in the same transaction as the insert.
        return Ok(
            match store::save(&state, (&auth).into(), blob, &name, detected.mime).await? {
                Saved::Created(file) => (StatusCode::CREATED, Json(file.into())),
                Saved::Existing(file) => (StatusCode::OK, Json(file.into())),
            },
        );
    }
    Err(Error::bad_request(format!("missing multipart field `{FILE_FIELD}`")).into())
}

fn multipart_error(err: MultipartError) -> Error {
    if err.status() == StatusCode::PAYLOAD_TOO_LARGE {
        Error::payload_too_large("request body too large")
    } else {
        Error::bad_request(err.body_text())
    }
}
