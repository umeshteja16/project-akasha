//! The extract_file job end to end: upload → worker → chunks + status, and
//! `GET /api/v1/files/{id}/extraction`.

mod support;

use akasha_core::Config;
use axum::http::StatusCode;
use serde_json::Value;
use sqlx::PgPool;
use support::{PDF, TestApp, test_config};
use uuid::Uuid;

const THREE_PAGES: &[u8] = include_bytes!("../../ingest/tests/fixtures/three-pages.pdf");
const OCR_PNG: &[u8] = include_bytes!("../../ingest/tests/fixtures/hello-ocr.png");

/// `(chunk_index, page, char_start, char_end, text)` of a file's chunks, in order.
type ChunkRow = (i32, Option<i32>, i32, i32, String);

async fn chunks(pool: &PgPool, file_id: &str) -> Vec<ChunkRow> {
    sqlx::query_as(
        "SELECT chunk_index, page, char_start, char_end, text FROM file_chunks
         WHERE file_id = $1::uuid ORDER BY chunk_index",
    )
    .bind(file_id)
    .fetch_all(pool)
    .await
    .expect("chunks")
}

async fn job_state(pool: &PgPool, file_id: &str) -> (String, i32, Option<String>) {
    sqlx::query_as(
        "SELECT status, attempts, last_error FROM jobs
         WHERE kind = 'extract_file' AND dedupe_key = $1 ORDER BY created_at DESC LIMIT 1",
    )
    .bind(file_id)
    .fetch_one(pool)
    .await
    .expect("job")
}

fn id(v: &Value) -> String {
    v["id"].as_str().expect("id").to_owned()
}

async fn file(app: &TestApp, cookie: &str, id: &str) -> Value {
    app.send("GET", &format!("/api/v1/files/{id}"), cookie, None)
        .await
        .json()
}

#[sqlx::test(migrator = "akasha_db::MIGRATOR")]
async fn text_upload_is_extracted_chunked_and_ready(pool: PgPool) {
    let app = TestApp::new(pool.clone());
    let ada = app.user("ada@example.com").await;
    let body = "# Notes\n\nThe **aardvark** digs at night.\n";
    let f = app.upload(&ada, "notes.md", body.as_bytes()).await.json();
    let fid = id(&f);
    assert_eq!(file(&app, &ada, &fid).await["status"], "pending");

    assert_eq!(app.run_jobs().await, 3, "extract, embed, then enrich");
    let detail = file(&app, &ada, &fid).await;
    assert_eq!(detail["status"], "ready");
    assert_eq!(detail["error"], Value::Null);
    assert_eq!(detail["processing"]["state"], "succeeded");

    let rows = chunks(&pool, &fid).await;
    assert_eq!(
        rows,
        vec![(
            0,
            None,
            0,
            34,
            "Notes\n\nThe aardvark digs at night.".to_owned()
        )]
    );
    let owner: Uuid = sqlx::query_scalar("SELECT owner_id FROM file_chunks LIMIT 1")
        .fetch_one(&pool)
        .await
        .expect("owner");
    let file_owner: Uuid = sqlx::query_scalar("SELECT owner_id FROM files")
        .fetch_one(&pool)
        .await
        .expect("owner");
    assert_eq!(
        owner, file_owner,
        "chunks carry the owner for filtered search"
    );
    let hit: bool = sqlx::query_scalar(
        "SELECT tsv @@ plainto_tsquery('english', 'aardvarks dig') FROM file_chunks",
    )
    .fetch_one(&pool)
    .await
    .expect("fts");
    assert!(hit, "stemmed full-text match");

    let res = app
        .send(
            "GET",
            &format!("/api/v1/files/{fid}/extraction"),
            &ada,
            None,
        )
        .await;
    assert_eq!(res.status, StatusCode::OK);
    let e = res.json();
    assert_eq!(e["extractor"], "markdown");
    assert_eq!(e["text"], "Notes\n\nThe aardvark digs at night.");
    assert_eq!(e["char_count"], 34);
    assert_eq!(e["chunk_count"], 1);
    assert_eq!(e["page_count"], Value::Null);
    assert_eq!(e["next_offset"], Value::Null);
    assert_eq!(e["pages"], serde_json::json!([]));
}

