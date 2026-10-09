//! Embeddings and reranking, in process (ADR 0003, ADR 0009).
//!
//! [`Embedder`] and [`Reranker`] are blocking traits: call them from a blocking
//! thread (`tokio::task::spawn_blocking`). Load each model once per process with
//! [`load_embedder`] / [`load_reranker`] and share the `Arc`.
//!
//! Real models run on ONNX Runtime through `fastembed` (feature `onnx`, on by
//! default). Their files are fetched into the models directory on first use
//! ([`download`]). The `hash-384` embedder and `overlap` reranker ([`fake`]) need
//! nothing and are deterministic: tests use them.

pub mod catalog;
pub mod download;
mod error;
pub mod fake;
#[cfg(feature = "onnx")]
pub mod onnx;

use std::{path::PathBuf, sync::Arc};

pub use catalog::{EmbedModel, RerankModel};
pub use error::MlError;

use catalog::{EmbedBackend, RerankBackend};

/// Turns text into vectors. Vectors are L2-normalised and `model().dim` long.
pub trait Embedder: Send + Sync {
    fn model(&self) -> &'static EmbedModel;

    /// Embed passages for indexing (adds the model's document prefix). Blocking.
    fn embed_documents(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>, MlError>;

    /// Embed a search query (adds the model's query prefix). Blocking.
    fn embed_query(&self, text: &str) -> Result<Vec<f32>, MlError>;
}

/// Scores how well each document answers a query (higher is better).
pub trait Reranker: Send + Sync {
    fn name(&self) -> &'static str;

    /// One score per document, in input order. Blocking.
    fn score(&self, query: &str, documents: &[&str]) -> Result<Vec<f32>, MlError>;
}

/// Where models come from and how they run.
#[derive(Debug, Clone)]
pub struct MlOptions {
    /// Model files live under `<models_dir>/hf/`.
    pub models_dir: PathBuf,
    /// Hugging Face compatible endpoint; empty: never download.
    pub models_url: String,
    /// ONNX Runtime shared library; empty: `ORT_DYLIB_PATH` or the default name.
    pub ort_library: String,
    /// Inference threads per model; 0: all cores.
    pub threads: usize,
}

/// Fetch the files of `model` (no-op for built-in models). Blocking.
pub fn download_embedder(model: &EmbedModel, options: &MlOptions) -> Result<(), MlError> {
    if let EmbedBackend::Onnx { files, .. } = &model.backend {
        download::ensure(&options.models_dir, &options.models_url, files)?;
    }
    Ok(())
}

/// Fetch the files of `model` (no-op for built-in models). Blocking.
pub fn download_reranker(model: &RerankModel, options: &MlOptions) -> Result<(), MlError> {
    if let RerankBackend::Onnx(files) = &model.backend {
        download::ensure(&options.models_dir, &options.models_url, files)?;
    }
    Ok(())
}

/// Load the embedding model called `name`, downloading its files if needed.
/// Blocking and slow (seconds): do it once and share the result.
pub fn load_embedder(name: &str, options: &MlOptions) -> Result<Arc<dyn Embedder>, MlError> {
    let model = catalog::embed_model(name)?;
    match &model.backend {
        EmbedBackend::Hash => Ok(Arc::new(fake::HashEmbedder::new(model))),
        #[cfg(feature = "onnx")]
        EmbedBackend::Onnx { files, pooling } => {
            let dir = download::ensure(&options.models_dir, &options.models_url, files)?;
            onnx::init_runtime(&options.ort_library)?;
            let embedder = onnx::OnnxEmbedder::load(model, files, *pooling, &dir, options.threads)?;
            tracing::info!(
                model = model.name,
                dim = model.dim,
                "embedding model loaded"
            );
            Ok(Arc::new(embedder))
        }
        #[cfg(not(feature = "onnx"))]
        EmbedBackend::Onnx { .. } => Err(no_onnx(model.name, options)),
    }
}

/// Load the reranker called `name`; `None` when reranking is disabled.
pub fn load_reranker(
    name: &str,
    options: &MlOptions,
) -> Result<Option<Arc<dyn Reranker>>, MlError> {
    let Some(model) = catalog::rerank_model(name)? else {
        return Ok(None);
    };
    match &model.backend {
        RerankBackend::Overlap => Ok(Some(Arc::new(fake::OverlapReranker))),
        #[cfg(feature = "onnx")]
        RerankBackend::Onnx(files) => {
            let dir = download::ensure(&options.models_dir, &options.models_url, files)?;
            onnx::init_runtime(&options.ort_library)?;
            let reranker = onnx::OnnxReranker::load(model, files, &dir, options.threads)?;
            tracing::info!(model = model.name, "reranker loaded");
            Ok(Some(Arc::new(reranker)))
        }
        #[cfg(not(feature = "onnx"))]
        RerankBackend::Onnx(_) => Err(no_onnx(model.name, options)),
    }
}

#[cfg(not(feature = "onnx"))]
fn no_onnx(name: &str, _options: &MlOptions) -> MlError {
    MlError::Runtime(format!(
        "`{name}` needs ONNX Runtime, but this build has no `onnx` feature"
    ))
}

/// Scale `v` to unit length (left alone if it is all zeros).
pub fn normalize(v: &mut [f32]) {
    let norm = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm > 0.0 {
        v.iter_mut().for_each(|x| *x /= norm);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn options(dir: &std::path::Path) -> MlOptions {
        MlOptions {
            models_dir: dir.to_path_buf(),
            models_url: String::new(),
            ort_library: String::new(),
            threads: 1,
        }
    }

    #[test]
    fn built_in_models_load_without_files() {
        let dir = tempfile::tempdir().expect("tempdir");
        let e = load_embedder(catalog::HASH_EMBED_MODEL, &options(dir.path())).expect("hash");
        assert_eq!(e.model().dim, 384);
        let r = load_reranker("overlap", &options(dir.path())).expect("overlap");
        assert_eq!(r.expect("enabled").name(), "overlap");
        assert!(
            load_reranker("none", &options(dir.path()))
                .expect("off")
                .is_none()
        );
    }

    #[test]
    fn real_models_without_files_fail_retryably_when_offline() {
        let dir = tempfile::tempdir().expect("tempdir");
        let err = match load_embedder(catalog::DEFAULT_EMBED_MODEL, &options(dir.path())) {
            Ok(_) => panic!("no files, no downloads"),
            Err(err) => err,
        };
        assert!(!err.is_permanent(), "{err}");
    }
}
