//! Refusal-gate calibration (`eval/gate.json`): the top reranker score of
//! questions the corpus answers and of questions it does not, their
//! distributions, how well the current threshold separates them and the
//! threshold that separates them best. Informational: never gated, not part
//! of baselines. Use it to set `AKASHA_CHAT_MIN_RERANK_SCORE` (or the
//! per-reranker default in `chat::evidence`) from real-model runs.

use std::path::Path;

use akasha_db::PgPool;
use akasha_search::{ChunkFilter, Models, SearchMode, SearchRequest};
use anyhow::Context;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Candidates per question, as chat retrieves them.
const RETRIEVE: usize = 20;

/// `eval/gate.json`.
#[derive(Debug, Clone, Deserialize)]
pub struct GateSet {
    /// Questions some corpus file answers.
    pub answerable: Vec<String>,
    /// Questions no corpus file answers (some near a corpus topic).
    pub unanswerable: Vec<String>,
}

impl GateSet {
    /// `None` when the file does not exist.
    pub fn load(path: &Path) -> anyhow::Result<Option<Self>> {
        if !path.exists() {
            return Ok(None);
        }
        let text =
            std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
        let set =
            serde_json::from_str(&text).with_context(|| format!("parsing {}", path.display()))?;
        Ok(Some(set))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GateReport {
    pub reranker: String,
    /// The threshold chat uses with this configuration (`None`: no gate).
    pub threshold: Option<f32>,
    pub answerable: ScoreStats,
    pub unanswerable: ScoreStats,
    /// Share of questions the current threshold classifies correctly.
    pub accuracy_at_threshold: Option<f64>,
    /// The lowest threshold with the best accuracy on this set.
    pub suggested_threshold: Option<f32>,
    pub accuracy_at_suggested: f64,
    pub questions: Vec<GateQuestion>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GateQuestion {
    pub question: String,
    pub answerable: bool,
    /// Top reranker score; `None` when nothing matched.
    pub top_score: Option<f32>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ScoreStats {
    pub count: usize,
    pub min: f32,
    pub p10: f32,
    pub median: f32,
    pub p90: f32,
    pub max: f32,
}

impl ScoreStats {
    fn of(scores: &[f32]) -> Self {
        if scores.is_empty() {
            return Self::default();
        }
        let mut sorted = scores.to_vec();
        sorted.sort_by(f32::total_cmp);
        let at = |p: f32| {
            let rank = ((p / 100.0) * sorted.len() as f32).ceil() as usize;
            sorted[rank.clamp(1, sorted.len()) - 1]
        };
        Self {
            count: sorted.len(),
            min: sorted[0],
            p10: at(10.0),
            median: at(50.0),
            p90: at(90.0),
            max: sorted[sorted.len() - 1],
        }
    }
}

/// Score every gate question against the owner's library.
pub async fn run(
    pool: &PgPool,
    owner: Uuid,
    models: &Models,
    reranker: &str,
    threshold: Option<f32>,
    set: &GateSet,
) -> anyhow::Result<GateReport> {
    let mut questions = Vec::new();
    let all = set
        .answerable
        .iter()
        .map(|q| (q, true))
        .chain(set.unanswerable.iter().map(|q| (q, false)));
    for (question, answerable) in all {
        let req = SearchRequest {
            query: question.clone(),
            mode: SearchMode::Hybrid,
            filter: ChunkFilter::default(),
            limit: RETRIEVE,
            offset: 0,
            rerank: true,
            // Calibrate on the reranker's raw view of the best passage.
            include_weak: true,
        };
        let res = akasha_search::search_chunks(pool, owner, &req, models).await?;
        let top_score = res.results.first().and_then(|h| h.chunk.scores.rerank);
        questions.push(GateQuestion {
            question: question.clone(),
            answerable,
            top_score,
        });
    }
    let scores = |want: bool| -> Vec<f32> {
        questions
            .iter()
            .filter(|q| q.answerable == want)
            .filter_map(|q| q.top_score)
            .collect()
    };
    let (yes, no) = (scores(true), scores(false));
    let (suggested_threshold, accuracy_at_suggested) = best_threshold(&questions);
    Ok(GateReport {
        reranker: reranker.to_owned(),
        threshold,
        answerable: ScoreStats::of(&yes),
        unanswerable: ScoreStats::of(&no),
        accuracy_at_threshold: threshold.map(|t| accuracy(&questions, t)),
        suggested_threshold,
        accuracy_at_suggested,
        questions,
    })
}

/// Share of questions classified correctly: answerable ones must reach `t`,
/// the others must stay below it (or match nothing).
fn accuracy(questions: &[GateQuestion], t: f32) -> f64 {
    if questions.is_empty() {
        return 0.0;
    }
    let right = questions
        .iter()
        .filter(|q| {
            let passes = q.top_score.is_some_and(|s| s >= t);
            passes == q.answerable
        })
        .count();
    right as f64 / questions.len() as f64
}

/// The lowest midpoint between neighbouring scores with the best accuracy.
fn best_threshold(questions: &[GateQuestion]) -> (Option<f32>, f64) {
    let mut scores: Vec<f32> = questions.iter().filter_map(|q| q.top_score).collect();
    scores.sort_by(f32::total_cmp);
    scores.dedup();
    let mut best: (Option<f32>, f64) = (None, 0.0);
    let candidates = scores
        .windows(2)
        .map(|w| (w[0] + w[1]) / 2.0)
        .chain(scores.first().map(|s| s - 0.01));
    let mut candidates: Vec<f32> = candidates.collect();
    candidates.sort_by(f32::total_cmp);
    for t in candidates {
        let acc = accuracy(questions, t);
        if acc > best.1 {
            best = (Some(t), acc);
        }
    }
    best
}

impl GateReport {
    /// Plain-text summary for the console.
    pub fn table(&self) -> String {
        let row = |name: &str, s: &ScoreStats| {
            format!(
                "{name:<14}{:>6}{:>9.3}{:>9.3}{:>9.3}{:>9.3}{:>9.3}\n",
                s.count, s.min, s.p10, s.median, s.p90, s.max
            )
        };
        let mut out = format!(
            "Refusal gate: top {} scores\n\n{:<14}{:>6}{:>9}{:>9}{:>9}{:>9}{:>9}\n",
            self.reranker, "questions", "n", "min", "p10", "median", "p90", "max"
        );
        out += &row("answerable", &self.answerable);
        out += &row("unanswerable", &self.unanswerable);
        let fmt = |t: Option<f32>| t.map_or("none".to_owned(), |t| format!("{t:.3}"));
        out += &format!(
            "\ncurrent threshold {} (accuracy {}); best on this set {} (accuracy {:.3})\n",
            fmt(self.threshold),
            self.accuracy_at_threshold
                .map_or("n/a".to_owned(), |a| format!("{a:.3}")),
            fmt(self.suggested_threshold),
            self.accuracy_at_suggested
        );
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn q(answerable: bool, score: Option<f32>) -> GateQuestion {
        GateQuestion {
            question: String::new(),
            answerable,
            top_score: score,
        }
    }

    #[test]
    fn the_best_threshold_separates_the_groups() {
        let qs = [
            q(true, Some(0.9)),
            q(true, Some(0.7)),
            q(false, Some(0.2)),
            q(false, Some(0.4)),
            q(false, None),
        ];
        let (t, acc) = best_threshold(&qs);
        assert_eq!(acc, 1.0);
        assert_eq!(t, Some(0.55));
        assert_eq!(accuracy(&qs, 0.3), 0.8);
        let stats = ScoreStats::of(&[3.0, 1.0, 2.0]);
        assert_eq!((stats.min, stats.median, stats.max), (1.0, 2.0, 3.0));
    }
}
