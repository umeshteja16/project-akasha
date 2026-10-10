//! The `extract_file` handler: pending → processing, then `embed_file` (or
//! straight to ready when there is no text) | failed.
//!
//! Idempotent: it re-reads the file (deleted → nothing to do), and replaces the
//! extraction and all chunks in one transaction, so a repeated or concurrent run
//! leaves the same rows behind as a single one.
//!
//! Failure handling: a permanent error (damaged or password-protected file) marks
//! the file `failed` at once. A retryable one (storage, database, OCR models) only
//! marks it `failed` on the job's last attempt; until then it stays `processing`.

use std::sync::Arc;

use akasha_db::{
    enrichment,
    extraction::{self, NewChunk, NewExtraction},
};
use akasha_ingest::{
    Chunk, ChunkOptions, EXTRACTOR_VERSION, Extraction, IngestError, Kind, Ocr, Options,
};
use akasha_jobs::{JobError, current_attempt};
use akasha_media::MediaError;
use akasha_storage::{ContentHash, StorageError};
use bytes::Bytes;
use uuid::Uuid;

use super::{
    JobContext,
    kinds::{EmbedFile, ExtractFile},
};

/// Why an extraction did not finish.
struct Failure {
    permanent: bool,
    /// Shown to the owner (`files.error`).
    user_message: String,
    /// Logged and kept in `jobs.last_error`.
    detail: String,
}

impl Failure {
    fn retry(detail: impl std::fmt::Display) -> Self {
        Self {
            permanent: false,
            user_message: "processing failed because of a server problem; try reindexing later"
                .into(),
            detail: detail.to_string(),
        }
    }

    fn permanent(user_message: impl Into<String>) -> Self {
        let user_message = user_message.into();
        Self {
            permanent: true,
            detail: user_message.clone(),
            user_message,
        }
    }
}

impl From<sqlx::Error> for Failure {
    fn from(err: sqlx::Error) -> Self {
        Self::retry(err)
    }
}

impl From<IngestError> for Failure {
    fn from(err: IngestError) -> Self {
        if err.is_permanent() {
            return Self::permanent(err.to_string());
        }
        Self {
            permanent: false,
            user_message: "text recognition is unavailable right now; try reindexing later".into(),
            detail: err.to_string(),
        }
    }
}

impl From<MediaError> for Failure {
    fn from(err: MediaError) -> Self {
        if err.is_permanent() {
            return Self::permanent(err.to_string());
        }
        Self {
            permanent: false,
            user_message: "speech recognition is unavailable right now; try reindexing later"
                .into(),
            detail: err.to_string(),
        }
    }
}

pub async fn extract_file(ctx: JobContext, job: ExtractFile) -> Result<(), JobError> {
    let id = job.file_id;
    match run(&ctx, id).await {
        Ok(()) => Ok(()),
        Err(failure) => {
            let last = failure.permanent || current_attempt().is_none_or(|a| a.is_last());
            if last {
                extraction::mark_failed(&ctx.db, id, &failure.user_message).await?;
            }
            tracing::warn!(file_id = %id, permanent = failure.permanent, error = %failure.detail, "extraction failed");
            Err(if failure.permanent {
                JobError::permanent(failure.detail)
            } else {
                JobError::retry(failure.detail)
            })
        }
    }
}

async fn run(ctx: &JobContext, id: Uuid) -> Result<(), Failure> {
    let Some(target) = extraction::target(&ctx.db, id).await? else {
        tracing::debug!(file_id = %id, "file deleted before extraction; nothing to do");
        return Ok(());
    };
    if !extraction::mark_processing(&ctx.db, id).await? {
        return Ok(());
    }
    let kind = Kind::of(&target.mime_type);
    if kind == Kind::Media
        && let Some(model) = ctx.transcriber.get().await?
    {
        let hash = parse_hash(&target.content_hash)?;
        let done = super::media::transcribe(ctx, model, &hash, &target.mime_type).await?;
        let chunks = akasha_ingest::chunk(&done.extraction, &super::media::TRANSCRIPT_CHUNKS);
        let duration = i32::try_from(done.duration_ms).ok();
        return store(ctx, id, &done.extraction, &chunks, duration).await;
    }
    let bytes = if matches!(kind, Kind::Media | Kind::Unsupported) {
        // Nothing to extract; do not read (possibly huge) media into memory.
        Bytes::new()
    } else {
        read_blob(ctx, &target.content_hash).await?
    };

    let ocr = if kind == Kind::Image {
        ctx.ocr.get().await?
    } else {
        None
    };
    let mime = target.mime_type.clone();
    let (mut result, mut chunks) = extract_blocking(bytes.clone(), mime.clone(), ocr).await?;
    if kind == Kind::Pdf && result.pages_needing_ocr() > 0 {
        // Second pass only for scanned PDFs, so text PDFs never wait for OCR models.
        match ctx.ocr.get().await {
            Ok(Some(ocr)) => (result, chunks) = extract_blocking(bytes, mime, Some(ocr)).await?,
            Ok(None) => {}
            Err(err) => {
                tracing::warn!(file_id = %id, %err, "OCR unavailable; scanned pages left without text")
            }
        }
    }
    store(ctx, id, &result, &chunks, None).await
}

