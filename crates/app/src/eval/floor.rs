//! Relevance-floor calibration: the scores of the passages semantic search
//! returns, split by whether their file actually answers the query, so the
//! per-model floors ([`akasha_ml::catalog::EmbedModel::min_similarity`],
//! [`akasha_ml::catalog::RerankModel::min_score`]) can be set from measurement
//! instead of guesswork.
//!
//! The floor only ever judges a passage that *only* vector search found, so
//! this runs in [`SearchMode::Semantic`] with `include_weak`, where every hit
//! is such a passage and nothing is filtered yet. Informational: never gated,
//! not part of baselines.

use std::collections::HashSet;

use akasha_core::Config;
use akasha_db::PgPool;
use akasha_ml::catalog;
use akasha_search::{ChunkFilter, Models, SearchMode, SearchRequest};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::{gate::ScoreStats, suite::Suite};

/// Passages scored per query, as search returns them.
const DEPTH: usize = 20;

/// Which score the floor compares.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Metric {
    /// The reranker's score (a reranker with a floor is configured).
    Rerank,
    /// Cosine similarity to the query.
    Similarity,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FloorReport {
    pub embed_model: String,
    pub metric: Metric,
    /// The floor in effect for this configuration (`None`: no floor).
    pub threshold: Option<f32>,
    /// Scores of passages whose file answers the query.
    pub relevant: ScoreStats,
    /// Scores of passages whose file does not (including every hit for the
    /// off-topic `negative` queries).
    pub irrelevant: ScoreStats,
    /// Share of relevant passages the current floor keeps.
    pub kept_relevant: Option<f64>,
    /// Share of irrelevant passages it sends to "loosely related".
    pub rejected_irrelevant: Option<f64>,
    /// The floor with the best balance of the two above; there are far more
    /// irrelevant passages than relevant ones, so plain accuracy would just
    /// reward rejecting everything.
    pub suggested_threshold: Option<f32>,
    pub kept_relevant_at_suggested: f64,
    pub rejected_irrelevant_at_suggested: f64,
}

/// One scored passage.
struct Hit {
    score: f32,
    relevant: bool,
}

/// Score every query's semantic hits against the files that answer it.
pub async fn run(
    pool: &PgPool,
    owner: Uuid,
    models: &Models,
    config: &Config,
    suite: &Suite,
) -> anyhow::Result<FloorReport> {
    let embed_model = models
        .embedder
        .as_ref()
        .map(|e| e.model().name.to_owned())
        .unwrap_or_else(|_| config.embed_model.clone());
    // Mirror `akasha_search`: a reranker with a floor decides, otherwise
    // similarity does.
    let rerank_floor = config
        .search_min_rerank_score
        .or_else(|| match &models.reranker {
            Ok(Some(r)) => catalog::rerank_model(r.name())
                .ok()
                .flatten()
                .map(|m| m.min_score),
            _ => None,
        });
    let (metric, threshold) = match rerank_floor {
        Some(min) => (Metric::Rerank, Some(min)),
        None => (
            Metric::Similarity,
            config.search_min_similarity.or_else(|| {
                models
                    .embedder
                    .as_ref()
                    .ok()
                    .map(|e| e.model().min_similarity)
            }),
        ),
    };
    let rerank = metric == Metric::Rerank;

    let mut hits = Vec::new();
    for q in &suite.queries {
        let req = SearchRequest {
            query: q.query.clone(),
            mode: SearchMode::Semantic,
            filter: ChunkFilter::default(),
            limit: DEPTH,
            offset: 0,
            rerank,
            // The floor's own view: nothing filtered out yet.
            include_weak: true,
        };
        let res = akasha_search::search_chunks(pool, owner, &req, models).await?;
        let relevant: HashSet<&str> = q.relevant.iter().map(String::as_str).collect();
        for hit in res.results {
            let score = match metric {
                Metric::Rerank => hit.chunk.scores.rerank,
                Metric::Similarity => hit.chunk.scores.semantic,
            };
            // A passage the chosen retriever did not score says nothing about
            // the floor.
            let Some(score) = score.filter(|s| !s.is_nan()) else {
                continue;
            };
            hits.push(Hit {
                score,
                relevant: relevant.contains(hit.file.name.as_str()),
            });
        }
    }

    let scores = |want: bool| -> Vec<f32> {
        hits.iter()
            .filter(|h| h.relevant == want)
            .map(|h| h.score)
            .collect()
    };
    let (yes, no) = (scores(true), scores(false));
    let (suggested_threshold, kept, rejected) = best_threshold(&hits);
    Ok(FloorReport {
        embed_model,
        metric,
        threshold,
        relevant: ScoreStats::of(&yes),
        irrelevant: ScoreStats::of(&no),
        kept_relevant: threshold.map(|t| share(&yes, |s| s >= t)),
        rejected_irrelevant: threshold.map(|t| share(&no, |s| s < t)),
        suggested_threshold,
        kept_relevant_at_suggested: kept,
        rejected_irrelevant_at_suggested: rejected,
    })
}

