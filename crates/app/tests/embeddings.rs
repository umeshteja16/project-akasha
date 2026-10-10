//! The embed_file job end to end with the deterministic hash embedder:
//! upload → extract → embed → ready, idempotency, batching, the model guard and
//! switching models.

mod support;

use akasha::{admin, jobs::kinds::EmbedFile};
use akasha_core::Config;
use akasha_db::embeddings::{self, ModelCheck};
use akasha_ml::{Embedder, catalog, fake::HashEmbedder};
use serde_json::Value;
use sqlx::PgPool;
use support::{TestApp, test_config};
use uuid::Uuid;

fn id(v: &Value) -> String {
    v["id"].as_str().expect("id").to_owned()
}

async fn file(app: &TestApp, cookie: &str, id: &str) -> Value {
    app.send("GET", &format!("/api/v1/files/{id}"), cookie, None)
        .await
        .json()
}

/// `(chunk id, text, embedding)` of a file's chunks, in order.
async fn vectors(pool: &PgPool, file_id: &str) -> Vec<(i64, String, Option<Vec<f32>>)> {
    sqlx::query_as(
        "SELECT id, text, embedding::real[] FROM file_chunks
         WHERE file_id = $1::uuid ORDER BY chunk_index",
    )
    .bind(file_id)
    .fetch_all(pool)
    .await
    .expect("vectors")
}

async fn embed_job(pool: &PgPool, file_id: &str) -> (String, i32, Option<String>) {
    sqlx::query_as(
        "SELECT status, attempts, last_error FROM jobs
         WHERE kind = 'embed_file' AND dedupe_key = $1 ORDER BY created_at DESC LIMIT 1",
    )
    .bind(file_id)
    .fetch_one(pool)
    .await
    .expect("embed job")
}

async fn enqueue_embed(pool: &PgPool, file_id: &str) {
    let mut tx = pool.begin().await.expect("tx");
    akasha_jobs::enqueue(
        &mut tx,
        &EmbedFile {
            file_id: file_id.parse().expect("uuid"),
        },
    )
    .await
    .expect("enqueue");
    tx.commit().await.expect("commit");
}

fn hash() -> HashEmbedder {
    HashEmbedder::new(catalog::embed_model(catalog::HASH_EMBED_MODEL).expect("model"))
}

#[sqlx::test(migrator = "akasha_db::MIGRATOR")]
async fn upload_is_extracted_embedded_and_ready(pool: PgPool) {
    let app = TestApp::new(pool.clone());
    let ada = app.user("ada@example.com").await;
    let fid = id(&app
        .upload(&ada, "notes.txt", b"The aardvark digs at night.")
        .await
        .json());

    assert_eq!(app.run_jobs().await, 3, "extract, embed, then enrich");
    let detail = file(&app, &ada, &fid).await;
    assert_eq!(detail["status"], "ready");
    assert_eq!(detail["processing"]["stage"], "embed");
    assert_eq!(detail["processing"]["state"], "succeeded");

    let rows = vectors(&pool, &fid).await;
    assert_eq!(rows.len(), 1);
    let stored = rows[0].2.clone().expect("embedded");
    assert_eq!(stored.len(), 384);
    let expected = hash().embed_documents(&[&rows[0].1]).expect("embed");
    assert_eq!(stored, expected[0]);
    let dims: i32 = sqlx::query_scalar("SELECT vector_dims(embedding) FROM file_chunks")
        .fetch_one(&pool)
        .await
        .expect("dims");
    assert_eq!(dims, 384);
    let recorded = embeddings::recorded(&pool).await.expect("recorded");
    assert_eq!(recorded.expect("model").name, catalog::HASH_EMBED_MODEL);
}

#[sqlx::test(migrator = "akasha_db::MIGRATOR")]
async fn files_without_text_skip_embedding(pool: PgPool) {
    let config = akasha_core::Config {
        transcribe_enabled: false,
        ..test_config()
    };
    let app = TestApp::with_config(pool.clone(), config.clone());
    let ada = app.user("ada@example.com").await;
    let mp3 = [b"ID3".as_slice(), &[3, 0, 0, 0, 0, 0, 0], &[0u8; 64]].concat();
    let fid = id(&app.upload(&ada, "song.mp3", &mp3).await.json());
    assert_eq!(app.run_jobs_with(&config).await, 1, "no embed job");
    let detail = file(&app, &ada, &fid).await;
    assert_eq!(detail["status"], "ready");
    assert_eq!(detail["processing"]["stage"], "extract");
}

