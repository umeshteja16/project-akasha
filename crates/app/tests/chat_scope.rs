//! A conversation's file scope: stored with the conversation, checked against
//! the owner's files, used for questions that do not name their own files.

mod support;

use axum::http::StatusCode;
use serde_json::{Value, json};
use sqlx::PgPool;
use support::{
    TestApp,
    chat::{ask, find},
};

const BURROWS: &[u8] = b"The aardvark digs a burrow at night and eats termites.";
const CHESS: &[u8] = b"# Openings\n\nQuantum chess openings and theory for beginners.";

async fn upload(app: &TestApp, cookie: &str, name: &str, bytes: &[u8]) -> String {
    app.upload(cookie, name, bytes).await.json()["id"]
        .as_str()
        .expect("id")
        .to_owned()
}

fn source_files(events: &[(String, Value)]) -> Vec<String> {
    find(events, "sources")["sources"]
        .as_array()
        .expect("sources")
        .iter()
        .map(|s| s["file_id"].as_str().expect("file id").to_owned())
        .collect()
}

#[sqlx::test(migrator = "akasha_db::MIGRATOR")]
async fn conversations_remember_their_file_scope(pool: PgPool) {
    let app = TestApp::new(pool);
    let ada = app.user("ada@example.com").await;
    let bob = app.user("bob@example.com").await;
    let burrows = upload(&app, &ada, "burrows.txt", BURROWS).await;
    let chess = upload(&app, &ada, "chess.md", CHESS).await;
    let bobs = upload(&app, &bob, "bob.txt", b"Bob's aardvark notes.").await;
    app.run_jobs().await;

    // Unknown and other people's ids are dropped.
    let res = app
        .send(
            "POST",
            "/api/v1/conversations",
            &ada,
            Some(json!({ "file_ids": [chess, bobs, uuid::Uuid::new_v4()] })),
        )
        .await;
    assert_eq!(res.status, StatusCode::CREATED);
    let conv = res.json()["id"].as_str().expect("id").to_owned();
    assert_eq!(res.json()["file_ids"], json!([chess]));
    let path = format!("/api/v1/conversations/{conv}");
    assert_eq!(
        app.send("GET", &path, &ada, None).await.json()["file_ids"],
        json!([chess])
    );

    // Questions without their own scope use the conversation's.
    let (_, events) = ask(&app, &ada, &conv, json!({ "content": "aardvark chess" })).await;
    let files = source_files(&events);
    assert!(!files.is_empty());
    assert!(files.iter().all(|f| *f == chess), "{files:?}");
    // A question's own scope wins.
    let (_, events) = ask(
        &app,
        &ada,
        &conv,
        json!({ "content": "aardvark chess", "file_ids": [burrows] }),
    )
    .await;
    assert!(source_files(&events).iter().all(|f| *f == burrows));

    // Change the scope; `[]` means every file.
    let res = app
        .send("PATCH", &path, &ada, Some(json!({ "file_ids": [burrows] })))
        .await;
    assert_eq!(res.status, StatusCode::OK);
    assert_eq!(res.json()["file_ids"], json!([burrows]));
    let (_, events) = ask(&app, &ada, &conv, json!({ "content": "aardvark burrow" })).await;
    assert!(source_files(&events).iter().all(|f| *f == burrows));
    let res = app
        .send(
            "PATCH",
            &path,
            &ada,
            Some(json!({ "file_ids": [], "title": "Animals" })),
        )
        .await;
    assert_eq!(res.json()["file_ids"], json!([]));
    assert_eq!(res.json()["title"], "Animals");

    // Validation and isolation.
    let res = app.send("PATCH", &path, &ada, Some(json!({}))).await;
    assert_eq!(res.status, StatusCode::BAD_REQUEST);
    let many: Vec<String> = (0..101).map(|_| uuid::Uuid::new_v4().to_string()).collect();
    let res = app
        .send("PATCH", &path, &ada, Some(json!({ "file_ids": many })))
        .await;
    assert_eq!(res.status, StatusCode::BAD_REQUEST);
    let res = app
        .send("PATCH", &path, &bob, Some(json!({ "file_ids": [] })))
        .await;
    assert_eq!(res.status, StatusCode::NOT_FOUND);
}
