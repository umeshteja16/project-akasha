//! File uploads over HTTP: type detection, limits, names, dedupe.

use std::time::Duration;

use akasha_core::Config;
use akasha_storage::ContentHash;
use axum::{
    body::Body,
    http::{Request, StatusCode, header},
};
use serde_json::json;
use sqlx::PgPool;

mod support;
use support::{BOUNDARY, PDF, TestApp};

#[sqlx::test(migrator = "akasha_db::MIGRATOR")]
async fn upload_detects_type_and_dedupes(pool: PgPool) {
    let app = TestApp::new(pool);
    let ada = app.user("ada@example.com").await;
    let bob = app.user("bob@example.com").await;

    // old magic-bytes script, case B: a real PDF is accepted.
    let first = app.upload(&ada, "valid_real.pdf", PDF).await;
    assert_eq!(first.status, StatusCode::CREATED);
    let file = first.json();
    assert_eq!(file["name"], "valid_real.pdf");
    assert_eq!(file["mime_type"], "application/pdf");
    assert_eq!(file["size_bytes"], PDF.len());
    assert_eq!(file["status"], "pending");
    assert_eq!(file["content_hash"], ContentHash::of(PDF).to_hex());

    let again = app.upload(&ada, "copy.pdf", PDF).await;
    assert_eq!(again.status, StatusCode::OK, "same bytes: existing file");
    assert_eq!(again.json()["id"], file["id"]);
    assert_eq!(again.json()["name"], "valid_real.pdf");

    let other = app.upload(&bob, "mine.pdf", PDF).await;
    assert_eq!(other.status, StatusCode::CREATED);
    assert_ne!(other.json()["id"], file["id"]);
}

#[sqlx::test(migrator = "akasha_db::MIGRATOR")]
async fn upload_rejects_disallowed_and_masquerading_files(pool: PgPool) {
    let app = TestApp::new(pool);
    let ada = app.user("ada@example.com").await;
    let mut elf = vec![0x7f, b'E', b'L', b'F', 2, 1, 1];
    elf.resize(128, 0);

    let cases: [(&str, &[u8]); 6] = [
        // old magic-bytes script, case A.
        (
            "fake_malicious.pdf",
            b"echo 'Malicious binary masquerader!'\n",
        ),
        ("notes.txt", &elf),
        ("setup.exe", b"MZ\x90\0\x03\0\0\0"),
        ("archive.zip", b"PK\x03\x04\x14\0\0\0\x08\0"),
        ("binary.txt", b"looks like text\0but is not"),
        ("latin1.txt", b"caf\xe9"),
    ];
    for (name, bytes) in cases {
        let res = app.upload(&ada, name, bytes).await;
        assert_eq!(res.status, StatusCode::UNSUPPORTED_MEDIA_TYPE, "{name}");
        assert_eq!(res.code(), "unsupported_media_type", "{name}");
    }
    let empty = app.upload(&ada, "empty.txt", b"").await;
    assert_eq!(empty.status, StatusCode::BAD_REQUEST);

    let list = app.send("GET", "/api/v1/files", &ada, None).await;
    assert_eq!(list.json()["items"], json!([]));
    assert_eq!(
        app.storage
            .prune_staging(Duration::ZERO)
            .await
            .expect("prune"),
        0
    );
}

#[sqlx::test(migrator = "akasha_db::MIGRATOR")]
async fn upload_enforces_size_limit_and_quota(pool: PgPool) {
    let config = Config {
        max_upload_mb: 1,
        ..Config::default()
    };
    let app = TestApp::with_config(pool.clone(), config);
    let ada = app.user("ada@example.com").await;

    let big = vec![b'a'; 1024 * 1024 + 1];
    let res = app.upload(&ada, "big.txt", &big).await;
    assert_eq!(res.status, StatusCode::PAYLOAD_TOO_LARGE);
    assert_eq!(res.code(), "payload_too_large");
    let fits = app.upload(&ada, "fits.txt", &big[1..]).await;
    assert_eq!(fits.status, StatusCode::CREATED);

    let me = app.send("GET", "/api/v1/me", &ada, None).await;
    let id = me.json()["id"].as_str().expect("id").parse().expect("uuid");
    akasha_db::files::set_quota(&pool, id, Some(1024 * 1024 + 10))
        .await
        .expect("quota");
    let over = app.upload(&ada, "more.txt", &[b'b'; 11]).await;
    assert_eq!(over.status, StatusCode::PAYLOAD_TOO_LARGE);
    assert_eq!(over.code(), "quota_exceeded");
    let under = app.upload(&ada, "less.txt", &[b'b'; 10]).await;
    assert_eq!(under.status, StatusCode::CREATED);
    // Re-uploading existing bytes costs nothing.
    let dup = app.upload(&ada, "again.txt", &[b'b'; 10]).await;
    assert_eq!(dup.status, StatusCode::OK);
}

#[sqlx::test(migrator = "akasha_db::MIGRATOR")]
async fn upload_sanitises_filenames(pool: PgPool) {
    let app = TestApp::new(pool);
    let ada = app.user("ada@example.com").await;

    // old filename upload script (HTML is not accepted, so as Markdown).
    let special = "Digital Product Design & Development Agency - Significa.md";
    let res = app.upload(&ada, special, b"# special & chars").await;
    assert_eq!(res.status, StatusCode::CREATED);
    assert_eq!(res.json()["name"], special);

    for (raw, expected) in [
        ("../../etc/passwd.txt", "passwd.txt"),
        ("C:\\\\Windows\\\\evil.md", "evil.md"),
        ("..", "unnamed"),
        ("report\u{202e}fdp.txt", "reportfdp.txt"),
    ] {
        let res = app.upload(&ada, raw, raw.as_bytes()).await;
        assert_eq!(res.status, StatusCode::CREATED, "{raw}");
        assert_eq!(res.json()["name"], expected, "{raw}");
    }
}

#[sqlx::test(migrator = "akasha_db::MIGRATOR")]
async fn upload_requires_a_file_field(pool: PgPool) {
    let app = TestApp::new(pool);
    let ada = app.user("ada@example.com").await;
    let req = Request::post("/api/v1/files")
        .header(header::COOKIE, &ada)
        .header(header::CONTENT_TYPE, format!("multipart/form-data; boundary={BOUNDARY}"))
        .body(Body::from(format!(
            "--{BOUNDARY}\r\nContent-Disposition: form-data; name=\"other\"\r\n\r\nx\r\n--{BOUNDARY}--\r\n"
        )))
        .expect("request");
    let res = app.request(req).await;
    assert_eq!(res.status, StatusCode::BAD_REQUEST);
    let json = app
        .send("POST", "/api/v1/files", &ada, Some(json!({})))
        .await;
    assert_eq!(json.status, StatusCode::BAD_REQUEST);
    assert_eq!(json.code(), "bad_request");
}