#[sqlx::test(migrator = "akasha_db::MIGRATOR")]
async fn many_chunks_are_embedded_in_batches(pool: PgPool) {
    let app = TestApp::new(pool.clone());
    let ada = app.user("ada@example.com").await;
    // ~150 chunks of up to 2000 characters: several database batches.
    let text: String = (0..30_000).map(|i| format!("word{} ", i % 977)).collect();
    let fid = id(&app.upload(&ada, "big.txt", text.as_bytes()).await.json());
    app.run_jobs().await;
    assert_eq!(file(&app, &ada, &fid).await["status"], "ready");
    let rows = vectors(&pool, &fid).await;
    assert!(rows.len() > 100, "{} chunks", rows.len());
    assert!(
        rows.iter()
            .all(|r| r.2.as_ref().is_some_and(|v| v.len() == 384))
    );
}

#[sqlx::test(migrator = "akasha_db::MIGRATOR")]
async fn embedding_is_idempotent_and_resumes_missing_chunks(pool: PgPool) {
    let app = TestApp::new(pool.clone());
    let ada = app.user("ada@example.com").await;
    let text: String = (0..1200).map(|i| format!("token{i} ")).collect();
    let fid = id(&app.upload(&ada, "t.txt", text.as_bytes()).await.json());
    app.run_jobs().await;
    let rows = vectors(&pool, &fid).await;
    assert!(rows.len() >= 3);

    // A sentinel on chunk 0 shows finished chunks are never rewritten; chunk 1
    // lost its vector (as if a previous attempt died half-way).
    let sentinel = vec![0.5f32; 384];
    sqlx::query("UPDATE file_chunks SET embedding = $2::real[]::vector WHERE id = $1")
        .bind(rows[0].0)
        .bind(&sentinel)
        .execute(&pool)
        .await
        .expect("sentinel");
    sqlx::query("UPDATE file_chunks SET embedding = NULL WHERE id = $1")
        .bind(rows[1].0)
        .execute(&pool)
        .await
        .expect("clear");

    enqueue_embed(&pool, &fid).await;
    assert_eq!(app.run_jobs().await, 1);
    enqueue_embed(&pool, &fid).await;
    assert_eq!(app.run_jobs().await, 1, "a duplicate delivery is harmless");

    let after = vectors(&pool, &fid).await;
    assert_eq!(after[0].2.as_ref(), Some(&sentinel));
    assert_eq!(after[1].2, rows[1].2, "missing vector filled in");
    assert_eq!(after[2].2, rows[2].2);
    assert_eq!(file(&app, &ada, &fid).await["status"], "ready");
}

#[sqlx::test(migrator = "akasha_db::MIGRATOR")]
async fn vectors_from_another_model_are_never_mixed(pool: PgPool) {
    let app = TestApp::new(pool.clone());
    let ada = app.user("ada@example.com").await;
    // The database was indexed with another 384-dimension model.
    sqlx::query("INSERT INTO embedding_model (name, dim) VALUES ('multilingual-e5-small', 384)")
        .execute(&pool)
        .await
        .expect("record");
    let fid = id(&app.upload(&ada, "a.txt", b"some text").await.json());
    app.run_jobs().await;

    let (status, _, error) = embed_job(&pool, &fid).await;
    assert_eq!(status, "failed", "retried later");
    assert!(error.expect("error").contains("akasha reembed"));
    assert_eq!(file(&app, &ada, &fid).await["status"], "processing");
    assert!(vectors(&pool, &fid).await.iter().all(|r| r.2.is_none()));

    // Startup refuses the same configuration.
    let err = admin::check_embedding_model(&pool, &test_config())
        .await
        .expect_err("mismatch");
    assert!(err.to_string().contains("akasha reembed"), "{err}");
}

