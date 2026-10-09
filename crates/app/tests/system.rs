//! `GET /api/v1/system/status`: models, services and the job queue, for
//! signed-in users only and without secrets.

mod support;

use akasha_core::{Config, Secret};
use axum::http::StatusCode;
use sqlx::PgPool;
use support::{TestApp, test_config};

const KEY: &str = "sk-ant-very-secret-test-key";

#[sqlx::test(migrator = "akasha_db::MIGRATOR")]
async fn system_status_reports_models_and_queue(pool: PgPool) {
    let config = Config {
        anthropic_api_key: Some(Secret::new(KEY)),
        database_url: "postgres://akasha:hunter2@db/akasha".into(),
        ..test_config()
    };
    let app = TestApp::with_config(pool, config);
    let res = app.send("GET", "/api/v1/system/status", "", None).await;
    assert_eq!(res.status, StatusCode::UNAUTHORIZED);

    let ada = app.user("ada@example.com").await;
    app.upload(&ada, "a.txt", b"The aardvark digs.").await;

    let res = app.send("GET", "/api/v1/system/status", &ada, None).await;
    assert_eq!(res.status, StatusCode::OK);
    let text = String::from_utf8(res.bytes.clone()).expect("utf-8");
    assert!(!text.contains(KEY) && !text.contains("hunter2"), "{text}");
    let body = res.json();
    assert_eq!(body["embedding"]["name"], "hash-384");
    assert_eq!(body["embedding"]["dimensions"], 384);
    assert_eq!(body["reranker"]["name"], "overlap");
    assert_eq!(body["onnx_runtime"]["status"], "not_needed");
    assert_eq!(body["ocr"]["status"], "disabled");
    assert_eq!(body["chat"]["provider"], "fake");
    assert_eq!(body["chat"]["local"], true);
    assert_eq!(body["chat"]["status"], "ready");
    assert_eq!(body["strict_offline"], false);
    assert_eq!(body["relevance"]["min_similarity"], 0.15);
    assert_eq!(body["relevance"]["min_rerank_score"], 0.25);
    assert!(body["worker"]["queued"].as_i64().expect("queued") > 0);
    assert_eq!(body["worker"]["health"], "idle");

    // Searching loads the models; the worker drains the queue.
    app.send("GET", "/api/v1/search?q=aardvark", &ada, None)
        .await;
    app.run_jobs().await;
    let body = app
        .send("GET", "/api/v1/system/status", &ada, None)
        .await
        .json();
    assert_eq!(body["embedding"]["status"], "ready");
    assert_eq!(body["reranker"]["status"], "ready");
    assert_eq!(body["worker"]["queued"], 0);
    assert!(body["worker"]["last_finished_at"].is_string());
}
