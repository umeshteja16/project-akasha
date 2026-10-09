//! The refusal gate: is the best retrieved passage good enough to answer from?
//!
//! Only a cross-encoder score says how well a passage answers a question (RRF
//! fusion scores are rank-based and look the same for any query), so the gate
//! compares the top passage's reranker score with a threshold per reranker.
//! Without a reranker score (reranking off or unavailable, or a single
//! candidate) the gate lets any result through and leaves the judgement to the
//! model, which is told to answer "not found" when the sources do not help.

use akasha_search::ChunkHit;

/// Minimum top reranker score per model, calibrated on `eval/` questions and
/// off-topic questions (see the `chat_gate` test and ADR 0012).
///
/// - `overlap` (test reranker): the share of the question's content words
///   found in the passage. On the calibration set answerable questions score
///   0.75-1.0 and off-topic ones 0-0.25, so 0.5 sits in the gap.
/// - ONNX cross-encoders return raw logits (positive: relevant, strongly
///   negative: unrelated). -3 is a deliberately lenient, not yet measured
///   default (real models cannot be downloaded in the dev sandbox): it should
///   only refuse clearly unrelated passages. Tune `AKASHA_CHAT_MIN_RERANK_SCORE`
///   with `akasha eval --real-models` data before relying on it.
pub fn default_min_score(reranker: &str) -> Option<f32> {
    match reranker {
        akasha_ml::catalog::OVERLAP_RERANK_MODEL => Some(0.5),
        "jina-reranker-v1-turbo-en" | "bge-reranker-base" | "bge-reranker-v2-m3" => Some(-3.0),
        _ => None,
    }
}

/// Why a question can or cannot be answered from `hits`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Evidence {
    /// Good enough (or nothing to judge it by).
    Sufficient,
    /// Nothing matched at all.
    Nothing,
    /// The best passage scored below the threshold.
    Weak { score: f32, min: f32 },
}

/// Judge `hits` (best first). `reranker` is the reranker's name when one ran;
/// `configured` overrides its default threshold.
pub fn assess(hits: &[ChunkHit], reranker: Option<&str>, configured: Option<f32>) -> Evidence {
    let Some(top) = hits.first() else {
        return Evidence::Nothing;
    };
    let Some(score) = top.chunk.scores.rerank else {
        return Evidence::Sufficient;
    };
    let min = configured.or_else(|| reranker.and_then(default_min_score));
    match min {
        Some(min) if score < min => Evidence::Weak { score, min },
        _ => Evidence::Sufficient,
    }
}
