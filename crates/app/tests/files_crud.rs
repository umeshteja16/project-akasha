//! File listing, updates, downloads, deletes and isolation over HTTP.

use axum::{
    body::Body,
    http::{Request, StatusCode, header},
};
use serde_json::{Value, json};
use sqlx::PgPool;

mod support;
use support::{PDF, PW, TestApp};

#[sqlx::test(migrator = "akasha_db::MIGRATOR")]
async fn list_paginates_and_filters(pool: PgPool) {
    let app = TestApp::new(pool);
    let ada = app.user("ada@example.com").await;
    for i in 0..3 {
        let res = app
            .upload(&ada, &format!("n{i}.txt"), format!("note {i}").as_bytes())
            .await;
        assert_eq!(res.status, StatusCode::CREATED);
    }
    let pdf = app.upload(&ada, "doc.pdf", PDF).await.json();

    let page1 = app
        .send("GET", "/api/v1/files?limit=3", &ada, None)
        .await
        .json();
    let names: Vec<&str> = page1["items"]
        .as_array()
        .expect("items")
        .iter()
        .map(|f| f["name"].as_str().expect("name"))
        .collect();
    assert_eq!(names, ["doc.pdf", "n2.txt", "n1.txt"]);
    let cursor = page1["next_cursor"].as_str().expect("cursor");
    let page2 = app
        .send(
            "GET",
            &format!("/api/v1/files?limit=3&cursor={cursor}"),
            &ada,
            None,
        )
        .await
        .json();
    assert_eq!(page2["items"][0]["name"], "n0.txt");
    assert_eq!(page2["next_cursor"], Value::Null);

    let path = format!("/api/v1/files/{}", pdf["id"].as_str().expect("id"));
    let patched = app
        .send(
            "PATCH",
            &path,
            &ada,
            Some(json!({ "is_pinned": true, "tags": [" Work ", "work", "Q3"] })),
        )
        .await;
    assert_eq!(patched.status, StatusCode::OK);
    assert_eq!(patched.json()["tags"], json!(["work", "q3"]));

    for query in [
        "category=pdf",
        "pinned=true",
        "tag=Work",
        "status=pending&category=pdf",
    ] {
        let res = app
            .send("GET", &format!("/api/v1/files?{query}"), &ada, None)
            .await
            .json();
        assert_eq!(res["items"].as_array().map(Vec::len), Some(1), "{query}");
    }
    let text = app
        .send("GET", "/api/v1/files?category=text", &ada, None)
        .await
        .json();
    assert_eq!(text["items"].as_array().map(Vec::len), Some(3));

    for bad in [
        "limit=0",
        "limit=201",
        "cursor=nope",
        "status=done",
        "category=exe",
    ] {
        let res = app
            .send("GET", &format!("/api/v1/files?{bad}"), &ada, None)
            .await;
        assert_eq!(res.status, StatusCode::BAD_REQUEST, "{bad}");
        assert_eq!(res.code(), "bad_request", "{bad}");
    }
}

#[sqlx::test(migrator = "akasha_db::MIGRATOR")]
async fn update_validates_input(pool: PgPool) {
    let app = TestApp::new(pool);
    let ada = app.user("ada@example.com").await;
    let file = app.upload(&ada, "a.txt", b"a").await.json();
    let path = format!("/api/v1/files/{}", file["id"].as_str().expect("id"));

    let renamed = app
        .send("PATCH", &path, &ada, Some(json!({ "name": "../b.md" })))
        .await;
    assert_eq!(renamed.json()["name"], "b.md");
    for body in [json!({}), json!({ "name": "  " }), json!({ "tags": [""] })] {
        let res = app.send("PATCH", &path, &ada, Some(body.clone())).await;
        assert_eq!(res.status, StatusCode::BAD_REQUEST, "{body}");
    }
}

#[sqlx::test(migrator = "akasha_db::MIGRATOR")]
async fn other_users_files_are_not_found(pool: PgPool) {
    let app = TestApp::new(pool);
    let ada = app.user("ada@example.com").await;
    let eve = app.user("eve@example.com").await;
    let file = app.upload(&ada, "secret.txt", b"secret").await.json();
    let id = file["id"].as_str().expect("id");
    let path = format!("/api/v1/files/{id}");

    for (method, path, body) in [
        ("GET", path.clone(), None),
        ("PATCH", path.clone(), Some(json!({ "name": "pwned.txt" }))),
        ("DELETE", path.clone(), None),
        ("GET", format!("{path}/download"), None),
    ] {
        let res = app.send(method, &path, &eve, body).await;
        assert_eq!(res.status, StatusCode::NOT_FOUND, "{method} {path}");
        assert_eq!(res.code(), "not_found");
    }
    let bulk = app
        .send(
            "POST",
            "/api/v1/files/bulk-delete",
            &eve,
            Some(json!({ "ids": [id] })),
        )
        .await;
    assert_eq!(bulk.json()["deleted"], json!([]));
    let list = app.send("GET", "/api/v1/files", &eve, None).await.json();
    assert_eq!(list["items"], json!([]));

    let still = app.send("GET", &path, &ada, None).await;
    assert_eq!(still.status, StatusCode::OK);
    assert_eq!(still.json()["name"], "secret.txt");

    let anon = app
        .request(
            Request::get("/api/v1/files")
                .body(Body::empty())
                .expect("req"),
        )
        .await;
    assert_eq!(anon.status, StatusCode::UNAUTHORIZED);
}