/// Share of `scores` that `pass` accepts; zero for an empty set.
fn share(scores: &[f32], pass: impl Fn(f32) -> bool) -> f64 {
    if scores.is_empty() {
        return 0.0;
    }
    scores.iter().filter(|s| pass(**s)).count() as f64 / scores.len() as f64
}

/// The floor with the best mean of "relevant kept" and "irrelevant rejected",
/// and those two shares at it. Ties go to the lower floor, which keeps more.
fn best_threshold(hits: &[Hit]) -> (Option<f32>, f64, f64) {
    let mut yes = Vec::new();
    let mut no = Vec::new();
    for hit in hits {
        if hit.relevant {
            yes.push(hit.score);
        } else {
            no.push(hit.score);
        }
    }
    if yes.is_empty() || no.is_empty() {
        return (None, 0.0, 0.0);
    }
    let mut candidates: Vec<f32> = hits.iter().map(|h| h.score).collect();
    candidates.sort_by(f32::total_cmp);
    candidates.dedup();
    let mut best: (Option<f32>, f64, f64) = (None, 0.0, 0.0);
    for t in candidates {
        let kept = share(&yes, |s| s >= t);
        let rejected = share(&no, |s| s < t);
        if (kept + rejected) / 2.0 > (best.1 + best.2) / 2.0 {
            best = (Some(t), kept, rejected);
        }
    }
    best
}

impl FloorReport {
    /// Plain-text summary for the console.
    pub fn table(&self) -> String {
        let metric = match self.metric {
            Metric::Rerank => "rerank score",
            Metric::Similarity => "cosine similarity",
        };
        let row = |name: &str, s: &ScoreStats| {
            format!(
                "{name:<14}{:>6}{:>9.3}{:>9.3}{:>9.3}{:>9.3}{:>9.3}\n",
                s.count, s.min, s.p10, s.median, s.p90, s.max
            )
        };
        let pct = |v: Option<f64>| match v {
            Some(v) => format!("{:.0}%", v * 100.0),
            None => "n/a".to_owned(),
        };
        let mut out = format!(
            "Relevance floor ({} of semantic hits, {})\n\n\
             {:<14}{:>6}{:>9}{:>9}{:>9}{:>9}{:>9}\n",
            metric, self.embed_model, "passages", "n", "min", "p10", "median", "p90", "max"
        );
        out.push_str(&row("relevant", &self.relevant));
        out.push_str(&row("irrelevant", &self.irrelevant));
        match self.threshold {
            Some(t) => out.push_str(&format!(
                "\nfloor in effect {t:.3}: keeps {} of relevant, rejects {} of irrelevant\n",
                pct(self.kept_relevant),
                pct(self.rejected_irrelevant),
            )),
            None => out.push_str("\nno floor in effect\n"),
        }
        match self.suggested_threshold {
            Some(t) => out.push_str(&format!(
                "best balance  {t:.3}: keeps {} of relevant, rejects {} of irrelevant\n",
                pct(Some(self.kept_relevant_at_suggested)),
                pct(Some(self.rejected_irrelevant_at_suggested)),
            )),
            None => out.push_str("not enough labelled hits to suggest a floor\n"),
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hits(relevant: &[f32], irrelevant: &[f32]) -> Vec<Hit> {
        relevant
            .iter()
            .map(|s| Hit {
                score: *s,
                relevant: true,
            })
            .chain(irrelevant.iter().map(|s| Hit {
                score: *s,
                relevant: false,
            }))
            .collect()
    }

    #[test]
    fn suggests_a_floor_between_the_two_groups() {
        let (floor, kept, rejected) = best_threshold(&hits(&[0.9, 0.8, 0.7], &[0.3, 0.2, 0.1]));
        let floor = floor.expect("a floor");
        assert!(floor > 0.3 && floor <= 0.7, "{floor} separates the groups");
        assert_eq!((kept, rejected), (1.0, 1.0));
    }

    #[test]
    fn overlapping_groups_trade_one_against_the_other() {
        // The best split cannot keep everything: 0.5 is in both groups.
        let (floor, kept, rejected) = best_threshold(&hits(&[0.9, 0.5], &[0.5, 0.1]));
        assert_eq!(floor, Some(0.5));
        assert_eq!((kept, rejected), (1.0, 0.5));
    }

    #[test]
    fn a_floor_needs_both_kinds_of_hit() {
        assert_eq!(best_threshold(&hits(&[0.9], &[])), (None, 0.0, 0.0));
        assert_eq!(best_threshold(&hits(&[], &[0.1])), (None, 0.0, 0.0));
        assert_eq!(best_threshold(&[]), (None, 0.0, 0.0));
    }

    #[test]
    fn negative_scores_work_too() {
        // ONNX rerankers score on a logit scale, well below zero.
        let (floor, kept, rejected) = best_threshold(&hits(&[1.5, -0.5], &[-4.0, -9.0]));
        let floor = floor.expect("a floor");
        assert!(floor > -4.0 && floor <= -0.5, "{floor}");
        assert_eq!((kept, rejected), (1.0, 1.0));
    }

    #[test]
    fn share_counts_what_passes() {
        assert_eq!(share(&[1.0, 2.0, 3.0, 4.0], |s| s >= 3.0), 0.5);
        assert_eq!(share(&[], |_| true), 0.0);
    }
}
