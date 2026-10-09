//! Model-written summaries and suggested tags (`enrich_file`) and conversation
//! titles (`title_conversation`), with deterministic models only.

mod support;

use std::sync::Arc;

use akasha_core::{Config, LlmProvider};
use akasha_llm::ChatModel;
use serde_json::{Value, json};
use sqlx::PgPool;
use support::{
    TestApp,
    chat::{ask, conversation},
    llm::Scripted,
    test_config,
};

const TEXT: &[u8] = b"Sourdough starter needs feeding twice a day. \
The starter doubles in size when it is active. Sourdough bread rises slowly.";

fn id(v: &Value) -> String {
    v["id"].as_str().expect("id").to_owned()
}

async fn file(app: &TestApp, cookie: &str, id: &str) -> Value {
    app.send("GET", &format!("/api/v1/files/{id}"), cookie, None)
        .await
        .json()
}

fn with(pool: &PgPool, model: &Arc<Scripted>) -> TestApp {
    let llm: Arc<dyn ChatModel> = model.clone();
    TestApp::with_llm(pool.clone(), test_config(), Some(llm))
}

/// Make every retrying job due now.
async fn retry_now(pool: &PgPool) {
    sqlx::query("UPDATE jobs SET run_at = now() WHERE status = 'failed'")
        .execute(pool)
        .await
        .expect("retry now");
}

#[sqlx::test(migrator = "akasha_db::MIGRATOR")]
async fn ready_files_get_a_summary_and_suggested_tags(pool: PgPool) {
    let config = Config {
        search_rate_per_minute: 0,
        ..test_config()
    };
    let app = TestApp::with_config(pool.clone(), config);
    let ada = app.user("ada@example.com").await;
    let fid = id(&app.upload(&ada, "bread.txt", TEXT).await.json());
    assert_eq!(app.run_jobs().await, 3, "extract, embed, enrich");

    let f = file(&app, &ada, &fid).await;
    assert_eq!(f["status"], "ready");
    assert_eq!(f["summary"], "Sourdough starter needs feeding twice a day.");
    assert_eq!(f["auto_tags"], json!(["sourdough", "starter", "needs"]));
    assert_eq!(f["tags"], json!([]));
    assert_eq!(f["enrichment"]["status"], "done");
    assert_eq!(f["enrichment"]["model"], "fake/fake-echo");

    // Filters and search match suggested tags; search results carry the summary.
    let listed = app
        .send("GET", "/api/v1/files?tag=Sourdough", &ada, None)
        .await
        .json();
    assert_eq!(listed["items"].as_array().map(Vec::len), Some(1));
    let found = app
        .send("GET", "/api/v1/search?q=starter&tags=sourdough", &ada, None)
        .await
        .json();
    assert_eq!(found["results"][0]["file"]["auto_tags"][0], "sourdough");
    assert_eq!(
        found["results"][0]["file"]["summary"],
        "Sourdough starter needs feeding twice a day."
    );

    // Nothing changed: a repeat delivery does not call the model again.
    let model = Scripted::new(None);
    let again = with(&pool, &model);
    enqueue(&pool, &fid, false).await;
    assert_eq!(again.run_jobs().await, 1);
    assert_eq!(model.calls(), 0);
}

async fn enqueue(pool: &PgPool, file_id: &str, force: bool) {
    let mut tx = pool.begin().await.expect("tx");
    let job = akasha::jobs::kinds::EnrichFile {
        file_id: file_id.parse().expect("uuid"),
        force,
    };
    akasha_jobs::enqueue(&mut tx, &job).await.expect("enqueue");
    tx.commit().await.expect("commit");
}

