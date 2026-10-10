//! Prometheus metrics: token-protected `/metrics` on the main port, HTTP and job
//! metrics by route template and kind.

mod support;

use akasha_core::{Config, Secret};
use axum::{
    body::Body,
    http::{Request, StatusCode, header},
};
use sqlx::PgPool;
use support::{TestApp, test_config};

async fn scrape(app: &TestApp, token: Option<&str>) -> (StatusCode, String) {
    let mut req = Request::get("/metrics");
    if let Some(token) = token {
        req = req.header(header::AUTHORIZATION, format!("Bearer {token}"));
    }
    let res = app.request(req.body(Body::empty()).expect("request")).await;
    (res.status, String::from_utf8(res.bytes).expect("utf8"))
}

#[sqlx::test(migrator = "akasha_db::MIGRATOR")]
async fn metrics_need_the_token_and_count_requests_and_jobs(pool: PgPool) {
    let config = Config {
        metrics_enabled: true,
        metrics_token: Some(Secret::new("scrape-me")),
        ..test_config()
    };
    akasha::metrics::check_config(&config).expect("valid");
    akasha::metrics::install(&config).expect("recorder");
    let app = TestApp::with_config(pool, config);

    assert_eq!(scrape(&app, None).await.0, StatusCode::UNAUTHORIZED);
    assert_eq!(
        scrape(&app, Some("wrong")).await.0,
        StatusCode::UNAUTHORIZED
    );

    let ada = app.user("ada@example.com").await;
    let res = app.upload(&ada, "notes.txt", b"metrics are fun").await;
    assert_eq!(res.status, StatusCode::CREATED);
    let id = res.json()["id"].as_str().expect("id").to_owned();
    app.send("GET", &format!("/api/v1/files/{id}"), &ada, None)
        .await;
    app.run_jobs().await;

    let (status, body) = scrape(&app, Some("scrape-me")).await;
    assert_eq!(status, StatusCode::OK);
    for needle in [
        "akasha_build_info{",
        r#"route="/api/v1/files/{id}""#,
        r#"route="/api/v1/auth/register""#,
        "akasha_http_request_duration_seconds_bucket",
        r#"akasha_jobs_finished_total{kind="extract_file",outcome="succeeded"}"#,
        "akasha_job_duration_seconds_bucket",
        r#"akasha_ingest_duration_seconds_bucket{stage="extract""#,
    ] {
        assert!(body.contains(needle), "missing {needle} in\n{body}");
    }
    assert!(!body.contains(&id), "raw paths never become labels");
}
