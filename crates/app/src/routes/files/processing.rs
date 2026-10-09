//! Processing (extraction, then embedding) state of a file: shown on `GET /api/v1/files/{id}` and
//! restarted by `POST /api/v1/files/{id}/reindex`.

use akasha_jobs::{Job, JobInfo, queue};
use axum::{
    extract::{Path, State},
    http::StatusCode,
};
use chrono::{DateTime, Utc};
use serde::Serialize;
use utoipa::ToSchema;
use uuid::Uuid;

use super::{not_found, types::FileResponse};
use crate::{
    auth::AuthUser,
    error::{ApiError, ErrorBody},
    extract::Json,
    jobs::kinds::{EmbedFile, ExtractFile},
    state::AppState,
};
use akasha_db::{PgPool, files};

/// One file plus the state of its latest processing job.
#[derive(Debug, Serialize, ToSchema)]
pub struct FileDetail {
    #[serde(flatten)]
    pub file: FileResponse,
    /// Latest processing job (extraction or embedding), if one was ever queued.
    pub processing: Option<ProcessingJob>,
}

/// State of a background job.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum JobState {
    /// Waiting for a worker.
    Queued,
    Running,
    Succeeded,
    /// An attempt failed; it will be retried at `next_attempt_at`.
    Failed,
    /// Gave up after its last attempt.
    Dead,
}

impl JobState {
    fn from_db(value: &str) -> Self {
        match value {
            "queued" => Self::Queued,
            "running" => Self::Running,
            "succeeded" => Self::Succeeded,
            "failed" => Self::Failed,
            // The column has a CHECK constraint; anything else is a bug.
            _ => Self::Dead,
        }
    }
}

/// Which processing step a job performs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum ProcessingStage {
    /// Text extraction and chunking.
    Extract,
    /// Computing chunk embeddings for semantic search.
    Embed,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ProcessingJob {
    pub stage: ProcessingStage,
    pub state: JobState,
    /// Attempts started so far.
    pub attempts: i32,
    pub max_attempts: i32,
    /// When it will next be tried (`queued` or `failed` only).
    pub next_attempt_at: Option<DateTime<Utc>>,
    pub updated_at: DateTime<Utc>,
}

impl From<JobInfo> for ProcessingJob {
    fn from(job: JobInfo) -> Self {
        let state = JobState::from_db(&job.status);
        let stage = if job.kind == EmbedFile::KIND {
            ProcessingStage::Embed
        } else {
            ProcessingStage::Extract
        };
        Self {
            stage,
            state,
            attempts: job.attempts,
            max_attempts: job.max_attempts,
            next_attempt_at: matches!(state, JobState::Queued | JobState::Failed)
                .then_some(job.run_at),
            updated_at: job.updated_at,
        }
    }
}

/// The latest processing job for `file_id`: the embed job once extraction queued
/// one, else the extraction job. Callers must have checked ownership.
pub async fn latest(db: &PgPool, file_id: Uuid) -> Result<Option<ProcessingJob>, ApiError> {
    let key = file_id.to_string();
    let extract = queue::latest_by_key(db, ExtractFile::KIND, &key).await?;
    let embed = queue::latest_by_key(db, EmbedFile::KIND, &key).await?;
    let job = match (extract, embed) {
        // A reindex queues a new extraction after the old embedding.
        (Some(x), Some(e)) => Some(if e.created_at >= x.created_at { e } else { x }),
        (x, e) => x.or(e),
    };
    Ok(job.map(Into::into))
}

/// Extract a file again (e.g. after a failure). Puts it back to `pending`.
#[utoipa::path(
    post, path = "/api/v1/files/{id}/reindex", tag = "files",
    params(("id" = Uuid, Path, description = "File id")),
    responses(
        (status = 202, description = "Extraction queued", body = FileDetail),
        (status = 401, body = ErrorBody), (status = 404, body = ErrorBody),
    ),
    security(("session_cookie" = []))
)]
pub async fn reindex(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<(StatusCode, Json<FileDetail>), ApiError> {
    let mut tx = state.db.begin().await?;
    let file = files::mark_pending(&mut tx, auth.user_id, id)
        .await?
        .ok_or_else(not_found)?;
    // Deduplicated: reindexing twice while queued queues one job.
    akasha_jobs::enqueue(&mut tx, &ExtractFile { file_id: id }).await?;
    tx.commit().await?;
    let processing = latest(&state.db, id).await?;
    Ok((
        StatusCode::ACCEPTED,
        Json(FileDetail {
            file: file.into(),
            processing,
        }),
    ))
}