#[sqlx::test(migrator = "akasha_db::MIGRATOR")]
async fn startup_check_records_the_model_and_rejects_other_dimensions(pool: PgPool) {
    let config = Config {
        embed_model: "bge-base-en-v1.5".into(),
        ..test_config()
    };
    let err = admin::check_embedding_model(&pool, &config)
        .await
        .expect_err("768 vs 384");
    assert!(err.to_string().contains("384"), "{err}");
    assert!(embeddings::recorded(&pool).await.expect("read").is_none());

    admin::check_embedding_model(&pool, &test_config())
        .await
        .expect("fresh database records the configured model");
    admin::check_embedding_model(&pool, &test_config())
        .await
        .expect("and accepts it again");

    let unknown = Config {
        embed_model: "no-such-model".into(),
        ..test_config()
    };
    assert!(admin::check_embedding_model(&pool, &unknown).await.is_err());
}

#[sqlx::test(migrator = "akasha_db::MIGRATOR")]
async fn switching_models_resizes_the_column_and_reembeds_everything(pool: PgPool) {
    let app = TestApp::new(pool.clone());
    let ada = app.user("ada@example.com").await;
    let fid = id(&app.upload(&ada, "a.txt", b"some text").await.json());
    app.run_jobs().await;

    // To a 768-dimension model: column re-typed, vectors dropped.
    let mut tx = pool.begin().await.expect("tx");
    let files = embeddings::reset(&mut tx, "bge-base-en-v1.5", 768)
        .await
        .expect("reset");
    tx.commit().await.expect("commit");
    assert_eq!(files, vec![fid.parse::<Uuid>().expect("uuid")]);
    let mut conn = pool.acquire().await.expect("conn");
    assert_eq!(
        embeddings::column_dim(&mut conn).await.expect("dim"),
        Some(768)
    );
    assert_eq!(
        embeddings::check_model(&pool, catalog::HASH_EMBED_MODEL, 384)
            .await
            .expect("check"),
        ModelCheck::Mismatch(embeddings::RecordedModel {
            name: "bge-base-en-v1.5".into(),
            dim: 768
        })
    );

    // And back, the way `akasha reembed` does it: every file queued again.
    let mut tx = pool.begin().await.expect("tx");
    let files = embeddings::reset(&mut tx, catalog::HASH_EMBED_MODEL, 384)
        .await
        .expect("reset");
    for file_id in files {
        akasha_jobs::enqueue(&mut tx, &EmbedFile { file_id })
            .await
            .expect("enqueue");
    }
    tx.commit().await.expect("commit");
    assert_eq!(file(&app, &ada, &fid).await["status"], "processing");
    assert!(vectors(&pool, &fid).await.iter().all(|r| r.2.is_none()));

    // Embed, then an enrich job that finds the summary current (no model call).
    assert_eq!(app.run_jobs().await, 2);
    assert_eq!(file(&app, &ada, &fid).await["status"], "ready");
    assert!(vectors(&pool, &fid).await.iter().all(|r| r.2.is_some()));
    let indexed: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM pg_indexes WHERE indexname = 'file_chunks_embedding_idx')",
    )
    .fetch_one(&pool)
    .await
    .expect("index");
    assert!(indexed, "HNSW index rebuilt");
}

#[sqlx::test(migrator = "akasha_db::MIGRATOR")]
async fn unavailable_models_fail_the_file_only_on_the_last_attempt(pool: PgPool) {
    let app = TestApp::new(pool.clone());
    let ada = app.user("ada@example.com").await;
    let fid = id(&app.upload(&ada, "a.txt", b"some text").await.json());
    // A real model, no files, downloads disabled: a retryable error.
    let models = tempfile::tempdir().expect("tempdir");
    let config = Config {
        embed_model: "all-minilm-l6-v2".into(),
        models_dir: models.path().to_string_lossy().into_owned(),
        ..test_config()
    };

    app.run_jobs_with(&config).await;
    let (status, attempts, error) = embed_job(&pool, &fid).await;
    assert_eq!((status.as_str(), attempts), ("failed", 1));
    assert!(error.expect("error").contains("downloads are disabled"));
    assert_eq!(file(&app, &ada, &fid).await["status"], "processing");

    sqlx::query("UPDATE jobs SET run_at = now(), max_attempts = 2 WHERE kind = 'embed_file'")
        .execute(&pool)
        .await
        .expect("make due");
    app.run_jobs_with(&config).await;
    assert_eq!(embed_job(&pool, &fid).await.0, "dead");
    let detail = file(&app, &ada, &fid).await;
    assert_eq!(detail["status"], "failed");
    assert!(
        detail["error"]
            .as_str()
            .expect("error")
            .contains("semantic indexing failed")
    );
}