#[sqlx::test(migrator = "akasha_db::MIGRATOR")]
async fn pdf_pages_are_kept_for_citations(pool: PgPool) {
    let app = TestApp::new(pool.clone());
    let ada = app.user("ada@example.com").await;
    let fid = id(&app.upload(&ada, "three.pdf", THREE_PAGES).await.json());
    app.run_jobs().await;
    assert_eq!(file(&app, &ada, &fid).await["status"], "ready");

    let rows = chunks(&pool, &fid).await;
    let pages: Vec<Option<i32>> = rows.iter().map(|r| r.1).collect();
    assert_eq!(pages, vec![Some(1), Some(3)], "blank page 2 has no chunk");
    assert_eq!(rows[1].4, "Page three is about zebras.");

    let e = app
        .send(
            "GET",
            &format!("/api/v1/files/{fid}/extraction"),
            &ada,
            None,
        )
        .await
        .json();
    assert_eq!(e["extractor"], "pdf");
    assert_eq!(e["page_count"], 3);
    assert_eq!(e["pages"][2]["number"], 3);
    assert_eq!(e["pages"][2]["source"], "text");
    assert_eq!(e["pages"][2]["char_start"], rows[1].2);

    // Paging through the text.
    let window = app
        .send(
            "GET",
            &format!("/api/v1/files/{fid}/extraction?offset=58&limit=10"),
            &ada,
            None,
        )
        .await
        .json();
    assert_eq!(window["text"], "Page three");
    assert_eq!(window["offset"], 58);
    assert_eq!(window["next_offset"], 68);
    let bad = app
        .send(
            "GET",
            &format!("/api/v1/files/{fid}/extraction?limit=0"),
            &ada,
            None,
        )
        .await;
    assert_eq!(bad.status, StatusCode::BAD_REQUEST);
}

#[sqlx::test(migrator = "akasha_db::MIGRATOR")]
async fn reextraction_replaces_chunks_idempotently(pool: PgPool) {
    let app = TestApp::new(pool.clone());
    let ada = app.user("ada@example.com").await;
    let text = "Sentence about retrieval and citations. ".repeat(200);
    let fid = id(&app.upload(&ada, "long.txt", text.as_bytes()).await.json());
    app.run_jobs().await;
    let first = chunks(&pool, &fid).await;
    assert!(first.len() > 2, "long text is split");

    // Reindex, plus a duplicate delivery of the same job.
    let res = app
        .send("POST", &format!("/api/v1/files/{fid}/reindex"), &ada, None)
        .await;
    assert_eq!(res.status, StatusCode::ACCEPTED);
    app.run_jobs().await;
    sqlx::query("INSERT INTO jobs (kind, payload) VALUES ('extract_file', jsonb_build_object('file_id', $1::text))")
        .bind(&fid)
        .execute(&pool)
        .await
        .expect("duplicate job");
    app.run_jobs().await;

    assert_eq!(chunks(&pool, &fid).await, first);
    let extractions: i64 = sqlx::query_scalar("SELECT count(*) FROM file_extractions")
        .fetch_one(&pool)
        .await
        .expect("count");
    assert_eq!(extractions, 1);
    assert_eq!(file(&app, &ada, &fid).await["status"], "ready");
}

#[sqlx::test(migrator = "akasha_db::MIGRATOR")]
async fn corrupt_pdf_fails_with_a_clear_message(pool: PgPool) {
    let app = TestApp::new(pool.clone());
    let ada = app.user("ada@example.com").await;
    let fid = id(&app.upload(&ada, "broken.pdf", PDF).await.json());
    app.run_jobs().await;

    let detail = file(&app, &ada, &fid).await;
    assert_eq!(detail["status"], "failed");
    assert_eq!(
        detail["error"],
        "this PDF could not be read; it may be damaged"
    );
    let (status, attempts, _) = job_state(&pool, &fid).await;
    assert_eq!((status.as_str(), attempts), ("dead", 1), "not retried");

    let res = app
        .send(
            "GET",
            &format!("/api/v1/files/{fid}/extraction"),
            &ada,
            None,
        )
        .await;
    assert_eq!(res.status, StatusCode::NOT_FOUND);
    assert!(
        res.json()["error"]["message"]
            .as_str()
            .expect("message")
            .contains("not been extracted")
    );
}

