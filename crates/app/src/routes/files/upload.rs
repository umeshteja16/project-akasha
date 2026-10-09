//! `POST /api/v1/files`: streaming multipart upload.

use axum::{
    extract::{
        Multipart, State,
        multipart::{Field, MultipartError, MultipartRejection},
    },
    http::StatusCode,
};
use bytes::Bytes;

use super::types::{FileResponse, UploadForm};
use crate::{
    auth::AuthUser,
    error::{ApiError, ErrorBody},
    extract::Json,
    files::{
        name,
        sniff::{self, Detected, SNIFF_LEN, TextValidator},
        store::{self, Saved},
    },
    state::AppState,
};
use akasha_core::Error;
use akasha_storage::{FinishedBlob, StagedBlob};

/// The multipart field that carries the file.
const FILE_FIELD: &str = "file";

/// Upload a file.
///
/// The body is streamed to storage, never held in memory. The type is detected from
/// the bytes and must be on the allow-list; it must also agree with the filename's
/// extension. Re-uploading bytes you already have returns the existing file with
/// `200` instead of `201`. New files start as `pending`.
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
        let (blob, detected) = receive(&state, field, &name, limit).await?;
        // TODO(step 2.4): enqueue the ingestion job for `Saved::Created` files once the
        // job queue exists; until then new files stay `pending`.
        return Ok(
            match store::save(&state, auth.user_id, blob, &name, detected.mime).await? {
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

/// Stream one field into staging, sniffing and validating as it goes.
async fn receive(
    state: &AppState,
    field: Field<'_>,
    name: &str,
    limit: u64,
) -> Result<(FinishedBlob, Detected), ApiError> {
    let mut staged = state.storage.stage().await.map_err(Error::from)?;
    match Receiver::new(name, limit).run(field, &mut staged).await {
        Ok(detected) => {
            let blob = staged.finish().await.map_err(Error::from)?;
            Ok((blob, detected))
        }
        Err(err) => {
            if let Err(abort_err) = staged.abort().await {
                tracing::warn!(err = %abort_err, "failed to abort staged upload");
            }
            Err(err.into())
        }
    }
}

struct Receiver<'a> {
    name: &'a str,
    limit: u64,
    received: u64,
    /// Bytes held back until there are enough to sniff.
    head: Vec<u8>,
    detected: Option<Detected>,
    text: Option<TextValidator>,
}

impl<'a> Receiver<'a> {
    fn new(name: &'a str, limit: u64) -> Self {
        Self {
            name,
            limit,
            received: 0,
            head: Vec::with_capacity(SNIFF_LEN),
            detected: None,
            text: None,
        }
    }

    async fn run(
        mut self,
        mut field: Field<'_>,
        staged: &mut StagedBlob,
    ) -> Result<Detected, Error> {
        while let Some(chunk) = field.chunk().await.map_err(multipart_error)? {
            self.received += chunk.len() as u64;
            if self.received > self.limit {
                return Err(Error::payload_too_large(format!(
                    "file exceeds the upload limit of {} MiB",
                    self.limit / (1024 * 1024)
                )));
            }
            if self.detected.is_some() {
                self.write(staged, chunk).await?;
            } else {
                self.head.extend_from_slice(&chunk);
                if self.head.len() >= SNIFF_LEN {
                    self.sniff(staged).await?;
                }
            }
        }
        if self.detected.is_none() {
            if self.head.is_empty() {
                return Err(Error::bad_request("file is empty"));
            }
            self.sniff(staged).await?;
        }
        if self.text.take().is_some_and(|t| !t.finish()) {
            return Err(not_text());
        }
        self.detected
            .ok_or_else(|| Error::internal("upload type was not detected"))
    }

    async fn sniff(&mut self, staged: &mut StagedBlob) -> Result<(), Error> {
        let detected = sniff::detect(&self.head, self.name)?;
        if detected.is_text {
            self.text = Some(TextValidator::default());
        }
        self.detected = Some(detected);
        let head = Bytes::from(std::mem::take(&mut self.head));
        self.write(staged, head).await
    }

    async fn write(&mut self, staged: &mut StagedBlob, chunk: Bytes) -> Result<(), Error> {
        if let Some(text) = &mut self.text
            && !text.feed(&chunk)
        {
            return Err(not_text());
        }
        staged.write(chunk).await.map_err(Error::from)
    }
}

fn not_text() -> Error {
    Error::unsupported_media_type("text files must be UTF-8 without NUL bytes")
}
