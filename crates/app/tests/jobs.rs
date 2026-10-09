//! Background jobs wired into the file routes: extraction is queued with uploads,
//! blobs are released by jobs after deletes commit, and the periodic sweeps.

mod support;

use akasha::jobs::kinds::{
    DeleteBlobIfUnreferenced, ExtractFile, PruneSessions, PruneStaging, SweepOrphanBlobs,
};
use akasha_jobs::{Job, enqueue};
use axum::http::StatusCode;
use serde_json::Value;
use sqlx::PgPool;
use support::{PDF, TestApp};

async fn jobs_of(pool: &PgPool, kind: &str) -> Vec<(String, String)> {
    sqlx::query_as(
        "SELECT coalesce(dedupe_key, ''), status FROM jobs WHERE kind = $1 ORDER BY created_at",
    )
    .bind(kind)
    .fetch_all(pool)
    .await
    .expect("jobs")
}

async fn put<J: Job>(pool: &PgPool, job: &J) {
    let mut conn = pool.acquire().await.expect("conn");
    enqueue(&mut conn, job).await.expect("enqueue");
}

fn id(v: &Value) -> &str {
    v["id"].as_str().expect("id")
}

#[sqlx::test(migrator = "akasha_db::MIGRATOR")]
async fn uploads_queue_extraction_and_files_stay_pending(pool: PgPool) {
    let app = TestApp::new(pool.clone());
    let ada = app.user("ada@example.com").await;
    let file = app.upload(&ada, "a.pdf", PDF).await.json();
    let again = app.upload(&ada, "a.pdf", PDF).await;
    assert_eq!(again.status, StatusCode::OK);

    let queued = jobs_of(&pool, ExtractFile::KIND).await;
    assert_eq!(queued, vec![(id(&file).to_owned(), "queued".to_owned())]);

    // No extraction handler yet: the worker leaves the job alone.
    app.run_jobs().await;
    let detail = app
        .send("GET", &format!("/api/v1/files/{}", id(&file)), &ada, None)
        .await
        .json();
    assert_eq!(detail["status"], "pending");
    assert_eq!(detail["processing"]["state"], "queued");
    assert_eq!(detail["processing"]["attempts"], 0);
    assert_eq!(jobs_of(&pool, ExtractFile::KIND).await[0].1, "queued");
}

#[sqlx::test(migrator = "akasha_db::MIGRATOR")]
async fn reindex_requeues_once_and_checks_ownership(pool: PgPool) {
    let app = TestApp::new(pool.clone());
    let ada = app.user("ada@example.com").await;
    let bob = app.user("bob@example.com").await;
    let file = app.upload(&ada, "a.pdf", PDF).await.json();
    let path = format!("/api/v1/files/{}/reindex", id(&file));

    sqlx::query("UPDATE files SET status = 'failed', error = 'boom'")
        .execute(&pool)
        .await
        .expect("simulate failure");
    sqlx::query("UPDATE jobs SET status = 'dead', finished_at = now() WHERE kind = 'extract_file'")
        .execute(&pool)
        .await
        .expect("simulate dead job");

    let res = app.send("POST", &path, &ada, None).await;
    assert_eq!(res.status, StatusCode::ACCEPTED);
    let body = res.json();
    assert_eq!(body["status"], "pending");
    assert_eq!(body["error"], Value::Null);
    assert_eq!(body["processing"]["state"], "queued");

    app.send("POST", &path, &ada, None).await;
    let states: Vec<String> = jobs_of(&pool, ExtractFile::KIND)
        .await
        .into_iter()
        .map(|(_, s)| s)
        .collect();
    assert_eq!(states, ["dead", "queued"], "second reindex deduplicated");

    let res = app.send("POST", &path, &bob, None).await;
    assert_eq!(res.status, StatusCode::NOT_FOUND);
}

