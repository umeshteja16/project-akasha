//! Loads the corpus through the real pipeline: upload checks, storage, the
//! extraction and embedding jobs.

use std::path::Path;

use akasha_db::{
    embeddings::{self, ModelCheck},
    scratch, users,
};
use akasha_ml::catalog;
use anyhow::{Context, bail};
use bytes::Bytes;
use uuid::Uuid;

use crate::{
    files::{sniff, store},
    jobs::{self, JobContext},
    state::AppState,
};

/// Record the configured embedding model in the (fresh) database, resizing the
/// vector column when the model's dimension differs from the default.
pub async fn prepare_model(state: &AppState) -> anyhow::Result<()> {
    let model = catalog::embed_model(&state.config.embed_model)?;
    let dim = i32::try_from(model.dim)?;
    match embeddings::check_model(&state.db, model.name, dim).await? {
        ModelCheck::Matches => Ok(()),
        ModelCheck::Mismatch(_) | ModelCheck::ColumnMismatch { .. } => {
            let mut tx = state.db.begin().await?;
            embeddings::reset(&mut tx, model.name, model.dim).await?;
            tx.commit().await?;
            Ok(())
        }
    }
}

/// Create the eval user, upload every corpus file and run the jobs. Returns the
/// user's id.
pub async fn load(state: &AppState, dir: &Path, names: &[String]) -> anyhow::Result<Uuid> {
    let users::Created::Ok(user) =
        users::create(&state.db, "eval@akasha.invalid", "!", Some("eval")).await?
    else {
        bail!("the eval user already exists; use a fresh database");
    };
    for name in names {
        let bytes = std::fs::read(dir.join(name)).with_context(|| format!("reading {name}"))?;
        let head = &bytes[..bytes.len().min(sniff::SNIFF_LEN)];
        let detected = sniff::detect(head, name).with_context(|| format!("{name}: rejected"))?;
        let mut staged = state.storage.stage().await?;
        staged.write(Bytes::from(bytes)).await?;
        let blob = staged.finish().await?;
        store::save(
            state,
            crate::activity::Actor::session(user.id),
            blob,
            name,
            detected.mime,
        )
        .await
        .map_err(|e| anyhow::anyhow!("{name}: {}", e.0))?;
    }

    // One job at a time: chunk ids (score tie-breakers) come out the same every run.
    let mut config = (*state.config).clone();
    config.worker_concurrency = 1;
    let ran = jobs::worker(JobContext::from(state), &config)?
        .run_until_idle()
        .await?;
    let stats = scratch::library_stats(&state.db, user.id).await?;
    tracing::info!(jobs = ran, ?stats, "corpus loaded");
    if stats.ready != stats.files || stats.embedded != stats.chunks {
        bail!(
            "corpus did not finish processing ({} of {} files ready, {} of {} chunks embedded); \
             see the log for the failing job",
            stats.ready,
            stats.files,
            stats.embedded,
            stats.chunks
        );
    }
    Ok(user.id)
}
