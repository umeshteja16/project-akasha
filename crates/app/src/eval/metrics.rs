//! Ranking metrics with binary relevance, over file-level results.
//!
//! - **Recall@k**: share of the relevant files among the first k results.
//! - **MRR**: 1 / rank of the first relevant result (0 if none in the top 10).
//! - **nDCG@10**: discounted cumulative gain of the top 10 (gain 1 per relevant
//!   file at rank i, discounted by log2(i + 1)), divided by the best possible.
//! - **Precision@10**: share of the (up to 10) returned files that are relevant;
//!   0 when nothing came back. Search hides loosely related results, so noise
//!   (unrelated files listed after the answer) lowers it.

use serde::{Deserialize, Serialize};

/// Results considered per query.
pub const DEPTH: usize = 10;

/// Quality of one ranking, or the mean over several.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct Metrics {
    pub recall_at_1: f64,
    pub recall_at_5: f64,
    pub recall_at_10: f64,
    pub mrr: f64,
    pub ndcg_at_10: f64,
    #[serde(default)]
    pub precision_at_10: f64,
}

impl Metrics {
    /// Scores `ranked` (file names, best first) against `relevant`.
    pub fn score(ranked: &[String], relevant: &[String]) -> Self {
        let hits: Vec<bool> = ranked
            .iter()
            .take(DEPTH)
            .map(|name| relevant.contains(name))
            .collect();
        let total = relevant.len().max(1) as f64;
        let recall = |k: usize| hits.iter().take(k).filter(|h| **h).count() as f64 / total;
        let mrr = hits
            .iter()
            .position(|h| *h)
            .map_or(0.0, |i| 1.0 / (i + 1) as f64);
        let gain = |i: usize| 1.0 / ((i + 2) as f64).log2();
        let dcg: f64 = hits
            .iter()
            .enumerate()
            .filter(|(_, h)| **h)
            .map(|(i, _)| gain(i))
            .sum();
        let ideal: f64 = (0..relevant.len().min(DEPTH)).map(gain).sum();
        let precision = if hits.is_empty() {
            0.0
        } else {
            hits.iter().filter(|h| **h).count() as f64 / hits.len() as f64
        };
        Self {
            recall_at_1: recall(1),
            recall_at_5: recall(5),
            recall_at_10: recall(DEPTH),
            mrr,
            ndcg_at_10: if ideal > 0.0 { dcg / ideal } else { 0.0 },
            precision_at_10: precision,
        }
    }

    /// The mean of each metric, rounded to 4 decimals (stable JSON diffs).
    pub fn mean(all: &[Self]) -> Self {
        let n = all.len().max(1) as f64;
        let avg = |f: fn(&Self) -> f64| round(all.iter().map(f).sum::<f64>() / n);
        Self {
            recall_at_1: avg(|m| m.recall_at_1),
            recall_at_5: avg(|m| m.recall_at_5),
            recall_at_10: avg(|m| m.recall_at_10),
            mrr: avg(|m| m.mrr),
            ndcg_at_10: avg(|m| m.ndcg_at_10),
            precision_at_10: avg(|m| m.precision_at_10),
        }
    }

    /// `(name, value)` pairs, for comparisons and tables.
    pub fn named(&self) -> [(&'static str, f64); 6] {
        [
            ("recall@1", self.recall_at_1),
            ("recall@5", self.recall_at_5),
            ("recall@10", self.recall_at_10),
            ("mrr", self.mrr),
            ("ndcg@10", self.ndcg_at_10),
            ("precision@10", self.precision_at_10),
        ]
    }
}

pub fn round(x: f64) -> f64 {
    (x * 10_000.0).round() / 10_000.0
}

/// The `p`-th percentile (0..=100, nearest rank) of `values`.
pub fn percentile(values: &[f64], p: f64) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    let mut sorted = values.to_vec();
    sorted.sort_by(f64::total_cmp);
    let rank = ((p / 100.0) * sorted.len() as f64).ceil() as usize;
    sorted[rank.clamp(1, sorted.len()) - 1]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| (*s).to_owned()).collect()
    }

    #[test]
    fn perfect_and_missing_rankings() {
        let rel = names(&["a"]);
        let best = Metrics::score(&names(&["a", "b"]), &rel);
        assert_eq!(best.mrr, 1.0);
        assert_eq!(best.ndcg_at_10, 1.0);
        assert_eq!(best.recall_at_1, 1.0);
        assert_eq!(best.precision_at_10, 0.5);
        let none = Metrics::score(&names(&["b", "c"]), &rel);
        assert_eq!(none, Metrics::default());
    }

    #[test]
    fn ranks_and_multiple_relevant_files() {
        let rel = names(&["a", "b"]);
        let m = Metrics::score(&names(&["x", "a", "y", "z", "w", "b"]), &rel);
        assert_eq!(m.mrr, 0.5);
        assert_eq!(m.recall_at_1, 0.0);
        assert_eq!(m.recall_at_5, 0.5);
        assert_eq!(m.recall_at_10, 1.0);
        assert!((m.precision_at_10 - 2.0 / 6.0).abs() < 1e-12);
        assert_eq!(Metrics::score(&[], &rel).precision_at_10, 0.0);
        let dcg = 1.0 / 3f64.log2() + 1.0 / 7f64.log2();
        let ideal = 1.0 + 1.0 / 3f64.log2();
        assert!((m.ndcg_at_10 - dcg / ideal).abs() < 1e-12);
        // Beyond the depth nothing counts.
        let mut deep = names(&["x"; 10]);
        deep.push("a".into());
        assert_eq!(Metrics::score(&deep, &rel).mrr, 0.0);
    }

    #[test]
    fn means_and_percentiles() {
        let a = Metrics {
            mrr: 1.0,
            ..Metrics::default()
        };
        let b = Metrics {
            mrr: 1.0 / 3.0,
            ..Metrics::default()
        };
        assert_eq!(Metrics::mean(&[a, b]).mrr, 0.6667);
        let v = [5.0, 1.0, 3.0, 2.0, 4.0];
        assert_eq!(percentile(&v, 50.0), 3.0);
        assert_eq!(percentile(&v, 95.0), 5.0);
        assert_eq!(percentile(&[], 50.0), 0.0);
    }
}
