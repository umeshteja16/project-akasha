//! Storage usage on `/me` and the owner's self-service limit.

use axum::http::StatusCode;
use serde_json::json;
use sqlx::PgPool;

mod support;
use support::TestApp;

#[sqlx::test(migrator = "akasha_db::MIGRATOR")]
async fn me_reports_usage_and_no_limit_by_default(pool: PgPool) {
    let app = TestApp::new(pool);
    let ada = app.user("ada@example.com").await;

    let me = app.send("GET", "/api/v1/me", &ada, None).await;
    assert_eq!(me.json()["storage"]["used_bytes"], 0);
    assert_eq!(me.json()["storage"]["quota_bytes"], json!(null));
    assert_eq!(me.json()["storage"]["remaining_bytes"], json!(null));

    assert_eq!(
        app.upload(&ada, "note.txt", &[b'a'; 10]).await.status,
        StatusCode::CREATED
    );
    let me = app.send("GET", "/api/v1/me", &ada, None).await;
    assert_eq!(me.json()["storage"]["used_bytes"], 10);
    // Bytes the account already holds are counted once.
    assert_eq!(
        app.upload(&ada, "again.txt", &[b'a'; 10]).await.status,
        StatusCode::OK
    );
    let me = app.send("GET", "/api/v1/me", &ada, None).await;
    assert_eq!(me.json()["storage"]["used_bytes"], 10);
}

#[sqlx::test(migrator = "akasha_db::MIGRATOR")]
async fn owner_sets_and_clears_their_own_limit(pool: PgPool) {
    let app = TestApp::new(pool);
    let ada = app.user("ada@example.com").await;
    assert_eq!(
        app.upload(&ada, "note.txt", &[b'a'; 10]).await.status,
        StatusCode::CREATED
    );

    let set = app
        .send(
            "PATCH",
            "/api/v1/me",
            &ada,
            Some(json!({ "storage_quota_bytes": 100 })),
        )
        .await;
    assert_eq!(set.status, StatusCode::OK);
    assert_eq!(set.json()["storage"]["quota_bytes"], 100);
    assert_eq!(set.json()["storage"]["remaining_bytes"], 90);

    // The limit is enforced on upload, not just displayed.
    let over = app.upload(&ada, "big.txt", &[b'b'; 91]).await;
    assert_eq!(over.status, StatusCode::PAYLOAD_TOO_LARGE);
    assert_eq!(over.code(), "quota_exceeded");
    assert_eq!(
        app.upload(&ada, "fits.txt", &[b'b'; 90]).await.status,
        StatusCode::CREATED
    );

    let cleared = app
        .send(
            "PATCH",
            "/api/v1/me",
            &ada,
            Some(json!({ "storage_quota_bytes": null })),
        )
        .await;
    assert_eq!(cleared.json()["storage"]["quota_bytes"], json!(null));
    assert_eq!(cleared.json()["storage"]["remaining_bytes"], json!(null));
    assert_eq!(
        app.upload(&ada, "more.txt", &[b'c'; 500]).await.status,
        StatusCode::CREATED
    );
}

#[sqlx::test(migrator = "akasha_db::MIGRATOR")]
async fn a_limit_below_what_is_stored_blocks_uploads_without_deleting(pool: PgPool) {
    let app = TestApp::new(pool);
    let ada = app.user("ada@example.com").await;
    assert_eq!(
        app.upload(&ada, "note.txt", &[b'a'; 10]).await.status,
        StatusCode::CREATED
    );

    let set = app
        .send(
            "PATCH",
            "/api/v1/me",
            &ada,
            Some(json!({ "storage_quota_bytes": 5 })),
        )
        .await;
    assert_eq!(set.status, StatusCode::OK);
    assert_eq!(set.json()["storage"]["used_bytes"], 10);
    assert_eq!(set.json()["storage"]["quota_bytes"], 5);
    // Over the limit clamps to zero rather than going negative.
    assert_eq!(set.json()["storage"]["remaining_bytes"], 0);

    // Nothing was deleted, but nothing more fits.
    let files = app.send("GET", "/api/v1/files", &ada, None).await;
    assert_eq!(files.json()["items"].as_array().expect("items").len(), 1);
    let over = app.upload(&ada, "more.txt", &[b'b'; 1]).await;
    assert_eq!(over.code(), "quota_exceeded");
}

#[sqlx::test(migrator = "akasha_db::MIGRATOR")]
async fn a_negative_limit_is_refused(pool: PgPool) {
    let app = TestApp::new(pool);
    let ada = app.user("ada@example.com").await;
    let res = app
        .send(
            "PATCH",
            "/api/v1/me",
            &ada,
            Some(json!({ "storage_quota_bytes": -1 })),
        )
        .await;
    assert_eq!(res.status, StatusCode::BAD_REQUEST);
    assert_eq!(res.code(), "bad_request");
}

#[sqlx::test(migrator = "akasha_db::MIGRATOR")]
async fn deleting_a_file_frees_the_space_it_used(pool: PgPool) {
    let app = TestApp::new(pool);
    let ada = app.user("ada@example.com").await;
    let created = app.upload(&ada, "note.txt", &[b'a'; 10]).await;
    let id = created.json()["id"].as_str().expect("id").to_owned();

    let del = app
        .send("DELETE", &format!("/api/v1/files/{id}"), &ada, None)
        .await;
    assert_eq!(del.status, StatusCode::NO_CONTENT);
    let me = app.send("GET", "/api/v1/me", &ada, None).await;
    assert_eq!(me.json()["storage"]["used_bytes"], 0);
}

#[sqlx::test(migrator = "akasha_db::MIGRATOR")]
async fn usage_counts_only_your_own_files(pool: PgPool) {
    let app = TestApp::new(pool);
    let ada = app.user("ada@example.com").await;
    let bob = app.user("bob@example.com").await;
    assert_eq!(
        app.upload(&ada, "note.txt", &[b'a'; 10]).await.status,
        StatusCode::CREATED
    );

    let me = app.send("GET", "/api/v1/me", &bob, None).await;
    assert_eq!(me.json()["storage"]["used_bytes"], 0);
}