#[sqlx::test(migrator = "akasha_db::MIGRATOR")]
async fn delete_queues_blob_release_in_the_same_transaction(pool: PgPool) {
    let app = TestApp::new(pool.clone());
    let ada = app.user("ada@example.com").await;
    let file = app.upload(&ada, "a.pdf", PDF).await.json();
    let hash = file["content_hash"].as_str().expect("hash").to_owned();

    let del = app
        .send(
            "DELETE",
            &format!("/api/v1/files/{}", id(&file)),
            &ada,
            None,
        )
        .await;
    assert_eq!(del.status, StatusCode::NO_CONTENT);
    assert_eq!(
        jobs_of(&pool, DeleteBlobIfUnreferenced::KIND).await,
        vec![(hash.clone(), "queued".to_owned())]
    );
    assert!(app.blob_exists(&hash).await, "untouched until the job runs");
    assert!(app.run_jobs().await >= 1);
    assert!(!app.blob_exists(&hash).await);
    assert_eq!(
        jobs_of(&pool, DeleteBlobIfUnreferenced::KIND).await[0].1,
        "succeeded"
    );
}

#[sqlx::test(migrator = "akasha_db::MIGRATOR")]
async fn blob_job_rechecks_references(pool: PgPool) {
    let app = TestApp::new(pool.clone());
    let ada = app.user("ada@example.com").await;
    let file = app.upload(&ada, "a.pdf", PDF).await.json();
    let hash = file["content_hash"].as_str().expect("hash").to_owned();

    // A stale job (e.g. the file was re-uploaded after a delete) must not delete a
    // blob that is referenced again.
    put(&pool, &DeleteBlobIfUnreferenced { hash: hash.clone() }).await;
    app.run_jobs().await;
    assert!(app.blob_exists(&hash).await);
    let dl = app
        .send(
            "GET",
            &format!("/api/v1/files/{}/download", id(&file)),
            &ada,
            None,
        )
        .await;
    assert_eq!(dl.bytes, PDF);

    // A garbage hash is dead-lettered, not retried forever.
    put(
        &pool,
        &DeleteBlobIfUnreferenced {
            hash: "nope".into(),
        },
    )
    .await;
    app.run_jobs().await;
    let jobs = jobs_of(&pool, DeleteBlobIfUnreferenced::KIND).await;
    assert_eq!(jobs[1], ("nope".to_owned(), "dead".to_owned()));
}

#[sqlx::test(migrator = "akasha_db::MIGRATOR")]
async fn orphan_sweep_deletes_only_unreferenced_blobs(pool: PgPool) {
    let app = TestApp::new(pool.clone());
    let ada = app.user("ada@example.com").await;
    let kept = app.upload(&ada, "a.pdf", PDF).await.json();
    let kept = kept["content_hash"].as_str().expect("hash").to_owned();
    // Bytes left behind by an upload whose transaction never committed.
    let orphan = app
        .storage
        .put_bytes(&b"orphaned bytes"[..])
        .await
        .expect("put")
        .hash
        .to_hex();

    put(&pool, &SweepOrphanBlobs {}).await;
    app.run_jobs().await;
    assert!(app.blob_exists(&kept).await);
    assert!(!app.blob_exists(&orphan).await);
}

#[sqlx::test(migrator = "akasha_db::MIGRATOR")]
async fn housekeeping_jobs_succeed(pool: PgPool) {
    let app = TestApp::new(pool.clone());
    sqlx::query("INSERT INTO users (email, password_hash) VALUES ('old@example.com', 'x')")
        .execute(&pool)
        .await
        .expect("user");
    sqlx::query(
        "INSERT INTO sessions (user_id, token_hash, expires_at)
         SELECT id, '\\x00', now() - interval '1 day' FROM users",
    )
    .execute(&pool)
    .await
    .expect("expired session");
    put(&pool, &PruneSessions {}).await;
    put(&pool, &PruneStaging {}).await;
    assert_eq!(app.run_jobs().await, 2);
    let sessions: i64 = sqlx::query_scalar("SELECT count(*) FROM sessions")
        .fetch_one(&pool)
        .await
        .expect("count");
    assert_eq!(sessions, 0);
    for kind in [PruneSessions::KIND, PruneStaging::KIND] {
        assert_eq!(jobs_of(&pool, kind).await[0].1, "succeeded", "{kind}");
    }
    let schedules = akasha::jobs::schedules().expect("schedules");
    assert_eq!(schedules.len(), 4);
}
