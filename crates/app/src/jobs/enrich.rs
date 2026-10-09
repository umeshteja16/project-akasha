//! The `enrich_file` handler: a model-written summary and suggested tags.
//!
//! Idempotent: a file whose summary already describes its current extraction
//! is skipped (unless `force`), and results are stored only while that
//! extraction is still current. Enrichment never changes the file's status or
//! the user's own tags: a failure is recorded as `enrichment_status = failed`
//! and the file stays `ready`. Without a language model nothing happens.

use akasha_db::enrichment;
use akasha_jobs::{JobError, current_attempt};
use akasha_llm::{LlmError, StopReason};
use uuid::Uuid;

use super::{JobContext, kinds::EnrichFile};
use crate::enrich;

pub async fn enrich_file(ctx: JobContext, job: EnrichFile) -> Result<(), JobError> {
    let Some(llm) = ctx.llm.clone() else {
        tracing::debug!(file_id = %job.file_id, "no language model; enrichment skipped");
        return Ok(());
    };
    let id = job.file_id;
    let Some(source) = enrichment::source(&ctx.db, id, enrich::HEAD_CHARS).await? else {
        return Ok(()); // deleted meanwhile
    };
    if source.status != "ready" {
        // Still (re)processing: becoming ready queues a new job.
        return Ok(());
    }
    if !job.force && source.is_current() {
        return Ok(());
    }
    let Some(extracted_at) = source.extracted_at else {
        return Ok(());
    };
    let head = source.head.as_deref().unwrap_or("");
    if head.trim().is_empty() {
        enrichment::mark(&ctx.db, id, "skipped", Some(extracted_at)).await?;
        return Ok(());
    }
    let samples = if source.char_count.unwrap_or(0) > enrich::HEAD_CHARS {
        enrichment::samples(
            &ctx.db,
            id,
            enrich::HEAD_CHARS,
            enrich::SAMPLES,
            enrich::SAMPLE_CHARS,
        )
        .await?
    } else {
        Vec::new()
    };
    let request = enrich::request(
        &source.original_name,
        head,
        &samples,
        ctx.config.llm_temperature,
    );
    let model = format!("{}/{}", llm.provider(), llm.model());
    let failure = match llm.complete(&request).await {
        Ok(c) if c.stop == StopReason::Refusal => Some((true, "the model declined".to_owned())),
        Ok(c) => match enrich::parse(&c.text) {
            Some(e) => {
                let stored =
                    enrichment::store(&ctx.db, id, extracted_at, &e.summary, &e.tags, &model)
                        .await?;
                tracing::info!(file_id = %id, stored, tags = e.tags.len(), %model, "file enriched");
                None
            }
            None => Some((
                false,
                "the model did not answer with the expected JSON".into(),
            )),
        },
        Err(err) => Some((is_permanent(&err), err.to_string())),
    };
    let Some((permanent, detail)) = failure else {
        return Ok(());
    };
    fail(&ctx, id, extracted_at, permanent, detail).await
}

/// Retrying cannot help: bad settings, or the provider rejected the request.
fn is_permanent(err: &LlmError) -> bool {
    match err {
        LlmError::Config(_) => true,
        LlmError::Status { status, .. } => (400..500).contains(status) && *status != 429,
        _ => false,
    }
}

/// Record the failure on the last attempt (earlier summaries stay) and tell
/// the queue whether to retry.
async fn fail(
    ctx: &JobContext,
    id: Uuid,
    extracted_at: chrono::DateTime<chrono::Utc>,
    permanent: bool,
    detail: String,
) -> Result<(), JobError> {
    let last = permanent || current_attempt().is_none_or(|a| a.is_last());
    if last {
        enrichment::mark(&ctx.db, id, "failed", Some(extracted_at)).await?;
    }
    tracing::warn!(file_id = %id, permanent, error = %detail, "enrichment failed");
    Err(if permanent {
        JobError::permanent(detail)
    } else {
        JobError::retry(detail)
    })
}
