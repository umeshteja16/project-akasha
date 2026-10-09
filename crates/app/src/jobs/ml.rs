//! The process-wide embedding model and reranker, loaded (and their files
//! downloaded) on first use and shared by the worker and, later, search.

use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
    time::Duration,
};

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
    /// Why the last load failed (cleared by a successful load).
    embedder_error: Mutex<Option<String>>,
    reranker_error: Mutex<Option<String>>,
}

/// Where a model stands, without loading it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModelState {
    /// Loaded and in use.
    Ready,
    /// Not loaded yet (models load on first use).
    NotLoaded,
    /// The last load failed (why).
    Failed(String),
}

impl MlProvider {
    pub fn from_config(config: &Config) -> Self {
        Self {
            embed_model: config.embed_model.clone(),
            rerank_model: config.rerank_model.clone(),
            options: ml_options(config),
            embedder: OnceCell::new(),
            reranker: OnceCell::new(),
            embedder_error: Mutex::new(None),
            reranker_error: Mutex::new(None),
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
            .await;
        remember(&self.embedder_error, embedder.as_ref().err());
        Ok(Arc::clone(embedder?))
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
            .await;
        remember(&self.reranker_error, reranker.as_ref().err());
        Ok(reranker?.clone())
    }
}

impl MlProvider {
    /// The embedding model's state (never loads it).
    pub fn embedder_state(&self) -> ModelState {
        state(self.embedder.initialized(), &self.embedder_error)
    }

    /// The reranker's state (never loads it).
    pub fn reranker_state(&self) -> ModelState {
        state(self.reranker.initialized(), &self.reranker_error)
    }

    /// The embedder if it is loaded, or loads within `wait`; `Ok(None)` while it is
    /// still loading (the load carries on in the background, so a request never
    /// waits for a model download).
    pub async fn embedder_within(
        self: &Arc<Self>,
        wait: Duration,
    ) -> Result<Option<Arc<dyn Embedder>>, MlError> {
        if let Some(e) = self.embedder.get() {
            return Ok(Some(Arc::clone(e)));
        }
        let me = Arc::clone(self);
        within(wait, tokio::spawn(async move { me.embedder().await })).await
    }

    /// Like [`Self::embedder_within`] for the reranker (`Ok(Some(None))`: disabled).
    pub async fn reranker_within(
        self: &Arc<Self>,
        wait: Duration,
    ) -> Result<Option<Option<Arc<dyn Reranker>>>, MlError> {
        if let Some(r) = self.reranker.get() {
            return Ok(Some(r.clone()));
        }
        let me = Arc::clone(self);
        within(wait, tokio::spawn(async move { me.reranker().await })).await
    }
}

fn remember(slot: &Mutex<Option<String>>, err: Option<&MlError>) {
    if let Ok(mut slot) = slot.lock() {
        *slot = err.map(ToString::to_string);
    }
}

fn state(loaded: bool, error: &Mutex<Option<String>>) -> ModelState {
    if loaded {
        return ModelState::Ready;
    }
    match error.lock().ok().and_then(|e| e.clone()) {
        Some(err) => ModelState::Failed(err),
        None => ModelState::NotLoaded,
    }
}

async fn within<T>(
    wait: Duration,
    task: tokio::task::JoinHandle<Result<T, MlError>>,
) -> Result<Option<T>, MlError> {
    match tokio::time::timeout(wait, task).await {
        Ok(Ok(loaded)) => loaded.map(Some),
        Ok(Err(err)) => Err(MlError::Inference(format!(
            "loading the model failed: {err}"
        ))),
        Err(_elapsed) => Ok(None),
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
