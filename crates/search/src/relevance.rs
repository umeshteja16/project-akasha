//! The relevance floor: vector search always returns *some* neighbours, even
//! for a query nothing in the library is about. A result that only the
//! semantic retriever found must clear a threshold to count as a match:
//!
//! - its reranker score, when it was reranked and the reranker has a floor,
//! - otherwise its cosine similarity to the query.
//!
//! Keyword and file-name hits always qualify (the words are really there).
//! Results below the floor are "loosely related": hidden by default, listed
//! after the matches with `include_weak`.

use akasha_ml::catalog;

use crate::{Models, engine::Ranked, types::Scores};

/// Overrides for the per-model defaults ([`catalog::EmbedModel::min_similarity`],
/// [`catalog::RerankModel::min_score`]); `None` keeps the model's default.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct RelevanceFloor {
    /// Cosine similarity a semantic-only result needs.
    pub min_similarity: Option<f32>,
    /// Reranker score a reranked semantic-only result needs.
    pub min_rerank_score: Option<f32>,
}

/// The thresholds in effect for `models`.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub(crate) struct Thresholds {
    pub similarity: Option<f32>,
    pub rerank: Option<f32>,
}

impl Thresholds {
    pub fn for_models(models: &Models) -> Self {
        let similarity = models.floor.min_similarity.or_else(|| {
            models
                .embedder
                .as_ref()
                .ok()
                .map(|e| e.model().min_similarity)
        });
        let rerank = models
            .floor
            .min_rerank_score
            .or_else(|| match &models.reranker {
                Ok(Some(r)) => catalog::rerank_model(r.name())
                    .ok()
                    .flatten()
                    .map(|m| m.min_score),
                _ => None,
            });
        Self { similarity, rerank }
    }

    /// Is this result only loosely related to the query?
    pub fn is_weak(&self, s: &Scores) -> bool {
        if s.keyword_rank.is_some() || s.filename_rank.is_some() {
            return false;
        }
        if let (Some(score), Some(min)) = (s.rerank, self.rerank) {
            return score.is_nan() || score < min;
        }
        match (s.semantic, self.similarity) {
            (Some(sim), Some(min)) => sim < min,
            _ => false,
        }
    }
}

/// Mark loosely related results and move them after the matches (each group
/// keeps its order).
pub(crate) fn apply(models: &Models, ranked: &mut Vec<Ranked>) {
    let t = Thresholds::for_models(models);
    for r in ranked.iter_mut() {
        r.weak = t.is_weak(&r.scores);
    }
    let (strong, weak): (Vec<Ranked>, Vec<Ranked>) = ranked.drain(..).partition(|r| !r.weak);
    ranked.extend(strong);
    ranked.extend(weak);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(similarity: Option<f32>, rerank: Option<f32>) -> Thresholds {
        Thresholds { similarity, rerank }
    }

    fn scores(f: impl FnOnce(&mut Scores)) -> Scores {
        let mut s = Scores::default();
        f(&mut s);
        s
    }

    fn weak(th: Thresholds, s: Scores) -> bool {
        th.is_weak(&s)
    }

    #[test]
    fn keyword_and_file_name_hits_always_qualify() {
        let th = t(Some(0.9), Some(0.9));
        assert!(!weak(
            th,
            scores(|s| {
                s.keyword_rank = Some(3);
                s.semantic = Some(0.01);
                s.rerank = Some(0.0);
            })
        ));
        assert!(!weak(th, scores(|s| s.filename_rank = Some(1))));
    }

    #[test]
    fn semantic_only_hits_use_the_rerank_score_then_similarity() {
        let th = t(Some(0.3), Some(0.5));
        let reranked = |r: f32| {
            scores(|s| {
                s.semantic = Some(0.99);
                s.rerank = Some(r);
            })
        };
        assert!(weak(th, reranked(0.2)), "reranker says unrelated");
        assert!(!weak(th, reranked(0.6)));
        assert!(weak(th, reranked(f32::NAN)));
        assert!(weak(th, scores(|s| s.semantic = Some(0.1))));
        assert!(!weak(th, scores(|s| s.semantic = Some(0.3))));
        // A reranker without a floor falls back to the similarity floor.
        assert!(!weak(t(Some(0.3), None), reranked(-9.0)));
        assert!(weak(
            t(Some(0.3), None),
            scores(|s| {
                s.semantic = Some(0.2);
                s.rerank = Some(9.0);
            })
        ));
        // No floor at all: nothing is weak.
        assert!(!weak(t(None, None), scores(|s| s.semantic = Some(0.0))));
    }
}
