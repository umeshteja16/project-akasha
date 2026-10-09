//! Search over HTTP, seeded through the real pipeline (upload → extract → embed)
//! with the hash embedder and overlap reranker.

mod support;

use axum::http::StatusCode;
use serde_json::{Value, json};
use sqlx::PgPool;
use support::{TestApp, test_config};

fn id(v: &Value) -> String {
    v["id"].as_str().expect("id").to_owned()
}

async fn seeded(pool: PgPool) -> (TestApp, String, String, String) {
    let app = TestApp::new(pool);
    let ada = app.user("ada@example.com").await;
    let a = id(&app
        .upload(&ada, "burrows.txt", b"The aardvark digs a burrow at night.")
        .await
        .json());
    let b = id(&app
        .upload(
            &ada,
            "chess.md",
            b"# Openings\n\nQuantum chess openings and theory.",
        )
        .await
        .json());
    app.run_jobs().await;
    (app, ada, a, b)
}

#[sqlx::test(migrator = "akasha_db::MIGRATOR")]
async fn search_returns_grouped_and_chunk_results(pool: PgPool) {
    let (app, ada, a, _) = seeded(pool).await;

    let res = app
        .send("GET", "/api/v1/search?q=aardvark", &ada, None)
        .await;
    assert_eq!(res.status, StatusCode::OK);
    let body = res.json();
    assert_eq!(body["mode"], "hybrid");
    assert_eq!(body["degraded"], false);
    assert_eq!(body["reranked"], true, "overlap reranker is configured");
    let top = &body["results"][0];
    assert_eq!(top["file"]["id"], a.as_str());
    assert_eq!(top["file"]["name"], "burrows.txt");
    assert_eq!(top["match_count"], 1);
    let m = &top["matches"][0];
    let snippet: Vec<char> = m["snippet"]["text"]
        .as_str()
        .expect("snippet")
        .chars()
        .collect();
    let h = &m["snippet"]["highlights"][0];
    let (start, end) = (
        h["start"].as_u64().expect("start"),
        h["end"].as_u64().expect("end"),
    );
    let marked: String = snippet[start as usize..end as usize].iter().collect();
    assert_eq!(marked, "aardvark");
    assert_eq!(m["scores"]["keyword_rank"], 1);
    assert_eq!(m["char_start"], 0);
    for stage in [
        "embed_ms",
        "keyword_ms",
        "semantic_ms",
        "fetch_ms",
        "rerank_ms",
        "total_ms",
    ] {
        assert!(body["timings"][stage].is_number(), "{stage}");
    }

    let res = app
        .send(
            "GET",
            "/api/v1/search/chunks?q=aardvark&mode=keyword&rerank=false",
            &ada,
            None,
        )
        .await;
    assert_eq!(res.status, StatusCode::OK);
    let body = res.json();
    assert_eq!(body["results"].as_array().expect("results").len(), 1);
    assert_eq!(
        body["results"][0]["text"],
        "The aardvark digs a burrow at night."
    );
    assert_eq!(body["results"][0]["file"]["id"], a.as_str());
    assert_eq!(body["reranked"], false);
}

#[sqlx::test(migrator = "akasha_db::MIGRATOR")]
async fn other_users_never_see_your_results(pool: PgPool) {
    let (app, ada, a, _) = seeded(pool).await;
    let bob = app.user("bob@example.com").await;
    for path in [
        "/api/v1/search?q=aardvark",
        "/api/v1/search/chunks?q=aardvark",
    ] {
        for mode in ["keyword", "semantic", "hybrid"] {
            let url = format!("{path}&mode={mode}&file_ids={a}");
            let res = app.send("GET", &url, &bob, None).await;
            assert_eq!(res.status, StatusCode::OK);
            assert_eq!(res.json()["results"], json!([]), "{url}");
        }
    }
    let res = app
        .send("GET", &format!("/api/v1/files/{a}/similar"), &bob, None)
        .await;
    assert_eq!(res.status, StatusCode::NOT_FOUND);
    let res = app.send("GET", "/api/v1/search?q=aardvark", "", None).await;
    assert_eq!(res.status, StatusCode::UNAUTHORIZED);
    // Ada still finds her file.
    let res = app
        .send("GET", "/api/v1/search?q=aardvark", &ada, None)
        .await;
    assert_eq!(res.json()["results"].as_array().expect("results").len(), 2);
}

#[sqlx::test(migrator = "akasha_db::MIGRATOR")]
async fn filters_apply_over_http(pool: PgPool) {
    let (app, ada, a, b) = seeded(pool).await;
    let tag = json!({ "tags": ["Zoo"], "is_pinned": true });
    let res = app
        .send("PATCH", &format!("/api/v1/files/{a}"), &ada, Some(tag))
        .await;
    assert_eq!(res.status, StatusCode::OK);

    let get = async |filter: &str| -> Vec<String> {
        // Semantic search returns every file's nearest chunks, so only filters narrow it.
        let url = format!("/api/v1/search?q=aardvark&mode=semantic&{filter}");
        let res = app.send("GET", &url, &ada, None).await;
        assert_eq!(res.status, StatusCode::OK, "{filter}");
        res.json()["results"]
            .as_array()
            .expect("results")
            .iter()
            .map(|r| r["file"]["id"].as_str().expect("id").to_owned())
            .collect()
    };
    assert_eq!(get("tags=zoo").await, vec![a.clone()]);
    assert_eq!(get("pinned=false").await, vec![b.clone()]);
    assert_eq!(get("type=text").await.len(), 2);
    assert!(get("type=pdf").await.is_empty());
    assert!(get("to=2000-01-01").await.is_empty());
    assert_eq!(get("from=2000-01-01").await.len(), 2);
    assert_eq!(get(&format!("file_ids={b}")).await, vec![b.clone()]);
}

