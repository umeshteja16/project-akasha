//! The models Akasha knows how to run, by the name used in configuration
//! (`AKASHA_EMBED_MODEL`, `AKASHA_RERANK_MODEL`). See ADR 0009 for the defaults.

use crate::MlError;

/// The embedding model used when none is configured: multilingual, 384
/// dimensions, fast on a CPU.
pub const DEFAULT_EMBED_MODEL: &str = "multilingual-e5-small";
/// The reranker used when none is configured: small and fast (English).
pub const DEFAULT_RERANK_MODEL: &str = "jina-reranker-v1-turbo-en";
/// Deterministic hashed bag-of-words "model" for tests and model-less dev setups.
/// No semantic quality: never use it for a real library.
pub const HASH_EMBED_MODEL: &str = "hash-384";
/// Word-overlap "reranker" for tests.
pub const OVERLAP_RERANK_MODEL: &str = "overlap";

/// Tokenizer files every Hugging Face model repo has at its root.
pub const TOKENIZER_FILES: [&str; 4] = [
    "tokenizer.json",
    "config.json",
    "special_tokens_map.json",
    "tokenizer_config.json",
];

/// How token vectors become one sentence vector.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pooling {
    Cls,
    Mean,
}

/// Where a model's files live: a Hugging Face repo, the ONNX graph inside it and
/// any external weight files the graph references.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModelFiles {
    pub repo: &'static str,
    pub onnx: &'static str,
    pub external: &'static [&'static str],
}

impl ModelFiles {
    /// Every file to fetch, relative to the repo root.
    pub fn all(&self) -> Vec<&'static str> {
        let mut files = vec![self.onnx];
        files.extend_from_slice(self.external);
        files.extend_from_slice(&TOKENIZER_FILES);
        files
    }
}

/// How an embedding model is run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EmbedBackend {
    Onnx { files: ModelFiles, pooling: Pooling },
    Hash,
}

/// An embedding model.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EmbedModel {
    pub name: &'static str,
    pub dim: usize,
    /// Prepended to search queries (asymmetric models are trained with these).
    pub query_prefix: &'static str,
    /// Prepended to indexed passages.
    pub document_prefix: &'static str,
    /// Longer inputs are truncated to this many tokens.
    pub max_tokens: usize,
    /// Relevance floor for search: a passage found *only* by vector similarity
    /// is "loosely related" below this cosine similarity (models spread scores
    /// very differently, so it is per model; `AKASHA_SEARCH_MIN_SIMILARITY`
    /// overrides it).
    pub min_similarity: f32,
    pub backend: EmbedBackend,
}

/// How a reranker is run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RerankBackend {
    Onnx(ModelFiles),
    Overlap,
}

/// A cross-encoder reranker.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RerankModel {
    pub name: &'static str,
    pub max_tokens: usize,
    /// Relevance floor for search: a passage found only by vector similarity
    /// is "loosely related" below this score (model specific scale;
    /// `AKASHA_SEARCH_MIN_RERANK_SCORE` overrides it).
    pub min_score: f32,
    pub backend: RerankBackend,
}

const fn onnx(repo: &'static str, onnx: &'static str, pooling: Pooling) -> EmbedBackend {
    EmbedBackend::Onnx {
        files: ModelFiles {
            repo,
            onnx,
            external: &[],
        },
        pooling,
    }
}

/// Supported embedding models. Changing models later means re-embedding everything
/// (`akasha reembed`), so pick deliberately.
pub const EMBED_MODELS: &[EmbedModel] = &[
    EmbedModel {
        name: "multilingual-e5-small",
        dim: 384,
        query_prefix: "query: ",
        document_prefix: "passage: ",
        max_tokens: 512,
        min_similarity: 0.80,
        backend: onnx(
            "intfloat/multilingual-e5-small",
            "onnx/model.onnx",
            Pooling::Mean,
        ),
    },
    EmbedModel {
        name: "bge-small-en-v1.5",
        dim: 384,
        query_prefix: "Represent this sentence for searching relevant passages: ",
        document_prefix: "",
        max_tokens: 512,
        min_similarity: 0.55,
        backend: onnx("Xenova/bge-small-en-v1.5", "onnx/model.onnx", Pooling::Cls),
    },
    EmbedModel {
        name: "bge-base-en-v1.5",
        dim: 768,
        query_prefix: "Represent this sentence for searching relevant passages: ",
        document_prefix: "",
        max_tokens: 512,
        min_similarity: 0.55,
        backend: onnx("Xenova/bge-base-en-v1.5", "onnx/model.onnx", Pooling::Cls),
    },
    EmbedModel {
        name: "nomic-embed-text-v1.5",
        dim: 768,
        query_prefix: "search_query: ",
        document_prefix: "search_document: ",
        max_tokens: 512,
        min_similarity: 0.45,
        backend: onnx(
            "nomic-ai/nomic-embed-text-v1.5",
            "onnx/model.onnx",
            Pooling::Mean,
        ),
    },
    EmbedModel {
        name: "bge-m3",
        dim: 1024,
        query_prefix: "",
        document_prefix: "",
        max_tokens: 512,
        min_similarity: 0.45,
        backend: EmbedBackend::Onnx {
            files: ModelFiles {
                repo: "BAAI/bge-m3",
                onnx: "onnx/model.onnx",
                external: &["onnx/model.onnx_data", "onnx/Constant_7_attr__value"],
            },
            pooling: Pooling::Cls,
        },
    },
    EmbedModel {
        name: "all-minilm-l6-v2",
        dim: 384,
        query_prefix: "",
        document_prefix: "",
        max_tokens: 256,
        min_similarity: 0.25,
        backend: onnx("Qdrant/all-MiniLM-L6-v2-onnx", "model.onnx", Pooling::Mean),
    },
    EmbedModel {
        name: HASH_EMBED_MODEL,
        dim: 384,
        query_prefix: "",
        document_prefix: "",
        max_tokens: 0,
        min_similarity: 0.15,
        backend: EmbedBackend::Hash,
    },
];