#[sqlx::test(migrator = "akasha_db::MIGRATOR")]
async fn user_tags_are_never_overwritten_and_suggestions_can_be_dropped(pool: PgPool) {
    let model = Scripted::new(Some(
        "```json\n{\"summary\": \"About bread.\", \"tags\": [\"Bread\", \"mine\", \"Baking\", \"bread\"]}\n```",
    ));
    let app = with(&pool, &model);
    let ada = app.user("ada@example.com").await;
    let fid = id(&app.upload(&ada, "bread.txt", TEXT).await.json());
    let patched = app
        .send(
            "PATCH",
            &format!("/api/v1/files/{fid}"),
            &ada,
            Some(json!({ "tags": ["Mine"] })),
        )
        .await;
    assert_eq!(patched.status, 200);
    app.run_jobs().await;
    let f = file(&app, &ada, &fid).await;
    assert_eq!(f["tags"], json!(["mine"]));
    // Deduplicated against each other and against the user's own tags.
    assert_eq!(f["auto_tags"], json!(["bread", "baking"]));
    assert_eq!(f["summary"], "About bread.");
    let req = model.last();
    assert!(req.json);
    assert!(
        req.messages[0]
            .content
            .starts_with("File name: bread.txt\nSourdough")
    );

    // Re-running replaces suggestions only.
    let rerun = app
        .send("POST", &format!("/api/v1/files/{fid}/enrich"), &ada, None)
        .await;
    assert_eq!(rerun.status, 202);
    assert_eq!(app.run_jobs().await, 1);
    assert_eq!(model.calls(), 2);
    assert_eq!(file(&app, &ada, &fid).await["tags"], json!(["mine"]));

    let dropped = app
        .send(
            "PATCH",
            &format!("/api/v1/files/{fid}"),
            &ada,
            Some(json!({ "auto_tags": ["baking"] })),
        )
        .await;
    assert_eq!(dropped.json()["auto_tags"], json!(["baking"]));
    assert_eq!(dropped.json()["tags"], json!(["mine"]));
}

#[sqlx::test(migrator = "akasha_db::MIGRATOR")]
async fn unusable_answers_fail_enrichment_but_not_the_file(pool: PgPool) {
    let model = Scripted::new(Some("I think this is about bread."));
    let app = with(&pool, &model);
    let ada = app.user("ada@example.com").await;
    let fid = id(&app.upload(&ada, "bread.txt", TEXT).await.json());
    app.run_jobs().await;
    let f = file(&app, &ada, &fid).await;
    assert_eq!(f["status"], "ready");
    assert_eq!(f["enrichment"], Value::Null, "retrying");
    for _ in 0..2 {
        retry_now(&pool).await;
        app.run_jobs().await;
    }
    assert_eq!(model.calls(), 3, "three attempts");
    let f = file(&app, &ada, &fid).await;
    assert_eq!(f["status"], "ready");
    assert_eq!(f["enrichment"]["status"], "failed");
    assert_eq!(f["summary"], Value::Null);
    let dead: String = sqlx::query_scalar("SELECT status FROM jobs WHERE kind = 'enrich_file'")
        .fetch_one(&pool)
        .await
        .expect("job");
    assert_eq!(dead, "dead");
}

