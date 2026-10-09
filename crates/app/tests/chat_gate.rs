//! Calibration of the refusal gate for the deterministic models (`hash-384` +
//! `overlap`): over part of the `eval/` corpus, natural questions the files
//! answer must pass the gate and questions they do not answer must be refused
//! without calling the model. Guards `chat::evidence::default_min_score`.

mod support;

use std::path::PathBuf;

use akasha::chat::evidence::{Evidence, assess};
use akasha_core::Config;
use akasha_search::{ChunkFilter, Models, SearchMode, SearchRequest};
use sqlx::PgPool;
use support::{TestApp, test_config};

const FILES: [&str; 8] = [
    "postgres-vacuum.md",
    "sourdough-starter.md",
    "marathon-training-plan.md",
    "rust-ownership.md",
    "travel-japan-itinerary.md",
    "cat-care.md",
    "coffee-brewing.md",
    "tomato-growing.md",
];

const ANSWERABLE: [&str; 10] = [
    "How often should I feed the sourdough starter?",
    "What does VACUUM FULL do to the table?",
    "How long is the marathon taper?",
    "When does the kitten need its booster vaccination?",
    "What is the coffee to water ratio for pour-over?",
    "What causes blossom end rot on tomatoes?",
    "Is the rail pass worth it in Japan?",
    "How many mutable references can I have in Rust?",
    "autovacuum scale factor for a large table",
    "What should I do if the starter smells of nail polish remover?",
];

const UNANSWERABLE: [&str; 8] = [
    "What is the capital of Australia?",
    "Who won the football world cup in 2014?",
    "How do I file my quarterly VAT return?",
    "What is the boiling point of mercury?",
    "Which vitamins are in spinach?",
    "How do I change a car tyre?",
    "What is my landlord's phone number?",
    "Explain quantum entanglement simply.",
];

#[sqlx::test(migrator = "akasha_db::MIGRATOR")]
async fn gate_lets_answerable_questions_through_and_refuses_off_topic_ones(pool: PgPool) {
    let config = Config {
        search_rate_per_minute: 0,
        ..test_config()
    };
    let app = TestApp::with_config(pool.clone(), config.clone());
    let ada = app.user("ada@example.com").await;
    let corpus = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../eval/corpus");
    for name in FILES {
        let bytes = std::fs::read(corpus.join(name)).expect("corpus file");
        assert!(app.upload(&ada, name, &bytes).await.status.is_success());
    }
    app.run_jobs().await;
    let owner: uuid::Uuid = sqlx::query_scalar("SELECT id FROM users")
        .fetch_one(&pool)
        .await
        .expect("owner");
    let models = Models {
        embedder: akasha_ml::load_embedder(&config.embed_model, &ml_options())
            .map_err(|e| e.to_string()),
        reranker: akasha_ml::load_reranker(&config.rerank_model, &ml_options())
            .map_err(|e| e.to_string()),
    };

    let mut report = Vec::new();
    let verdict = |q: &str| {
        let req = SearchRequest {
            query: q.into(),
            mode: SearchMode::Hybrid,
            filter: ChunkFilter::default(),
            limit: 20,
            offset: 0,
            rerank: true,
        };
        let pool = pool.clone();
        let models = models.clone();
        async move {
            let res = akasha_search::search_chunks(&pool, owner, &req, &models)
                .await
                .expect("search");
            let top = res.results.first().and_then(|h| h.chunk.scores.rerank);
            (assess(&res.results, Some("overlap"), None), top)
        }
    };
    let mut wrong = Vec::new();
    for q in ANSWERABLE {
        let (v, top) = verdict(q).await;
        report.push(format!("answerable   {top:?} {q}"));
        if v != Evidence::Sufficient {
            wrong.push(format!("refused answerable: {q} ({top:?})"));
        }
    }
    for q in UNANSWERABLE {
        let (v, top) = verdict(q).await;
        report.push(format!("unanswerable {top:?} {q}"));
        if v == Evidence::Sufficient {
            wrong.push(format!("answered off-topic: {q} ({top:?})"));
        }
    }
    assert!(
        wrong.is_empty(),
        "{}\n\nscores:\n{}",
        wrong.join("\n"),
        report.join("\n")
    );
}

fn ml_options() -> akasha_ml::MlOptions {
    akasha_ml::MlOptions {
        models_dir: PathBuf::from("unused"),
        models_url: String::new(),
        ort_library: String::new(),
        threads: 1,
    }
}