#[sqlx::test(migrator = "akasha_db::MIGRATOR")]
async fn media_is_ready_without_text_and_images_note_disabled_ocr(pool: PgPool) {
    let config = Config {
        transcribe_enabled: false,
        ..test_config()
    };
    let app = TestApp::with_config(pool.clone(), config.clone());
    let ada = app.user("ada@example.com").await;
    let mut mp3 = b"ID3\x03\x00\x00\x00\x00\x00\x0a".to_vec();
    mp3.extend_from_slice(&[0u8; 64]);
    let audio = id(&app.upload(&ada, "song.mp3", &mp3).await.json());
    let image = id(&app.upload(&ada, "scan.png", OCR_PNG).await.json());
    app.run_jobs_with(&config).await;

    for (fid, note) in [
        (&audio, "transcription is turned off"),
        (&image, "OCR is disabled"),
    ] {
        assert_eq!(file(&app, &ada, fid).await["status"], "ready");
        let e = app
            .send(
                "GET",
                &format!("/api/v1/files/{fid}/extraction"),
                &ada,
                None,
            )
            .await
            .json();
        assert_eq!(e["extractor"], "none");
        assert_eq!(e["text"], "");
        assert_eq!(e["chunk_count"], 0);
        assert!(e["notes"][0].as_str().expect("note").contains(note), "{e}");
    }
}

#[sqlx::test(migrator = "akasha_db::MIGRATOR")]
async fn retryable_failures_mark_the_file_failed_only_on_the_last_attempt(pool: PgPool) {
    let app = TestApp::new(pool.clone());
    let ada = app.user("ada@example.com").await;
    let fid = id(&app.upload(&ada, "scan.png", OCR_PNG).await.json());
    // OCR on, but no models and downloads disabled: a retryable error.
    let models = tempfile::tempdir().expect("tempdir");
    let config = Config {
        ocr_enabled: true,
        models_dir: models.path().to_string_lossy().into_owned(),
        ocr_models_url: String::new(),
        ..test_config()
    };

    app.run_jobs_with(&config).await;
    let (status, attempts, error) = job_state(&pool, &fid).await;
    assert_eq!((status.as_str(), attempts), ("failed", 1));
    assert!(error.expect("error").contains("OCR models"));
    let detail = file(&app, &ada, &fid).await;
    assert_eq!(detail["status"], "processing", "still being retried");
    assert_eq!(detail["error"], Value::Null);

    // Make the next attempt the last one.
    sqlx::query("UPDATE jobs SET run_at = now(), max_attempts = 2 WHERE kind = 'extract_file'")
        .execute(&pool)
        .await
        .expect("make due");
    app.run_jobs_with(&config).await;
    assert_eq!(job_state(&pool, &fid).await.0, "dead");
    let detail = file(&app, &ada, &fid).await;
    assert_eq!(detail["status"], "failed");
    assert_eq!(
        detail["error"],
        "text recognition is unavailable right now; try reindexing later"
    );
}

#[sqlx::test(migrator = "akasha_db::MIGRATOR")]
async fn deleted_files_and_other_owners(pool: PgPool) {
    let app = TestApp::new(pool.clone());
    let ada = app.user("ada@example.com").await;
    let bob = app.user("bob@example.com").await;
    let kept = id(&app.upload(&ada, "kept.txt", b"kept").await.json());
    let gone = id(&app.upload(&ada, "gone.txt", b"gone").await.json());
    // Deleted after the job was queued (bypassing the route keeps the job).
    sqlx::query("DELETE FROM files WHERE id = $1::uuid")
        .bind(&gone)
        .execute(&pool)
        .await
        .expect("delete");
    app.run_jobs().await;
    assert_eq!(job_state(&pool, &gone).await.0, "succeeded", "no-op");
    assert!(chunks(&pool, &gone).await.is_empty());

    let path = format!("/api/v1/files/{kept}/extraction");
    assert_eq!(
        app.send("GET", &path, &ada, None).await.status,
        StatusCode::OK
    );
    let res = app.send("GET", &path, &bob, None).await;
    assert_eq!(res.status, StatusCode::NOT_FOUND);
    assert_eq!(res.code(), "not_found");

    // Deleting the file removes its chunks and extraction.
    app.send("DELETE", &format!("/api/v1/files/{kept}"), &ada, None)
        .await;
    let left: i64 = sqlx::query_scalar(
        "SELECT (SELECT count(*) FROM file_chunks) + (SELECT count(*) FROM file_extractions)",
    )
    .fetch_one(&pool)
    .await
    .expect("count");
    assert_eq!(left, 0);
}