const fn rerank(
    repo: &'static str,
    onnx: &'static str,
    external: &'static [&'static str],
) -> RerankBackend {
    RerankBackend::Onnx(ModelFiles {
        repo,
        onnx,
        external,
    })
}

/// Supported rerankers.
pub const RERANK_MODELS: &[RerankModel] = &[
    RerankModel {
        name: "jina-reranker-v1-turbo-en",
        max_tokens: 512,
        min_score: -2.0,
        backend: rerank("jinaai/jina-reranker-v1-turbo-en", "onnx/model.onnx", &[]),
    },
    RerankModel {
        name: "bge-reranker-base",
        max_tokens: 512,
        min_score: -2.0,
        backend: rerank("BAAI/bge-reranker-base", "onnx/model.onnx", &[]),
    },
    RerankModel {
        name: "bge-reranker-v2-m3",
        max_tokens: 512,
        min_score: -2.0,
        backend: rerank(
            "rozgo/bge-reranker-v2-m3",
            "model.onnx",
            &["model.onnx.data"],
        ),
    },
    RerankModel {
        name: OVERLAP_RERANK_MODEL,
        max_tokens: 0,
        min_score: 0.25,
        backend: RerankBackend::Overlap,
    },
];

/// The embedding model called `name` (case-insensitive).
pub fn embed_model(name: &str) -> Result<&'static EmbedModel, MlError> {
    EMBED_MODELS
        .iter()
        .find(|m| m.name.eq_ignore_ascii_case(name.trim()))
        .ok_or_else(|| MlError::UnknownModel {
            kind: "embedding",
            name: name.to_owned(),
            supported: names(EMBED_MODELS.iter().map(|m| m.name)),
        })
}

/// The reranker called `name`; `None` when reranking is disabled (`""` or `none`).
pub fn rerank_model(name: &str) -> Result<Option<&'static RerankModel>, MlError> {
    let name = name.trim();
    if name.is_empty() || name.eq_ignore_ascii_case("none") {
        return Ok(None);
    }
    RERANK_MODELS
        .iter()
        .find(|m| m.name.eq_ignore_ascii_case(name))
        .map(Some)
        .ok_or_else(|| MlError::UnknownModel {
            kind: "rerank",
            name: name.to_owned(),
            supported: format!("{}, none", names(RERANK_MODELS.iter().map(|m| m.name))),
        })
}

fn names<'a>(it: impl Iterator<Item = &'a str>) -> String {
    it.collect::<Vec<_>>().join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lookups_are_case_insensitive_and_unknown_names_list_the_options() {
        assert_eq!(
            embed_model("Multilingual-E5-Small").expect("known").dim,
            384
        );
        let err = embed_model("gpt-embed").expect_err("unknown");
        assert!(err.is_permanent());
        assert!(err.to_string().contains(DEFAULT_EMBED_MODEL));
        assert!(rerank_model("none").expect("disabled").is_none());
        assert!(rerank_model("").expect("disabled").is_none());
        assert!(rerank_model(DEFAULT_RERANK_MODEL).expect("known").is_some());
        assert!(rerank_model("nope").is_err());
    }

    #[test]
    fn defaults_exist_and_names_are_unique() {
        embed_model(DEFAULT_EMBED_MODEL).expect("default embed model");
        rerank_model(DEFAULT_RERANK_MODEL).expect("default reranker");
        let mut seen: Vec<_> = EMBED_MODELS.iter().map(|m| m.name).collect();
        seen.sort_unstable();
        seen.dedup();
        assert_eq!(seen.len(), EMBED_MODELS.len());
    }

    #[test]
    fn model_files_include_external_weights_and_tokenizer() {
        let m3 = embed_model("bge-m3").expect("known");
        let EmbedBackend::Onnx { files, .. } = m3.backend else {
            panic!("onnx model");
        };
        let all = files.all();
        assert_eq!(all[0], "onnx/model.onnx");
        assert!(all.contains(&"onnx/model.onnx_data"));
        assert!(all.contains(&"tokenizer.json"));
    }
}
