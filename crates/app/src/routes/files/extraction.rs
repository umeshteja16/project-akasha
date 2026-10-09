//! `GET /api/v1/files/{id}/extraction`: the text extracted from a file.

use axum::extract::{Path, State};
use chrono::{DateTime, Utc};
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
use akasha_core::Error;
use akasha_db::{extraction, files};

const DEFAULT_LIMIT: i32 = 100_000;
const MAX_LIMIT: i32 = 1_000_000;

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct ExtractionQuery {
    /// First character to return (0-based, in Unicode characters). Default 0.
    pub offset: Option<i32>,
    /// Characters to return, 1-1000000. Default 100000.
    pub limit: Option<i32>,
}

/// Where a PDF page's text came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum PageSource {
    /// The PDF's text layer.
    Text,
    /// Recognised from the page image.
    Ocr,
    /// No text layer; OCR was disabled or could not read it.
    NeedsOcr,
    /// The page could not be parsed.
    Unreadable,
}

/// A PDF page's span in the full text (`char_start..char_end`, in characters).
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct PageSpan {
    /// 1-based page number.
    pub number: u32,
    pub char_start: u64,
    pub char_end: u64,
    pub source: PageSource,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ExtractionResponse {
    pub file_id: Uuid,
    /// `text`, `markdown`, `csv`, `json`, `pdf`, `ocr`, or `none` when nothing could be
    /// extracted (e.g. audio and video, which are not transcribed yet).
    pub extractor: String,
    pub extractor_version: String,
    /// Pages in the document; `null` for formats without pages.
    pub page_count: Option<i32>,
    /// Length of the whole text in characters.
    pub char_count: i32,
    /// Chunks indexed for search.
    pub chunk_count: i64,
    /// Page spans, in order (PDFs only).
    pub pages: Vec<PageSpan>,
    /// Remarks such as "OCR is disabled" or "truncated".
    pub notes: Vec<String>,
    /// Character offset of `text` in the whole text.
    pub offset: i32,
    /// The requested window of the text.
    pub text: String,
    /// Pass as `offset` to read on; `null` at the end.
    pub next_offset: Option<i32>,
    pub created_at: DateTime<Utc>,
}

/// The text extracted from one of your files, a window at a time.
#[utoipa::path(
    get, path = "/api/v1/files/{id}/extraction", tag = "files", operation_id = "get_extraction",
    params(("id" = Uuid, Path, description = "File id"), ExtractionQuery),
    responses(
        (status = 200, body = ExtractionResponse),
        (status = 400, body = ErrorBody), (status = 401, body = ErrorBody),
        (status = 404, description = "No such file, or not extracted yet", body = ErrorBody),
    ),
    security(("session_cookie" = []))
)]
pub async fn get(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Query(query): Query<ExtractionQuery>,
) -> Result<Json<ExtractionResponse>, ApiError> {
    let offset = query.offset.unwrap_or(0);
    let limit = query.limit.unwrap_or(DEFAULT_LIMIT);
    if offset < 0 {
        return Err(Error::bad_request("offset must not be negative").into());
    }
    if !(1..=MAX_LIMIT).contains(&limit) {
        return Err(Error::bad_request(format!("limit must be 1-{MAX_LIMIT}")).into());
    }
    let Some(found) = extraction::get(&state.db, auth.user_id, id, offset, limit).await? else {
        // Tell "no such file" apart from "not extracted yet".
        files::get(&state.db, auth.user_id, id)
            .await?
            .ok_or_else(not_found)?;
        return Err(Error::not_found("this file has not been extracted yet").into());
    };
    let pages: Vec<PageSpan> = serde_json::from_value(found.pages).map_err(|err| {
        tracing::error!(%err, file_id = %id, "stored page spans are malformed");
        Error::internal("stored extraction is malformed")
    })?;
    let end = offset.saturating_add(limit);
    Ok(Json(ExtractionResponse {
        file_id: id,
        extractor: found.extractor,
        extractor_version: found.extractor_version,
        page_count: found.page_count,
        char_count: found.char_count,
        chunk_count: found.chunk_count,
        pages,
        notes: found.notes,
        offset,
        text: found.text,
        next_offset: (end < found.char_count).then_some(end),
        created_at: found.created_at,
    }))
}
