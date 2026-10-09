//! `akasha eval`: search quality on a fixed benchmark (`eval/`).
//!
//! The corpus (`eval/corpus/`) goes through the real pipeline (upload checks,
//! extraction and embedding jobs) for a throwaway user, then every query of
//! `eval/queries.json` runs in each search mode. The report has Recall@1/5/10,
//! MRR and nDCG@10 per mode (and per query kind) plus latency, and is compared
//! with a committed baseline (`eval/baselines/<embed>+<rerank>.json`): any
//! quality metric more than the tolerance below the baseline fails the run.
//!
//! Two tiers: the default uses the deterministic `hash-384` embedder and
//! `overlap` reranker (no downloads; also run by `cargo test`, guarding fusion
//! and ranking code); `--real-models` uses the configured models.

mod ingest;
pub mod metrics;
pub mod report;
pub mod suite;

use std::{collections::BTreeMap, path::PathBuf, time::Instant};

use akasha_core::Config;
use akasha_db::PgPool;
use akasha_search::{ChunkFilter, Models, SearchMode, SearchRequest};
use akasha_storage::Storage;
use anyhow::{Context, bail};
use uuid::Uuid;

use self::{
    metrics::{DEPTH, Metrics, percentile, round},
    report::{ModeReport, QueryOutcome, Report},
    suite::Suite,
};
use crate::state::AppState;

/// Allowed drop per quality metric before the run fails.
pub const DEFAULT_TOLERANCE: f64 = 0.02;

/// `akasha eval` options.
#[derive(Debug, Clone)]
pub struct EvalOptions {
    /// Use the configured models instead of the deterministic fakes.
    pub real_models: bool,
    /// Holds `queries.json`, `corpus/` and `baselines/`.
    pub dir: PathBuf,
    /// Where to write the full JSON report (default `target/eval/<models>.json`).
    pub out: Option<PathBuf>,
    /// Overwrite the baseline with this run instead of comparing.
    pub update_baseline: bool,
    pub tolerance: f64,
}

/// The configuration the eval runs with: the deterministic models unless
/// `real_models`, no OCR (the corpus is text), one worker job at a time.
pub fn eval_config(mut config: Config, real_models: bool) -> Config {
    if !real_models {
        config.embed_model = akasha_ml::catalog::HASH_EMBED_MODEL.into();
        config.rerank_model = akasha_ml::catalog::OVERLAP_RERANK_MODEL.into();
    }
    config.ocr_enabled = false;
    config.worker_concurrency = 1;
    config
}

/// `<embed>+<rerank>`: names baselines and default report files.
pub fn models_key(config: &Config) -> String {
    format!("{}+{}", config.embed_model, rerank_name(config))
}

fn rerank_name(config: &Config) -> &str {
    match config.rerank_model.trim() {
        "" => "none",
        r => r,
    }
}

/// Run the benchmark in a scratch database next to `config.database_url`.
pub async fn run_cli(config: Config, opts: EvalOptions) -> anyhow::Result<()> {
    let config = eval_config(config, opts.real_models);
    let key = models_key(&config);
    let corpus_dir = opts.dir.join("corpus");
    let names = suite::corpus_files(&corpus_dir)?;
    let suite = Suite::load(&opts.dir.join("queries.json"), &names)?;

    let db = akasha_db::scratch::TempDatabase::create(&config.database_url)
        .await
        .context("creating a scratch database (needs CREATEDB)")?;
    tracing::info!(database = db.name(), "scratch database created");
    let result = evaluate(db.pool.clone(), config, &suite, &corpus_dir, &names).await;
    if let Err(err) = db.drop_now().await {
        tracing::warn!(%err, "dropping the scratch database failed");
    }
    let report = result?;

    println!("{}", report.table());
    let out = opts
        .out
        .unwrap_or_else(|| PathBuf::from(format!("target/eval/{key}.json")));
    report.save(&out)?;
    println!("Full report: {}", out.display());

    let baseline_path = opts.dir.join("baselines").join(format!("{key}.json"));
    if opts.update_baseline {
        report.summary().save(&baseline_path)?;
        println!("Baseline updated: {}", baseline_path.display());
        return Ok(());
    }
    if !baseline_path.exists() {
        println!(
            "No baseline for `{key}` ({}); nothing to compare. Record one with --update-baseline.",
            baseline_path.display()
        );
        return Ok(());
    }
    let baseline = Report::load(&baseline_path)?;
    for line in report::improvements(&report, &baseline, opts.tolerance) {
        println!("improved: {line}");
    }
    let regressions = report::regressions(&report, &baseline, opts.tolerance);
    if !regressions.is_empty() {
        bail!(
            "search quality regressed against {}:\n  {}",
            baseline_path.display(),
            regressions.join("\n  ")
        );
    }
    println!("No regression against {}", baseline_path.display());
    Ok(())
}

