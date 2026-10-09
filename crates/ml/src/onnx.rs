//! Real models through ONNX Runtime (fastembed). ONNX Runtime itself is a shared
//! library loaded at run time (`ort` load-dynamic, ADR 0009), so building never
//! downloads native code; the Docker image ships the library.

use std::{
    fs,
    path::{Path, PathBuf},
    sync::Mutex,
};

use fastembed::{
    InitOptionsUserDefined, OnnxSource, RerankInitOptionsUserDefined, TextEmbedding, TextRerank,
    TokenizerFiles, UserDefinedEmbeddingModel, UserDefinedRerankingModel,
};

use crate::{
    Embedder, MlError, Reranker,
    catalog::{EmbedModel, ModelFiles, Pooling, RerankModel},
    normalize,
};

/// Texts per ONNX Runtime call. Bounds activation memory: with 512-token inputs a
/// batch of 16 needs well under 1 GB even for base-sized models.
const BATCH: usize = 16;

#[cfg(target_os = "linux")]
const DEFAULT_LIBRARY: &str = "libonnxruntime.so";
#[cfg(target_os = "macos")]
const DEFAULT_LIBRARY: &str = "libonnxruntime.dylib";
#[cfg(target_os = "windows")]
const DEFAULT_LIBRARY: &str = "onnxruntime.dll";
#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
const DEFAULT_LIBRARY: &str = "libonnxruntime.so";

/// The ONNX Runtime library to load: `configured` if set, else `ORT_DYLIB_PATH`,
/// else the platform's library name (found next to the binary or on the loader path).
pub fn runtime_library(configured: &str) -> PathBuf {
    if !configured.trim().is_empty() {
        return PathBuf::from(configured.trim());
    }
    match std::env::var("ORT_DYLIB_PATH") {
        Ok(path) if !path.trim().is_empty() => PathBuf::from(path.trim()),
        _ => PathBuf::from(DEFAULT_LIBRARY),
    }
}

/// Load ONNX Runtime once per process. A failure is not remembered, so a later
/// call (after the library was installed) can still succeed.
pub fn init_runtime(configured: &str) -> Result<(), MlError> {
    let path = runtime_library(configured);
    let fresh = ort::init_from(&path)
        .map_err(|e| {
            MlError::Runtime(format!(
                "{e} (install ONNX Runtime >= 1.24 or set AKASHA_ORT_DYLIB_PATH)"
            ))
        })?
        .commit();
    if fresh {
        tracing::info!(library = %path.display(), "ONNX Runtime loaded");
    }
    Ok(())
}

fn tokenizer_files(dir: &Path) -> Result<TokenizerFiles, MlError> {
    Ok(TokenizerFiles {
        tokenizer_file: read(&dir.join("tokenizer.json"))?,
        config_file: read(&dir.join("config.json"))?,
        special_tokens_map_file: read(&dir.join("special_tokens_map.json"))?,
        tokenizer_config_file: read(&dir.join("tokenizer_config.json"))?,
    })
}

fn read(path: &Path) -> Result<Vec<u8>, MlError> {
    fs::read(path).map_err(|e| MlError::Models(format!("reading {}: {e}", path.display())))
}

fn inference(err: impl std::fmt::Display) -> MlError {
    MlError::Inference(err.to_string())
}

/// A sentence-embedding model. One instance per process: inference is serialised
/// by a mutex (ONNX Runtime already uses every core for a single call).
pub struct OnnxEmbedder {
    model: &'static EmbedModel,
    inner: Mutex<TextEmbedding>,
}