#[sqlx::test(migrator = "akasha_db::MIGRATOR")]
async fn download_streams_bytes_with_safe_headers(pool: PgPool) {
    let app = TestApp::new(pool);
    let ada = app.user("ada@example.com").await;
    let body = "naïve \"notes\"".as_bytes();
    let file = app.upload(&ada, "draft.md", body).await.json();
    let id = file["id"].as_str().expect("id");
    let rename = json!({ "name": "résumé \"v2\".md" });
    let renamed = app
        .send("PATCH", &format!("/api/v1/files/{id}"), &ada, Some(rename))
        .await;
    assert_eq!(renamed.status, StatusCode::OK);
    let path = format!("/api/v1/files/{id}/download");

    let res = app.send("GET", &path, &ada, None).await;
    assert_eq!(res.status, StatusCode::OK);
    assert_eq!(res.bytes, body);
    let h = |name| res.headers[name].to_str().expect("header").to_owned();
    assert_eq!(h(header::CONTENT_TYPE), "text/markdown; charset=utf-8");
    assert_eq!(h(header::CONTENT_LENGTH), body.len().to_string());
    assert_eq!(h(header::X_CONTENT_TYPE_OPTIONS), "nosniff");
    assert_eq!(
        h(header::CONTENT_DISPOSITION),
        "attachment; filename=\"r_sum_ _v2_.md\"; filename*=UTF-8''r%C3%A9sum%C3%A9%20%22v2%22.md"
    );
}

#[sqlx::test(migrator = "akasha_db::MIGRATOR")]
async fn blobs_are_deleted_only_when_unreferenced(pool: PgPool) {
    let app = TestApp::new(pool);
    let ada = app.user("ada@example.com").await;
    let bob = app.user("bob@example.com").await;
    let a = app.upload(&ada, "a.pdf", PDF).await.json();
    let b = app.upload(&bob, "b.pdf", PDF).await.json();
    let hash = a["content_hash"].as_str().expect("hash").to_owned();
    assert!(app.blob_exists(&hash).await);

    let del = app
        .send(
            "DELETE",
            &format!("/api/v1/files/{}", a["id"].as_str().expect("id")),
            &ada,
            None,
        )
        .await;
    assert_eq!(del.status, StatusCode::NO_CONTENT);
    assert!(app.blob_exists(&hash).await, "bob still uses the blob");
    let dl = format!("/api/v1/files/{}/download", b["id"].as_str().expect("id"));
    assert_eq!(app.send("GET", &dl, &bob, None).await.bytes, PDF);

    let bulk = app
        .send(
            "POST",
            "/api/v1/files/bulk-delete",
            &bob,
            Some(json!({ "ids": [b["id"], b["id"]] })),
        )
        .await;
    assert_eq!(bulk.json()["deleted"], json!([b["id"]]));
    assert!(!app.blob_exists(&hash).await, "last reference gone");

    let gone = app
        .send(
            "DELETE",
            &format!("/api/v1/files/{}", a["id"].as_str().expect("id")),
            &ada,
            None,
        )
        .await;
    assert_eq!(gone.status, StatusCode::NOT_FOUND);
}

#[sqlx::test(migrator = "akasha_db::MIGRATOR")]
async fn deleting_the_account_releases_blobs(pool: PgPool) {
    let app = TestApp::new(pool);
    let ada = app.user("ada@example.com").await;
    let bob = app.user("bob@example.com").await;
    let mine = app.upload(&ada, "mine.txt", b"only ada").await.json();
    let shared = app.upload(&ada, "shared.pdf", PDF).await.json();
    app.upload(&bob, "shared.pdf", PDF).await;

    let res = app
        .send(
            "DELETE",
            "/api/v1/me",
            &ada,
            Some(json!({ "password": PW })),
        )
        .await;
    assert_eq!(res.status, StatusCode::NO_CONTENT);
    assert!(
        !app.blob_exists(mine["content_hash"].as_str().expect("hash"))
            .await
    );
    assert!(
        app.blob_exists(shared["content_hash"].as_str().expect("hash"))
            .await
    );
}