/// Load the corpus into `pool` (an empty, migrated database) and run every query
/// in every mode.
pub async fn evaluate(
    pool: PgPool,
    config: Config,
    suite: &Suite,
    corpus_dir: &std::path::Path,
    names: &[String],
) -> anyhow::Result<Report> {
    let state = AppState::new(pool.clone(), config, Storage::in_memory());
    ingest::prepare_model(&state).await?;
    let owner = ingest::load(&state, corpus_dir, names).await?;
    let stats = akasha_db::scratch::library_stats(&pool, owner).await?;

    let embedder = state.ml.embedder().await.context("loading the embedder")?;
    let reranker = state.ml.reranker().await.context("loading the reranker")?;
    let has_reranker = reranker.is_some();
    let models = Models {
        embedder: Ok(embedder),
        reranker: Ok(reranker),
    };
    let mut modes = vec![
        ("keyword", SearchMode::Keyword, false),
        ("semantic", SearchMode::Semantic, false),
        ("hybrid", SearchMode::Hybrid, false),
    ];
    if has_reranker {
        modes.push(("hybrid_rerank", SearchMode::Hybrid, true));
    }

    let mut report = Report {
        version: 1,
        embed_model: state.config.embed_model.clone(),
        rerank_model: rerank_name(&state.config).to_owned(),
        files: stats.files,
        chunks: stats.chunks,
        queries: suite.queries.len(),
        modes: BTreeMap::new(),
        per_query: Vec::new(),
    };
    for (name, mode, rerank) in modes {
        let (mode_report, outcomes) = run_mode(&pool, owner, suite, &models, name, mode, rerank)
            .await
            .with_context(|| format!("mode {name}"))?;
        report.modes.insert(name.to_owned(), mode_report);
        report.per_query.extend(outcomes);
    }
    Ok(report)
}

async fn run_mode(
    pool: &PgPool,
    owner: Uuid,
    suite: &Suite,
    models: &Models,
    name: &str,
    mode: SearchMode,
    rerank: bool,
) -> anyhow::Result<(ModeReport, Vec<QueryOutcome>)> {
    let request = |query: &str| SearchRequest {
        query: query.to_owned(),
        mode,
        filter: ChunkFilter::default(),
        limit: DEPTH,
        offset: 0,
        rerank,
    };
    // Warm caches (and lazily loaded model sessions) outside the timings.
    if let Some(q) = suite.queries.first() {
        akasha_search::search_files(pool, owner, &request(&q.query), models).await?;
    }
    let mut scored: Vec<(&str, Metrics)> = Vec::new();
    let mut latencies = Vec::new();
    let mut outcomes = Vec::new();
    for q in &suite.queries {
        let started = Instant::now();
        let res = akasha_search::search_files(pool, owner, &request(&q.query), models).await?;
        latencies.push(started.elapsed().as_secs_f64() * 1000.0);
        if res.meta.degraded {
            bail!("search degraded: {:?}", res.meta.warnings);
        }
        let ranked: Vec<String> = res.results.into_iter().map(|h| h.file.name).collect();
        let m = Metrics::score(&ranked, &q.relevant);
        outcomes.push(QueryOutcome {
            id: q.id.clone(),
            mode: name.to_owned(),
            first_relevant: ranked
                .iter()
                .take(DEPTH)
                .position(|n| q.relevant.contains(n))
                .map(|i| i + 1),
            ndcg_at_10: round(m.ndcg_at_10),
            top: ranked.into_iter().take(5).collect(),
        });
        scored.push((&q.kind, m));
    }
    let all: Vec<Metrics> = scored.iter().map(|(_, m)| *m).collect();
    let mut by_kind: BTreeMap<String, Vec<Metrics>> = BTreeMap::new();
    for (kind, m) in &scored {
        by_kind.entry((*kind).to_owned()).or_default().push(*m);
    }
    let report = ModeReport {
        overall: Metrics::mean(&all),
        latency_p50_ms: (percentile(&latencies, 50.0) * 10.0).round() / 10.0,
        latency_p95_ms: (percentile(&latencies, 95.0) * 10.0).round() / 10.0,
        by_kind: by_kind
            .into_iter()
            .map(|(k, v)| (k, Metrics::mean(&v)))
            .collect(),
    };
    Ok((report, outcomes))
}
