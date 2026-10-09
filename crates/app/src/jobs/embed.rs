//! The `embed_file` handler: vectors for a file's chunks, then processing → ready.
//!
//! Idempotent and resumable: only chunks without a vector are embedded and each
//! batch commits on its own, so a retry (or a duplicate delivery) continues where
//! the last run stopped and never redoes finished chunks. Vectors are written only
//! while the configured model is the one recorded in the database (ADR 0009).
//!
//! A file that becomes `ready` gets an `enrich_file` job in the same
//! transaction when a language model is configured.
//!
//! Failure handling mirrors extraction: the file stays `processing` until the
//! job's last attempt, then becomes `failed` with a user-safe message.

use std::sync::Arc;

use akasha_db::{
    embeddings::{self, Finish, ModelCheck},
    extraction,
};
use akasha_jobs::{JobError, current_attempt};
use akasha_ml::{Embedder, MlError};
use uuid::Uuid;

use super::{
    JobContext,
    kinds::{EmbedFile, EnrichFile},
};

/// Chunks per database round trip (and per call into the model, which batches
/// further internally). Bounds memory: 64 × 2000 characters of text.
const BATCH: i64 = 64;

const USER_MESSAGE: &str =
    "semantic indexing failed because of a server problem; try reindexing later";

/// Why embedding did not finish (the detail goes to `jobs.last_error`).
struct Failure {
    permanent: bool,
    detail: String,
}

impl Failure {
    fn retry(detail: impl std::fmt::Display) -> Self {
        Self {
            permanent: false,
            detail: detail.to_string(),
        }
    }
}

impl From<sqlx::Error> for Failure {
    fn from(err: sqlx::Error) -> Self {
        Self::retry(err)
    }
}

impl From<MlError> for Failure {
    fn from(err: MlError) -> Self {
        Self {
            permanent: err.is_permanent(),
            detail: err.to_string(),
        }
    }
}

pub async fn embed_file(ctx: JobContext, job: EmbedFile) -> Result<(), JobError> {
    let id = job.file_id;
    match run(&ctx, id).await {
        Ok(()) => Ok(()),
        Err(failure) => {
            let last = failure.permanent || current_attempt().is_none_or(|a| a.is_last());
            if last {
                extraction::mark_failed(&ctx.db, id, USER_MESSAGE).await?;
            }
            tracing::warn!(file_id = %id, permanent = failure.permanent, error = %failure.detail, "embedding failed");
            Err(if failure.permanent {
                JobError::permanent(failure.detail)
            } else {
                JobError::retry(failure.detail)
            })
        }
    }
}

async fn run(ctx: &JobContext, id: Uuid) -> Result<(), Failure> {
    let mut batch = embeddings::pending(&ctx.db, id, BATCH).await?;
    let mut embedded = 0u64;
    if !batch.is_empty() {
        // Load the model only when there is work (files without text never do).
        let embedder = ctx.ml.embedder().await?;
        let model = embedder.model();
        let dim = i32::try_from(model.dim).map_err(Failure::retry)?;
        ensure_model(ctx, model.name, dim).await?;
        while !batch.is_empty() {
            let ids: Vec<i64> = batch.iter().map(|c| c.id).collect();
            let texts: Vec<String> = batch.into_iter().map(|c| c.text).collect();
            let vectors = embed_blocking(Arc::clone(&embedder), texts).await?;
            let mut conn = ctx.db.acquire().await?;
            let stored = embeddings::store(&mut conn, model.name, dim, &ids, &vectors).await?;
            drop(conn);
            if stored == 0 {
                // Either the chunks were replaced (re-extraction queues its own
                // job) or the recorded model changed under us: find out which.
                ensure_model(ctx, model.name, dim).await?;
                break;
            }
            embedded += stored;
            batch = embeddings::pending(&ctx.db, id, BATCH).await?;
        }
    }

    let mut tx = ctx.db.begin().await?;
    let finish = embeddings::finish(&mut tx, id).await?;
    if finish == (Finish::Done { became_ready: true }) && ctx.enriches_files() {
        // Same transaction: the summary job exists exactly when the file is ready.
        akasha_jobs::enqueue(
            &mut tx,
            &EnrichFile {
                file_id: id,
                force: false,
            },
        )
        .await
        .map_err(Failure::retry)?;
    }
    tx.commit().await?;
    match finish {
        Finish::Done { .. } => tracing::info!(file_id = %id, chunks = embedded, "file embedded"),
        Finish::Gone => tracing::debug!(file_id = %id, "file deleted during embedding"),
        Finish::Pending => {
            tracing::debug!(file_id = %id, "chunks changed during embedding; a newer job finishes")
        }
    }
    Ok(())
}

/// Refuse to write vectors from a model other than the recorded one.
async fn ensure_model(ctx: &JobContext, name: &str, dim: i32) -> Result<(), Failure> {
    match embeddings::check_model(&ctx.db, name, dim).await? {
        ModelCheck::Matches => Ok(()),
        ModelCheck::Mismatch(recorded) => Err(Failure::retry(format!(
            "the database holds vectors from `{}` ({} dimensions) but this worker runs `{name}` \
             ({dim}); set AKASHA_EMBED_MODEL back or run `akasha reembed`",
            recorded.name, recorded.dim
        ))),
        ModelCheck::ColumnMismatch { column_dim } => Err(Failure::retry(format!(
            "file_chunks.embedding has {column_dim} dimensions but `{name}` makes {dim}; \
             run `akasha reembed`"
        ))),
    }
}

/// Inference is CPU-bound: run it on the blocking pool. Returns the vectors
/// flattened row after row (the layout `embeddings::store` takes).
async fn embed_blocking(
    embedder: Arc<dyn Embedder>,
    texts: Vec<String>,
) -> Result<Vec<f32>, Failure> {
    let joined = tokio::task::spawn_blocking(move || {
        let refs: Vec<&str> = texts.iter().map(String::as_str).collect();
        let vectors = embedder.embed_documents(&refs)?;
        let dim = embedder.model().dim;
        if vectors.len() != refs.len() || vectors.iter().any(|v| v.len() != dim) {
            return Err(MlError::Inference(
                "model returned malformed vectors".into(),
            ));
        }
        Ok(vectors.concat())
    })
    .await;
    match joined {
        Ok(result) => Ok(result?),
        Err(err) => Err(Failure::retry(format!("embedding task failed: {err}"))),
    }
}
