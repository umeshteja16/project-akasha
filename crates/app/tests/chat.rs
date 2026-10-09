//! Grounded chat over HTTP with the deterministic fake model: SSE answers with
//! citations, the refusal gate, `no_llm`, errors, cancellation, scoping and
//! owner isolation.

mod support;

use std::{
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};

use akasha_core::Config;
use akasha_llm::{ChatModel, ChatRequest, ChatStream, LlmError, fake::FakeChatModel};
use axum::{
    body::Body,
    http::{Request, StatusCode, header},
};
use futures_util::future::BoxFuture;
use http_body_util::BodyExt;
use serde_json::{Value, json};
use sqlx::PgPool;
use support::{
    TestApp,
    chat::{ask, conversation, find},
    test_config,
};
use tower::ServiceExt;

/// Counts calls, then delegates to [`FakeChatModel`].
struct Counting {
    inner: FakeChatModel,
    calls: Arc<AtomicUsize>,
}

impl ChatModel for Counting {
    fn provider(&self) -> &'static str {
        "fake"
    }

    fn model(&self) -> &str {
        self.inner.model()
    }

    fn stream<'a>(&'a self, req: &'a ChatRequest) -> BoxFuture<'a, Result<ChatStream, LlmError>> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.inner.stream(req)
    }
}

fn counting(inner: FakeChatModel) -> (Arc<dyn ChatModel>, Arc<AtomicUsize>) {
    let calls = Arc::new(AtomicUsize::new(0));
    let model = Counting {
        inner,
        calls: Arc::clone(&calls),
    };
    (Arc::new(model), calls)
}

const BURROWS: &[u8] = b"The aardvark digs a burrow at night and eats termites.";
const CHESS: &[u8] = b"# Openings\n\nQuantum chess openings and theory for beginners.";

async fn seeded(app: &TestApp, email: &str) -> (String, String, String) {
    let cookie = app.user(email).await;
    let a = app.upload(&cookie, "burrows.txt", BURROWS).await.json()["id"]
        .as_str()
        .expect("id")
        .to_owned();
    let b = app.upload(&cookie, "chess.md", CHESS).await.json()["id"]
        .as_str()
        .expect("id")
        .to_owned();
    app.run_jobs().await;
    (cookie, a, b)
}