#[sqlx::test(migrator = "akasha_db::MIGRATOR")]
async fn long_texts_send_the_start_and_later_samples(pool: PgPool) {
    let model = Scripted::new(Some(r#"{"summary": "Long.", "tags": ["long"]}"#));
    let app = with(&pool, &model);
    let ada = app.user("ada@example.com").await;
    let mut text = String::new();
    for i in 0..400 {
        text.push_str(&format!("Paragraph {i} talks about topic number {i}.\n\n"));
    }
    app.upload(&ada, "long.md", text.as_bytes()).await;
    app.run_jobs().await;
    let prompt = model.last().messages[0].content.clone();
    assert!(prompt.contains("Paragraph 0 "));
    assert!(prompt.contains("[... passages from later in the document ...]"));
    assert!(
        (300..400).any(|i| prompt.contains(&format!("Paragraph {i} "))),
        "the last third is sampled"
    );
    assert!(
        prompt.chars().count() < 8_000 + 3 * 1_000 + 200,
        "bounded input"
    );
}

#[sqlx::test(migrator = "akasha_db::MIGRATOR")]
async fn without_a_model_nothing_is_queued_and_rerunning_is_unavailable(pool: PgPool) {
    let app = TestApp::with_llm(pool.clone(), test_config(), None);
    let ada = app.user("ada@example.com").await;
    let fid = id(&app.upload(&ada, "bread.txt", TEXT).await.json());
    assert_eq!(app.run_jobs().await, 2, "extract, embed");
    assert_eq!(file(&app, &ada, &fid).await["enrichment"], Value::Null);
    let res = app
        .send("POST", &format!("/api/v1/files/{fid}/enrich"), &ada, None)
        .await;
    assert_eq!(res.status, 503);

    // Strict offline mode with a cloud provider: no model, nothing sent anywhere.
    let offline = Config {
        strict_offline: true,
        llm_provider: LlmProvider::Anthropic,
        anthropic_api_key: Some(akasha_core::Secret::new("sk-test")),
        ..test_config()
    };
    let app = TestApp::with_config(pool.clone(), offline);
    let bob = app.user("bob@example.com").await;
    app.upload(&bob, "b.txt", b"Offline bread notes.").await;
    assert_eq!(app.run_jobs().await, 2);
}

#[sqlx::test(migrator = "akasha_db::MIGRATOR")]
async fn rerunning_checks_owner_and_state(pool: PgPool) {
    let app = TestApp::new(pool.clone());
    let ada = app.user("ada@example.com").await;
    let eve = app.user("eve@example.com").await;
    let fid = id(&app.upload(&ada, "bread.txt", TEXT).await.json());
    let early = app
        .send("POST", &format!("/api/v1/files/{fid}/enrich"), &ada, None)
        .await;
    assert_eq!(early.status, 409, "not ready yet");
    app.run_jobs().await;
    let other = app
        .send("POST", &format!("/api/v1/files/{fid}/enrich"), &eve, None)
        .await;
    assert_eq!(other.status, 404);
    let ok = app
        .send("POST", &format!("/api/v1/files/{fid}/enrich"), &ada, None)
        .await;
    assert_eq!(ok.status, 202);
    assert_eq!(ok.json()["enrichment"]["status"], "done");
}

#[sqlx::test(migrator = "akasha_db::MIGRATOR")]
async fn the_first_answer_names_the_conversation_unless_renamed(pool: PgPool) {
    let app = TestApp::new(pool.clone());
    let ada = app.user("ada@example.com").await;
    app.upload(&ada, "bread.txt", TEXT).await;
    app.run_jobs().await;

    // Longer than the 60-character question title, shorter than 80.
    let question = "Does the sourdough starter need feeding twice a day when it doubles in size";
    let conv = conversation(&app, &ada).await;
    let (status, _) = ask(&app, &ada, &conv, json!({ "content": question })).await;
    assert_eq!(status, 200);
    let before = app
        .send("GET", &format!("/api/v1/conversations/{conv}"), &ada, None)
        .await
        .json();
    assert_ne!(before["title"], question, "shortened question first");
    assert_eq!(app.run_jobs().await, 1, "title job");
    let after = app
        .send("GET", &format!("/api/v1/conversations/{conv}"), &ada, None)
        .await
        .json();
    // The fake model echoes the question (the last line of the prompt).
    assert_eq!(after["title"], question);
    // A second answer does not queue another title.
    ask(&app, &ada, &conv, json!({ "content": "And in winter?" })).await;
    assert_eq!(app.run_jobs().await, 0);

    // A rename before the job runs wins.
    let renamed = conversation(&app, &ada).await;
    ask(&app, &ada, &renamed, json!({ "content": question })).await;
    app.send(
        "PATCH",
        &format!("/api/v1/conversations/{renamed}"),
        &ada,
        Some(json!({ "title": "Mine" })),
    )
    .await;
    assert_eq!(app.run_jobs().await, 1);
    let kept = app
        .send(
            "GET",
            &format!("/api/v1/conversations/{renamed}"),
            &ada,
            None,
        )
        .await
        .json();
    assert_eq!(kept["title"], "Mine");
}
