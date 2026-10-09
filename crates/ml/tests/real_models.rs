//! Real models through ONNX Runtime. Ignored by default (downloads hundreds of MB
//! and needs the ONNX Runtime library):
//!
//! ```sh
//! ORT_DYLIB_PATH=/path/to/libonnxruntime.so \
//!   cargo test -p akasha-ml --test real_models -- --ignored
//! ```
//!
//! `AKASHA_TEST_EMBED_MODEL` / `AKASHA_TEST_RERANK_MODEL` pick the models (default:
//! the production defaults), `AKASHA_TEST_MODELS_DIR` the cache (default
//! `target/ml-models`), `AKASHA_TEST_MODELS_URL` the endpoint (empty: offline).
#![cfg(feature = "onnx")]

use std::path::PathBuf;

use akasha_ml::{MlOptions, catalog, download, load_embedder, load_reranker};

fn options() -> MlOptions {
    let dir = std::env::var("AKASHA_TEST_MODELS_DIR").map_or_else(
        |_| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/ml-models"),
        PathBuf::from,
    );
    MlOptions {
        models_dir: dir,
        models_url: std::env::var("AKASHA_TEST_MODELS_URL")
            .unwrap_or_else(|_| download::DEFAULT_BASE_URL.to_owned()),
        ort_library: String::new(),
        threads: 0,
    }
}

fn env_or(name: &str, default: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| default.to_owned())
}

fn dot(a: &[f32], b: &[f32]) -> f32 {
    a.iter().zip(b).map(|(x, y)| x * y).sum()
}

#[test]
#[ignore = "downloads a model and needs ONNX Runtime"]
fn embedder_ranks_the_relevant_passage_first() {
    let name = env_or("AKASHA_TEST_EMBED_MODEL", catalog::DEFAULT_EMBED_MODEL);
    let embedder = load_embedder(&name, &options()).expect("load embedder");
    let docs = [
        "The aardvark is a nocturnal mammal that digs for ants and termites.",
        "Quarterly tax returns must be filed before the end of the month.",
        "Pasta should be cooked in plenty of salted boiling water.",
    ];
    let started = std::time::Instant::now();
    let vectors = embedder.embed_documents(&docs).expect("embed");
    eprintln!(
        "{name}: embedded {} passages in {:?}",
        docs.len(),
        started.elapsed()
    );
    let query = embedder
        .embed_query("what does the aardvark eat?")
        .expect("query");
    assert_eq!(vectors.len(), 3);
    assert!(vectors.iter().all(|v| v.len() == embedder.model().dim));
    assert!((dot(&query, &query) - 1.0).abs() < 1e-3, "normalised");
    let scores: Vec<f32> = vectors.iter().map(|v| dot(&query, v)).collect();
    eprintln!("{name}: cosine scores {scores:?}");
    assert!(scores[0] > scores[1] && scores[0] > scores[2]);
}

#[test]
#[ignore = "downloads a model and needs ONNX Runtime"]
fn reranker_scores_the_relevant_passage_highest() {
    let name = env_or("AKASHA_TEST_RERANK_MODEL", catalog::DEFAULT_RERANK_MODEL);
    let reranker = load_reranker(&name, &options())
        .expect("load reranker")
        .expect("enabled");
    let scores = reranker
        .score(
            "what does the aardvark eat?",
            &[
                "Quarterly tax returns are due soon.",
                "Aardvarks feed almost exclusively on ants and termites.",
            ],
        )
        .expect("score");
    eprintln!("{name}: rerank scores {scores:?}");
    assert!(scores[1] > scores[0]);
}
