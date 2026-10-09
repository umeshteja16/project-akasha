//! The relevance floor over HTTP: a small library where vector search would
//! list every file for any query. Only real matches come back by default;
//! `include_weak=true` adds the loosely related rest, marked and listed last.

mod support;

use std::path::PathBuf;

use axum::http::StatusCode;
use serde_json::Value;
use sqlx::PgPool;
use support::TestApp;

/// Seven unrelated files from the eval corpus (the reported case: a search for
/// "lunar module eagle" listed the bird log and the budget CSV too).
const FILES: [&str; 7] = [
    "moon-landing-1969.md",
    "bird-watching-log.txt",
    "household-budget.csv",
    "cat-care.md",
    "coffee-brewing.md",
    "sleep-hygiene.md",
    "git-rebase-guide.md",
];

async fn library(pool: PgPool) -> (TestApp, String) {
    let app = TestApp::new(pool);
    let ada = app.user("ada@example.com").await;
    let corpus = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../eval/corpus");
    for name in FILES {
        let bytes = std::fs::read(corpus.join(name)).expect("corpus file");
        assert!(app.upload(&ada, name, &bytes).await.status.is_success());
    }
    app.run_jobs().await;
    (app, ada)
}

fn names(body: &Value) -> Vec<String> {
    body["results"]
        .as_array()
        .expect("results")
        .iter()
        .map(|r| r["file"]["name"].as_str().expect("name").to_owned())
        .collect()
}

#[sqlx::test(migrator = "akasha_db::MIGRATOR")]
async fn unrelated_files_are_not_listed_as_results(pool: PgPool) {
    let (app, ada) = library(pool).await;

    for params in ["", "&rerank=false"] {
        let res = app
            .send(
                "GET",
                &format!("/api/v1/search?q=lunar+module+eagle{params}"),
                &ada,
                None,
            )
            .await;
        assert_eq!(res.status, StatusCode::OK);
        let body = res.json();
        assert_eq!(
            names(&body),
            vec!["moon-landing-1969.md"],
            "only the moon landing matches ({params})"
        );
        assert_eq!(body["results"][0]["loosely_related"], false);
        assert!(
            body["loosely_related"].as_u64().expect("count") > 0,
            "the hidden neighbours are counted ({params})"
        );
    }
    // Pure vector search with the (meaningless) hash embedder: whatever clears
    // the floor, the unrelated files do not.
    let res = app
        .send(
            "GET",
            "/api/v1/search?q=lunar+module+eagle&mode=semantic&rerank=false",
            &ada,
            None,
        )
        .await;
    let listed = names(&res.json());
    assert!(
        listed.iter().all(|n| n == "moon-landing-1969.md"),
        "{listed:?}"
    );

    // On request, the loosely related files follow the match, marked.
    let res = app
        .send(
            "GET",
            "/api/v1/search?q=lunar+module+eagle&include_weak=true&limit=20",
            &ada,
            None,
        )
        .await;
    let body = res.json();
    let all = names(&body);
    assert_eq!(all[0], "moon-landing-1969.md");
    assert!(all.len() > 1, "semantic neighbours exist: {all:?}");
    assert_eq!(body["loosely_related"], 0);
    let results = body["results"].as_array().expect("results");
    assert_eq!(results[0]["loosely_related"], false);
    assert!(results[1..].iter().all(|r| r["loosely_related"] == true));

    // Passage-level search applies the same floor.
    let res = app
        .send(
            "GET",
            "/api/v1/search/chunks?q=lunar+module+eagle",
            &ada,
            None,
        )
        .await;
    let body = res.json();
    let files: Vec<&str> = body["results"]
        .as_array()
        .expect("results")
        .iter()
        .map(|r| r["file"]["name"].as_str().expect("name"))
        .collect();
    assert!(
        files.iter().all(|f| *f == "moon-landing-1969.md"),
        "{files:?}"
    );
}

#[sqlx::test(migrator = "akasha_db::MIGRATOR")]
async fn off_topic_queries_find_nothing_and_keyword_hits_always_count(pool: PgPool) {
    let (app, ada) = library(pool).await;

    let res = app
        .send(
            "GET",
            "/api/v1/search?q=quantum+chromodynamics+lattice",
            &ada,
            None,
        )
        .await;
    let body = res.json();
    assert_eq!(names(&body), Vec::<String>::new());
    assert!(body["loosely_related"].as_u64().expect("count") > 0);

    // A single shared word is a keyword hit: always listed.
    let res = app.send("GET", "/api/v1/search?q=kitten", &ada, None).await;
    assert_eq!(names(&res.json()), vec!["cat-care.md"]);
}
