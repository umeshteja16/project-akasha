//! The eval report (also the baseline format) and the regression check.

use std::{collections::BTreeMap, path::Path};

use anyhow::Context;
use serde::{Deserialize, Serialize};

use super::metrics::Metrics;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Report {
    pub version: u32,
    pub embed_model: String,
    pub rerank_model: String,
    pub files: i64,
    pub chunks: i64,
    pub queries: usize,
    /// Keyed by mode: keyword, semantic, hybrid, hybrid_rerank.
    pub modes: BTreeMap<String, ModeReport>,
    /// Every query's outcome (left out of baselines).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub per_query: Vec<QueryOutcome>,
    /// Refusal-gate calibration (left out of baselines; needs a reranker).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gate: Option<super::gate::GateReport>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModeReport {
    #[serde(flatten)]
    pub overall: Metrics,
    /// Wall-clock search latency (informational, never gated).
    pub latency_p50_ms: f64,
    pub latency_p95_ms: f64,
    pub by_kind: BTreeMap<String, Metrics>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryOutcome {
    pub id: String,
    pub mode: String,
    /// 1-based rank of the first relevant file in the top 10.
    pub first_relevant: Option<usize>,
    pub ndcg_at_10: f64,
    /// The first five file names returned.
    pub top: Vec<String>,
}

impl Report {
    pub fn load(path: &Path) -> anyhow::Result<Self> {
        let text =
            std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
        serde_json::from_str(&text).with_context(|| format!("parsing {}", path.display()))
    }

    pub fn save(&self, path: &Path) -> anyhow::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let json = serde_json::to_string_pretty(self)? + "\n";
        std::fs::write(path, json).with_context(|| format!("writing {}", path.display()))
    }

    /// The report without per-query detail, as committed baselines store it.
    pub fn summary(&self) -> Self {
        Self {
            per_query: Vec::new(),
            gate: None,
            ..self.clone()
        }
    }

    /// Plain-text table of the per-mode metrics.
    pub fn table(&self) -> String {
        let mut out = format!(
            "{} queries over {} files ({} chunks); embed {}, rerank {}\n\n\
             {:<14}{:>9}{:>9}{:>10}{:>8}{:>9}{:>9}{:>9}\n",
            self.queries,
            self.files,
            self.chunks,
            self.embed_model,
            self.rerank_model,
            "mode",
            "R@1",
            "R@5",
            "R@10",
            "MRR",
            "nDCG@10",
            "p50 ms",
            "p95 ms"
        );
        for (mode, r) in &self.modes {
            let m = &r.overall;
            out += &format!(
                "{mode:<14}{:>9.3}{:>9.3}{:>10.3}{:>8.3}{:>9.3}{:>9.1}{:>9.1}\n",
                m.recall_at_1,
                m.recall_at_5,
                m.recall_at_10,
                m.mrr,
                m.ndcg_at_10,
                r.latency_p50_ms,
                r.latency_p95_ms
            );
        }
        out
    }
}

/// Quality metrics that fell more than `tolerance` below the baseline, as
/// human-readable lines. Latency is not compared (machines differ).
pub fn regressions(current: &Report, baseline: &Report, tolerance: f64) -> Vec<String> {
    let mut out = Vec::new();
    for (mode, base) in &baseline.modes {
        let Some(now) = current.modes.get(mode) else {
            out.push(format!("{mode}: not measured (in the baseline)"));
            continue;
        };
        for ((name, was), (_, is)) in base.overall.named().into_iter().zip(now.overall.named()) {
            if is < was - tolerance {
                out.push(format!(
                    "{mode} {name}: {is:.4} < baseline {was:.4} - {tolerance}"
                ));
            }
        }
    }
    out
}

/// Metrics that beat the baseline by more than `tolerance` (time to update it).
pub fn improvements(current: &Report, baseline: &Report, tolerance: f64) -> Vec<String> {
    let mut out = Vec::new();
    for (mode, base) in &baseline.modes {
        if let Some(now) = current.modes.get(mode) {
            for ((name, was), (_, is)) in base.overall.named().into_iter().zip(now.overall.named())
            {
                if is > was + tolerance {
                    out.push(format!("{mode} {name}: {is:.4} > baseline {was:.4}"));
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(mrr: f64) -> Report {
        let mode = ModeReport {
            overall: Metrics {
                mrr,
                ..Metrics::default()
            },
            latency_p50_ms: 1.0,
            latency_p95_ms: 2.0,
            by_kind: BTreeMap::new(),
        };
        Report {
            version: 1,
            embed_model: "e".into(),
            rerank_model: "r".into(),
            files: 1,
            chunks: 1,
            queries: 1,
            modes: BTreeMap::from([("hybrid".to_owned(), mode)]),
            per_query: Vec::new(),
            gate: None,
        }
    }

    #[test]
    fn only_drops_beyond_the_tolerance_regress() {
        let base = report(0.80);
        assert!(regressions(&report(0.79), &base, 0.02).is_empty());
        assert_eq!(regressions(&report(0.70), &base, 0.02).len(), 1);
        assert_eq!(improvements(&report(0.90), &base, 0.02).len(), 1);
        let mut missing = report(0.8);
        missing.modes.clear();
        assert_eq!(regressions(&missing, &base, 0.02).len(), 1);
    }
}