impl OnnxEmbedder {
    /// Load from `dir` (the model's repo directory). `threads == 0`: all cores.
    pub fn load(
        model: &'static EmbedModel,
        files: &ModelFiles,
        pooling: Pooling,
        dir: &Path,
        threads: usize,
    ) -> Result<Self, MlError> {
        let mut user =
            UserDefinedEmbeddingModel::new(read(&dir.join(files.onnx))?, tokenizer_files(dir)?)
                .with_pooling(match pooling {
                    Pooling::Cls => fastembed::Pooling::Cls,
                    Pooling::Mean => fastembed::Pooling::Mean,
                });
        for external in files.external {
            // The graph refers to external weights by file name.
            let name = external.rsplit('/').next().unwrap_or(external);
            user = user.with_external_initializer(name.to_owned(), read(&dir.join(external))?);
        }
        let mut options = InitOptionsUserDefined::new().with_max_length(model.max_tokens);
        if threads > 0 {
            options = options.with_intra_threads(threads);
        }
        let inner = TextEmbedding::try_new_from_user_defined(user, options).map_err(inference)?;
        Ok(Self {
            model,
            inner: Mutex::new(inner),
        })
    }

    fn embed(&self, prefix: &str, texts: &[&str]) -> Result<Vec<Vec<f32>>, MlError> {
        let inputs: Vec<String> = texts.iter().map(|t| format!("{prefix}{t}")).collect();
        let mut vectors = {
            let mut inner = self
                .inner
                .lock()
                .map_err(|_| inference("embedding model is unusable after a panic"))?;
            inner.embed(&inputs, Some(BATCH)).map_err(inference)?
        };
        if vectors.len() != texts.len() {
            return Err(inference(format!(
                "model returned {} vectors for {} texts",
                vectors.len(),
                texts.len()
            )));
        }
        for v in &mut vectors {
            if v.len() != self.model.dim {
                return Err(inference(format!(
                    "model returned {} dimensions, expected {}",
                    v.len(),
                    self.model.dim
                )));
            }
            normalize(v);
        }
        Ok(vectors)
    }
}

impl Embedder for OnnxEmbedder {
    fn model(&self) -> &'static EmbedModel {
        self.model
    }

    fn embed_documents(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>, MlError> {
        self.embed(self.model.document_prefix, texts)
    }

    fn embed_query(&self, text: &str) -> Result<Vec<f32>, MlError> {
        self.embed(self.model.query_prefix, &[text])?
            .pop()
            .ok_or_else(|| inference("model returned no vector"))
    }
}

/// A cross-encoder reranker (same threading rules as [`OnnxEmbedder`]).
pub struct OnnxReranker {
    model: &'static RerankModel,
    inner: Mutex<TextRerank>,
}

impl OnnxReranker {
    pub fn load(
        model: &'static RerankModel,
        files: &ModelFiles,
        dir: &Path,
        threads: usize,
    ) -> Result<Self, MlError> {
        // From a file, so external weight files next to it are found by ONNX Runtime.
        let user = UserDefinedRerankingModel::new(
            OnnxSource::File(dir.join(files.onnx)),
            tokenizer_files(dir)?,
        );
        let mut options = RerankInitOptionsUserDefined::new().with_max_length(model.max_tokens);
        if threads > 0 {
            options = options.with_intra_threads(threads);
        }
        let inner = TextRerank::try_new_from_user_defined(user, options).map_err(inference)?;
        Ok(Self {
            model,
            inner: Mutex::new(inner),
        })
    }
}

impl Reranker for OnnxReranker {
    fn name(&self) -> &'static str {
        self.model.name
    }

    fn score(&self, query: &str, documents: &[&str]) -> Result<Vec<f32>, MlError> {
        if documents.is_empty() {
            return Ok(Vec::new());
        }
        let results = {
            let mut inner = self
                .inner
                .lock()
                .map_err(|_| inference("reranker is unusable after a panic"))?;
            inner
                .rerank(query, documents, false, Some(BATCH))
                .map_err(inference)?
        };
        let mut scores = vec![f32::NEG_INFINITY; documents.len()];
        for r in results {
            if let Some(slot) = scores.get_mut(r.index) {
                *slot = r.score;
            }
        }
        Ok(scores)
    }
}
