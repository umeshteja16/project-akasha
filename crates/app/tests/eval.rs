//! The deterministic search-quality gate: the `eval/` benchmark with the built-in
//! `hash-384` embedder and `overlap` reranker must not fall below the committed
//! baseline (`eval/baselines/hash-384+overlap.json`). Guards fusion, ranking and
//! retrieval SQL; real-model quality is checked by `akasha eval --real-models`.
//!
//! After an intended ranking change, re-record the baseline with
//! `cargo run -p akasha -- eval --update-baseline` and commit it.

use std::path::PathBuf;

use akasha::eval::{self, report, suite};
use sqlx::PgPool;

mod support;

#[sqlx::test(migrator = "akasha_db::MIGRATOR")]
async fn search_quality_does_not_regress(pool: PgPool) {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../eval");
    let corpus = dir.join("corpus");
    let names = suite::corpus_files(&corpus).expect("corpus");
    let suite = suite::Suite::load(&dir.join("queries.json"), &names).expect("suite");
    let config = eval::eval_config(support::test_config(), false);
    let key = eval::models_key(&config);
    let report = eval::evaluate(pool, config, &suite, &corpus, &names)
        .await
        .expect("eval");

    assert_eq!(report.files, names.len() as i64);
    assert_eq!(report.queries, suite.queries.len());
    // The refusal-gate calibration ran for every gate question (informational).
    let gate = report.gate.as_ref().expect("gate report");
    let set = suite.gate.as_ref().expect("eval/gate.json");
    assert_eq!(
        gate.questions.len(),
        set.answerable.len() + set.unanswerable.len()
    );
    assert!(gate.answerable.median > gate.unanswerable.median);
    let baseline =
        report::Report::load(&dir.join(format!("baselines/{key}.json"))).expect("baseline");
    let regressions = report::regressions(&report, &baseline, eval::DEFAULT_TOLERANCE);
    assert!(
        regressions.is_empty(),
        "search quality regressed (re-record with `akasha eval --update-baseline` if \
         intended):\n{}\n{}",
        regressions.join("\n"),
        report.table()
    );
}