fn parse_hash(hash: &str) -> Result<ContentHash, Failure> {
    hash.parse()
        .map_err(|_| Failure::permanent("the file record is invalid"))
}

async fn read_blob(ctx: &JobContext, hash: &str) -> Result<Bytes, Failure> {
    let hash = parse_hash(hash)?;
    match ctx.storage.get_bytes(&hash).await {
        Ok(bytes) => Ok(bytes),
        Err(StorageError::NotFound) => Err(Failure::permanent("the file's contents are missing")),
        Err(err) => Err(Failure::retry(err)),
    }
}

/// Extraction and chunking are CPU-bound: run them off the async workers.
async fn extract_blocking(
    bytes: Bytes,
    mime: String,
    ocr: Option<Arc<Ocr>>,
) -> Result<(Extraction, Vec<Chunk>), Failure> {
    let joined = tokio::task::spawn_blocking(move || {
        let options = Options {
            ocr: ocr.as_deref(),
            ..Options::default()
        };
        let extraction = akasha_ingest::extract(&bytes, &mime, &options)?;
        let chunks = akasha_ingest::chunk(&extraction, &ChunkOptions::default());
        Ok::<_, IngestError>((extraction, chunks))
    })
    .await;
    match joined {
        Ok(result) => Ok(result?),
        Err(err) => Err(Failure::retry(format!("extraction task failed: {err}"))),
    }
}

fn to_i32(n: usize) -> Result<i32, Failure> {
    i32::try_from(n).map_err(|_| Failure::permanent("this file has too much text to index"))
}

/// Replace the stored extraction and chunks, then move the file on.
async fn store(
    ctx: &JobContext,
    id: Uuid,
    result: &Extraction,
    chunks: &[Chunk],
    duration_ms: Option<i32>,
) -> Result<(), Failure> {
    let pages = serde_json::to_value(&result.pages).map_err(Failure::retry)?;
    let segments = serde_json::to_value(&result.segments).map_err(Failure::retry)?;
    let paged = result.extractor == "pdf";
    let new = NewExtraction {
        extractor: result.extractor,
        extractor_version: EXTRACTOR_VERSION,
        page_count: if paged {
            Some(to_i32(result.pages.len())?)
        } else {
            None
        },
        char_count: to_i32(result.char_count())?,
        text: &result.text,
        pages,
        segments,
        duration_ms,
        notes: &result.notes,
    };
    let rows = chunks
        .iter()
        .map(|c| {
            Ok(NewChunk {
                chunk_index: to_i32(c.index as usize)?,
                page: c.page.map(|p| to_i32(p as usize)).transpose()?,
                start_ms: c.start_ms.map(|t| to_i32(t as usize)).transpose()?,
                end_ms: c.end_ms.map(|t| to_i32(t as usize)).transpose()?,
                char_start: to_i32(c.char_start)?,
                char_end: to_i32(c.char_end)?,
                text: &c.text,
            })
        })
        .collect::<Result<Vec<_>, Failure>>()?;

    let mut tx = ctx.db.begin().await?;
    if extraction::replace(&mut tx, id, &new, &rows)
        .await?
        .is_none()
    {
        tracing::debug!(file_id = %id, "file deleted during extraction; nothing stored");
        return Ok(());
    }
    if chunks.is_empty() {
        // Nothing to embed (media, empty or text-less files), and nothing to
        // summarise: enrichment is `skipped` from the moment the file is ready.
        extraction::mark_ready(&mut tx, id).await?;
        enrichment::skip_current(&mut tx, id).await?;
    } else {
        // The file stays `processing` until its chunks have vectors.
        akasha_jobs::enqueue(&mut tx, &EmbedFile { file_id: id })
            .await
            .map_err(Failure::retry)?;
    }
    tx.commit().await?;
    tracing::info!(
        file_id = %id,
        extractor = result.extractor,
        chars = result.char_count(),
        chunks = chunks.len(),
        "file extracted"
    );
    Ok(())
}