#[sqlx::test(migrator = "akasha_db::MIGRATOR")]
async fn invalid_parameters_are_400(pool: PgPool) {
    let (app, ada, _, _) = seeded(pool).await;
    let long = "a".repeat(501);
    for q in [
        "/api/v1/search",
        "/api/v1/search?q=",
        &format!("/api/v1/search?q={long}"),
        "/api/v1/search?q=x&limit=0",
        "/api/v1/search?q=x&limit=51",
        "/api/v1/search?q=x&page=0",
        "/api/v1/search?q=x&page=5&limit=50",
        "/api/v1/search?q=x&mode=fuzzy",
        "/api/v1/search?q=x&type=spreadsheet",
        "/api/v1/search?q=x&from=yesterday",
        "/api/v1/search?q=x&file_ids=1,2",
    ] {
        let res = app.send("GET", q, &ada, None).await;
        assert_eq!(res.status, StatusCode::BAD_REQUEST, "{q}");
        assert_eq!(res.code(), "bad_request", "{q}");
    }
}

#[sqlx::test(migrator = "akasha_db::MIGRATOR")]
async fn searches_are_rate_limited_per_user(pool: PgPool) {
    let config = akasha_core::Config {
        search_rate_per_minute: 2,
        ..test_config()
    };
    let app = TestApp::with_config(pool, config);
    let ada = app.user("ada@example.com").await;
    let bob = app.user("bob@example.com").await;
    for _ in 0..2 {
        let res = app.send("GET", "/api/v1/search?q=x", &ada, None).await;
        assert_eq!(res.status, StatusCode::OK);
    }
    let res = app
        .send("GET", "/api/v1/search/chunks?q=x", &ada, None)
        .await;
    assert_eq!(res.status, StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(res.code(), "rate_limited");
    let res = app.send("GET", "/api/v1/search?q=x", &bob, None).await;
    assert_eq!(res.status, StatusCode::OK, "limits are per user");
}

#[sqlx::test(migrator = "akasha_db::MIGRATOR")]
async fn missing_embedding_model_degrades_to_keyword(pool: PgPool) {
    let models = tempfile::tempdir().expect("tempdir");
    // A real ONNX model with no files and no download URL cannot load.
    let config = akasha_core::Config {
        embed_model: akasha_ml::catalog::DEFAULT_EMBED_MODEL.into(),
        models_dir: models.path().display().to_string(),
        ..test_config()
    };
    let app = TestApp::with_config(pool.clone(), config);
    let ada = app.user("ada@example.com").await;
    app.upload(&ada, "a.txt", b"The aardvark digs.").await;
    app.run_jobs().await;

    let res = app
        .send("GET", "/api/v1/search?q=aardvark&mode=semantic", &ada, None)
        .await;
    assert_eq!(res.status, StatusCode::OK);
    let body = res.json();
    assert_eq!(body["requested_mode"], "semantic");
    assert_eq!(body["mode"], "keyword");
    assert_eq!(body["degraded"], true);
    assert!(
        body["warnings"][0]
            .as_str()
            .expect("warning")
            .contains("embedding model")
    );
    assert_eq!(body["results"].as_array().expect("results").len(), 1);
}

#[sqlx::test(migrator = "akasha_db::MIGRATOR")]
async fn similar_files_over_http(pool: PgPool) {
    let (app, ada, a, b) = seeded(pool).await;
    let c = id(&app
        .upload(&ada, "more.txt", b"Another aardvark digs another burrow.")
        .await
        .json());
    app.run_jobs().await;

    let res = app
        .send("GET", &format!("/api/v1/files/{a}/similar"), &ada, None)
        .await;
    assert_eq!(res.status, StatusCode::OK);
    let items = res.json()["items"].clone();
    assert_eq!(items[0]["file"]["id"], c.as_str());
    assert_eq!(items[1]["file"]["id"], b.as_str());
    assert!(items[0]["similarity"].as_f64() > items[1]["similarity"].as_f64());

    let res = app
        .send(
            "GET",
            &format!("/api/v1/files/{a}/similar?limit=1"),
            &ada,
            None,
        )
        .await;
    assert_eq!(res.json()["items"].as_array().expect("items").len(), 1);
    let res = app
        .send(
            "GET",
            &format!("/api/v1/files/{a}/similar?limit=21"),
            &ada,
            None,
        )
        .await;
    assert_eq!(res.status, StatusCode::BAD_REQUEST);
}
