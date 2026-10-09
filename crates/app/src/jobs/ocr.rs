//! The process-wide OCR engine, loaded (and its models downloaded) on first use.

use std::{path::PathBuf, sync::Arc};

use akasha_core::Config;
use akasha_ingest::{IngestError, Ocr, models};
use tokio::sync::OnceCell;

/// Hands out the OCR engine when OCR is enabled. Loading happens once; a failed
/// load (e.g. no network for the first model download) is retried on the next call.
pub struct OcrProvider {
    enabled: bool,
    dir: PathBuf,
    base_url: String,
    engine: OnceCell<Arc<Ocr>>,
}

impl OcrProvider {
    pub fn from_config(config: &Config) -> Self {
        Self {
            enabled: config.ocr_enabled,
            dir: PathBuf::from(&config.models_dir),
            base_url: config.ocr_models_url.clone(),
            engine: OnceCell::new(),
        }
    }

    /// `None` when OCR is disabled.
    pub async fn get(&self) -> Result<Option<Arc<Ocr>>, IngestError> {
        if !self.enabled {
            return Ok(None);
        }
        let engine = self
            .engine
            .get_or_try_init(|| async {
                let dir = self.dir.clone();
                let base_url = self.base_url.clone();
                let loaded = tokio::task::spawn_blocking(move || models::load_ocr(&dir, &base_url))
                    .await
                    .map_err(|e| IngestError::Models(e.to_string()))??;
                tracing::info!(dir = %self.dir.display(), "OCR engine loaded");
                Ok::<_, IngestError>(Arc::new(loaded))
            })
            .await?;
        Ok(Some(Arc::clone(engine)))
    }
}