#[sqlx::test(migrator = "akasha_db::MIGRATOR")]
async fn answers_stream_with_citations_and_are_stored(pool: PgPool) {
    let (model, calls) = counting(FakeChatModel::new());
    let app = TestApp::with_llm(pool, test_config(), Some(model));
    let (ada, burrows, _) = seeded(&app, "ada@example.com").await;
    let conv = conversation(&app, &ada).await;

    let (status, events) = ask(
        &app,
        &ada,
        &conv,
        json!({ "content": "What does the aardvark eat?" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let names: Vec<&str> = events.iter().map(|(e, _)| e.as_str()).collect();
    assert_eq!(names.first(), Some(&"sources"));
    assert_eq!(names.last(), Some(&"done"));
    assert!(
        names.iter().filter(|n| **n == "delta").count() > 1,
        "streamed in pieces"
    );

    let sources = find(&events, "sources");
    assert_eq!(sources["conversation_id"], conv.as_str());
    assert_eq!(sources["search_query"], "What does the aardvark eat?");
    let listed = sources["sources"].as_array().expect("sources");
    assert_eq!(listed[0]["n"], 1);
    assert_eq!(listed[0]["file_id"], burrows.as_str());
    assert_eq!(listed[0]["file_name"], "burrows.txt");
    assert!(
        listed[0]["quote"]
            .as_str()
            .expect("quote")
            .contains("aardvark")
    );

    let deltas: String = events
        .iter()
        .filter(|(e, _)| e == "delta")
        .map(|(_, d)| d["text"].as_str().expect("text"))
        .collect();
    let done = find(&events, "done");
    assert_eq!(done["status"], "answered");
    assert_eq!(done["content"], deltas.as_str());
    assert!(deltas.contains("[1]"));
    assert_eq!(done["model"], "fake/fake-echo");
    assert!(done["usage"]["output_tokens"].as_u64().expect("usage") > 0);
    let cited = done["citations"].as_array().expect("citations");
    assert_eq!(
        cited.len(),
        listed.len(),
        "the fake cites every source once"
    );
    assert_eq!(cited[0]["chunk_id"], listed[0]["chunk_id"]);
    assert_eq!(
        calls.load(Ordering::SeqCst),
        1,
        "no rewrite for a first question"
    );

    // Stored: question + answer, and the conversation got a title.
    let res = app
        .send(
            "GET",
            &format!("/api/v1/conversations/{conv}/messages"),
            &ada,
            None,
        )
        .await;
    assert_eq!(res.status, StatusCode::OK);
    let items = res.json()["items"].as_array().expect("items").clone();
    assert_eq!(items.len(), 2);
    assert_eq!(items[0]["role"], "user");
    assert_eq!(items[0]["content"], "What does the aardvark eat?");
    assert_eq!(items[1]["role"], "assistant");
    assert_eq!(items[1]["id"], done["message_id"]);
    assert_eq!(items[1]["citations"], done["citations"]);
    assert_eq!(items[1]["model"], "fake/fake-echo");
    let res = app
        .send("GET", &format!("/api/v1/conversations/{conv}"), &ada, None)
        .await;
    assert_eq!(res.json()["title"], "What does the aardvark eat?");

    // A follow-up is rewritten by the model (the fake echoes the question back).
    let (_, events) = ask(
        &app,
        &ada,
        &conv,
        json!({ "content": "And when does it dig?" }),
    )
    .await;
    assert_eq!(
        find(&events, "sources")["search_query"],
        "And when does it dig?"
    );
    assert_eq!(calls.load(Ordering::SeqCst), 3, "rewrite + answer");
    assert_eq!(find(&events, "done")["status"], "answered");
}

#[sqlx::test(migrator = "akasha_db::MIGRATOR")]
async fn weak_evidence_is_refused_without_calling_the_model(pool: PgPool) {
    let (model, calls) = counting(FakeChatModel::new());
    let app = TestApp::with_llm(pool, test_config(), Some(model));
    let (ada, _, _) = seeded(&app, "ada@example.com").await;
    let conv = conversation(&app, &ada).await;

    let (_, events) = ask(
        &app,
        &ada,
        &conv,
        json!({ "content": "What is the capital of Australia?" }),
    )
    .await;
    assert_eq!(find(&events, "sources")["sources"], json!([]));
    let done = find(&events, "done");
    assert_eq!(done["status"], "refused");
    assert_eq!(done["content"], "I couldn't find this in your files.");
    assert_eq!(done["citations"], json!([]));
    assert_eq!(done["model"], Value::Null);
    assert_eq!(calls.load(Ordering::SeqCst), 0, "the model was not asked");

    // An empty library refuses too.
    let bob = app.user("bob@example.com").await;
    let conv = conversation(&app, &bob).await;
    let (_, events) = ask(&app, &bob, &conv, json!({ "content": "aardvark" })).await;
    assert_eq!(find(&events, "done")["status"], "refused");

    // A threshold above every score refuses even good matches.
    let strict = TestApp::with_llm(
        app.db.clone(),
        Config {
            chat_min_rerank_score: Some(1.5),
            ..test_config()
        },
        None,
    );
    let conv = conversation(&strict, &ada).await;
    let (_, events) = ask(
        &strict,
        &ada,
        &conv,
        json!({ "content": "aardvark termites" }),
    )
    .await;
    assert_eq!(find(&events, "done")["status"], "refused");
}

#[sqlx::test(migrator = "akasha_db::MIGRATOR")]
async fn without_a_model_chat_returns_the_passages(pool: PgPool) {
    let config = Config {
        llm_provider: akasha_core::LlmProvider::None,
        ..test_config()
    };
    let app = TestApp::with_config(pool, config);
    let (ada, _, _) = seeded(&app, "ada@example.com").await;
    let conv = conversation(&app, &ada).await;
    let (status, events) = ask(&app, &ada, &conv, json!({ "content": "aardvark burrow" })).await;
    assert_eq!(status, StatusCode::OK);
    let sources = find(&events, "sources")["sources"].clone();
    let done = find(&events, "done");
    assert_eq!(done["status"], "no_llm");
    assert_eq!(done["citations"], sources, "every source is returned");
    assert!(!sources.as_array().expect("array").is_empty());
}

#[sqlx::test(migrator = "akasha_db::MIGRATOR")]
async fn scoping_to_files_limits_the_sources(pool: PgPool) {
    let app = TestApp::new(pool);
    let (ada, _, chess) = seeded(&app, "ada@example.com").await;
    let conv = conversation(&app, &ada).await;
    let (_, events) = ask(
        &app,
        &ada,
        &conv,
        json!({ "content": "quantum chess openings", "file_ids": [chess] }),
    )
    .await;
    let sources = find(&events, "sources")["sources"]
        .as_array()
        .expect("sources")
        .clone();
    assert!(!sources.is_empty());
    assert!(sources.iter().all(|s| s["file_id"] == chess.as_str()));
}

#[sqlx::test(migrator = "akasha_db::MIGRATOR")]
async fn model_failures_end_with_an_error_event_and_are_stored(pool: PgPool) {
    let model = FakeChatModel {
        fail_after: Some(2),
        ..FakeChatModel::default()
    };
    let app = TestApp::with_llm(pool, test_config(), Some(Arc::new(model)));
    let (ada, _, _) = seeded(&app, "ada@example.com").await;
    let conv = conversation(&app, &ada).await;
    let (_, events) = ask(&app, &ada, &conv, json!({ "content": "aardvark termites" })).await;
    let names: Vec<&str> = events.iter().map(|(e, _)| e.as_str()).collect();
    assert_eq!(names, ["sources", "delta", "delta", "error"]);
    let err = find(&events, "error");
    assert_eq!(err["code"], "llm_error");
    let res = app
        .send(
            "GET",
            &format!("/api/v1/conversations/{conv}/messages"),
            &ada,
            None,
        )
        .await;
    let items = res.json()["items"].as_array().expect("items").clone();
    assert_eq!(items[1]["id"], err["message_id"]);
    assert_eq!(items[1]["status"], "error");
    assert!(!items[1]["content"].as_str().expect("partial").is_empty());
}

#[sqlx::test(migrator = "akasha_db::MIGRATOR")]
async fn closing_the_connection_cancels_generation(pool: PgPool) {
    let slow = FakeChatModel {
        delay: Duration::from_millis(100),
        ..FakeChatModel::default()
    };
    let (model, _) = counting(slow);
    let app = TestApp::with_llm(pool.clone(), test_config(), Some(model));
    let (ada, _, _) = seeded(&app, "ada@example.com").await;
    let conv = conversation(&app, &ada).await;

    let req = Request::post(format!("/api/v1/conversations/{conv}/messages"))
        .header(header::COOKIE, &ada)
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(
            json!({ "content": "aardvark termites" }).to_string(),
        ))
        .expect("request");
    let res = app.router.clone().oneshot(req).await.expect("response");
    assert_eq!(res.status(), StatusCode::OK);
    let mut body = res.into_body();
    // Read until the first delta, then hang up.
    let mut seen = String::new();
    while !seen.contains("event: delta") {
        let frame = body.frame().await.expect("frame").expect("ok");
        if let Some(data) = frame.data_ref() {
            seen.push_str(std::str::from_utf8(data).expect("utf-8"));
        }
    }
    drop(body);

    let mut status = None;
    for _ in 0..50 {
        status =
            sqlx::query_scalar::<_, String>("SELECT status FROM messages WHERE role = 'assistant'")
                .fetch_optional(&pool)
                .await
                .expect("query");
        if status.is_some() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert_eq!(status.as_deref(), Some("cancelled"));
}

#[sqlx::test(migrator = "akasha_db::MIGRATOR")]
async fn questions_are_validated_and_rate_limited(pool: PgPool) {
    let config = Config {
        chat_rate_per_minute: 2,
        ..test_config()
    };
    let app = TestApp::with_config(pool, config);
    let ada = app.user("ada@example.com").await;
    let conv = conversation(&app, &ada).await;
    let (status, body) = ask(&app, &ada, &conv, json!({ "content": "   " })).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body:?}");
    let (status, _) = ask(&app, &ada, &conv, json!({ "content": "x".repeat(4001) })).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let (status, body) = ask(&app, &ada, &conv, json!({ "content": "hello" })).await;
    assert_eq!(
        status,
        StatusCode::TOO_MANY_REQUESTS,
        "two per minute: {body:?}"
    );
    assert_eq!(body[0].1["error"]["code"], "rate_limited");
}
