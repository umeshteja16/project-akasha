//! The process-wide embedding model and reranker, loaded (and their files
//! downloaded) on first use and shared by the worker and, later, search.

use std::{path::PathBuf, sync::Arc};

use akasha_core::Config;
use akasha_ml::{Embedder, MlError, MlOptions, Reranker};
use tokio::sync::OnceCell;

/// Hands out the configured models. Each loads once; a failed load (no network
/// for the first download, ONNX Runtime missing) is retried on the next call.
pub struct MlProvider {
    embed_model: String,
    rerank_model: String,
    options: MlOptions,
    embedder: OnceCell<Arc<dyn Embedder>>,
    reranker: OnceCell<Option<Arc<dyn Reranker>>>,
}

impl MlProvider {
    pub fn from_config(config: &Config) -> Self {
        Self {
            embed_model: config.embed_model.clone(),
            rerank_model: config.rerank_model.clone(),
            options: ml_options(config),
            embedder: OnceCell::new(),
            reranker: OnceCell::new(),
        }
    }

    /// The embedding model.
    pub async fn embedder(&self) -> Result<Arc<dyn Embedder>, MlError> {
        let embedder = self
            .embedder
            .get_or_try_init(|| async {
                let name = self.embed_model.clone();
                let options = self.options.clone();
                tokio::task::spawn_blocking(move || akasha_ml::load_embedder(&name, &options))
                    .await
                    .map_err(|e| MlError::Inference(format!("loading the model panicked: {e}")))?
            })
            .await?;
        Ok(Arc::clone(embedder))
    }

    /// The reranker; `None` when reranking is disabled.
    pub async fn reranker(&self) -> Result<Option<Arc<dyn Reranker>>, MlError> {
        let reranker = self
            .reranker
            .get_or_try_init(|| async {
                let name = self.rerank_model.clone();
                let options = self.options.clone();
                tokio::task::spawn_blocking(move || akasha_ml::load_reranker(&name, &options))
                    .await
                    .map_err(|e| MlError::Inference(format!("loading the model panicked: {e}")))?
            })
            .await?;
        Ok(reranker.clone())
    }
}

/// Model locations and runtime settings from the configuration.
pub fn ml_options(config: &Config) -> MlOptions {
    MlOptions {
        models_dir: PathBuf::from(&config.models_dir),
        models_url: config.ml_models_url.clone(),
        ort_library: config.ort_dylib_path.clone(),
        threads: usize::try_from(config.ml_threads).unwrap_or(0),
    }
}
