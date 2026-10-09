//! Browsing the library: sort orders, the tag list and inline previews.

use axum::http::{StatusCode, header};
use serde_json::{Value, json};
use sqlx::PgPool;

mod support;
use support::{PDF, TestApp};

fn names(page: &Value) -> Vec<String> {
    page["items"]
        .as_array()
        .expect("items")
        .iter()
        .map(|f| f["name"].as_str().expect("name").to_owned())
        .collect()
}

#[sqlx::test(migrator = "akasha_db::MIGRATOR")]
async fn list_sorts_with_matching_cursors(pool: PgPool) {
    let app = TestApp::new(pool);
    let ada = app.user("ada@example.com").await;
    for (name, body) in [
        ("b.txt", "bb"),
        ("C.txt", "c"),
        ("a.txt", "aaaa"),
        ("d.txt", "ddd"),
    ] {
        let res = app.upload(&ada, name, body.as_bytes()).await;
        assert_eq!(res.status, StatusCode::CREATED);
    }

    for (sort, expected) in [
        ("newest", ["d.txt", "a.txt", "C.txt", "b.txt"]),
        ("oldest", ["b.txt", "C.txt", "a.txt", "d.txt"]),
        ("name", ["a.txt", "b.txt", "C.txt", "d.txt"]),
        ("size", ["a.txt", "d.txt", "b.txt", "C.txt"]),
    ] {
        let mut seen = Vec::new();
        let mut cursor: Option<String> = None;
        loop {
            let mut path = format!("/api/v1/files?limit=3&sort={sort}");
            if let Some(c) = &cursor {
                path.push_str(&format!("&cursor={c}"));
            }
            let res = app.send("GET", &path, &ada, None).await;
            assert_eq!(res.status, StatusCode::OK, "{sort}");
            let page = res.json();
            seen.extend(names(&page));
            match page["next_cursor"].as_str() {
                Some(c) => cursor = Some(c.to_owned()),
                None => break,
            }
        }
        assert_eq!(seen, expected, "{sort}");
    }

    let first = app
        .send("GET", "/api/v1/files?limit=1&sort=name", &ada, None)
        .await
        .json();
    let cursor = first["next_cursor"].as_str().expect("cursor");
    let wrong = app
        .send(
            "GET",
            &format!("/api/v1/files?sort=size&cursor={cursor}"),
            &ada,
            None,
        )
        .await;
    assert_eq!(wrong.status, StatusCode::BAD_REQUEST);
    let bad = app
        .send("GET", "/api/v1/files?sort=random", &ada, None)
        .await;
    assert_eq!(bad.status, StatusCode::BAD_REQUEST);
}

#[sqlx::test(migrator = "akasha_db::MIGRATOR")]
async fn tags_lists_own_and_suggested_tags_per_owner(pool: PgPool) {
    let app = TestApp::new(pool);
    let ada = app.user("ada@example.com").await;
    let bob = app.user("bob@example.com").await;
    let file = app.upload(&ada, "a.txt", b"alpha").await.json();
    app.upload(&bob, "b.txt", b"beta").await;
    let path = format!("/api/v1/files/{}", file["id"].as_str().expect("id"));
    let patch = json!({ "tags": ["work"], "auto_tags": ["tax", "work"] });
    let res = app.send("PATCH", &path, &ada, Some(patch)).await;
    assert_eq!(res.status, StatusCode::OK);

    let tags = app.send("GET", "/api/v1/tags", &ada, None).await;
    assert_eq!(tags.status, StatusCode::OK);
    assert_eq!(
        tags.json()["items"],
        json!([
            { "tag": "tax", "user_files": 0, "auto_files": 1 },
            { "tag": "work", "user_files": 1, "auto_files": 0 },
        ])
    );
    let none = app.send("GET", "/api/v1/tags", &bob, None).await.json();
    assert_eq!(none["items"], json!([]));
    let anon = app.send("GET", "/api/v1/tags", "", None).await;
    assert_eq!(anon.status, StatusCode::UNAUTHORIZED);
}

#[sqlx::test(migrator = "akasha_db::MIGRATOR")]
async fn inline_downloads_only_for_viewable_types(pool: PgPool) {
    let app = TestApp::new(pool);
    let ada = app.user("ada@example.com").await;
    let pdf = app.upload(&ada, "doc.pdf", PDF).await.json();
    let txt = app.upload(&ada, "a.txt", b"plain").await.json();
    let md = app.upload(&ada, "b.md", b"# heading").await.json();
    let url = |file: &Value, query: &str| {
        let id = file["id"].as_str().expect("id");
        format!("/api/v1/files/{id}/download{query}")
    };

    let res = app
        .send("GET", &url(&pdf, "?inline=true"), &ada, None)
        .await;
    assert_eq!(res.status, StatusCode::OK);
    let h = |res: &support::Reply, name| res.headers[name].to_str().expect("h").to_owned();
    assert!(h(&res, header::CONTENT_DISPOSITION).starts_with("inline; filename=\"doc.pdf\""));
    assert_eq!(h(&res, header::CONTENT_TYPE), "application/pdf");
    assert_eq!(h(&res, header::X_CONTENT_TYPE_OPTIONS), "nosniff");
    assert_eq!(h(&res, header::X_FRAME_OPTIONS), "SAMEORIGIN");
    assert_eq!(
        h(&res, header::CONTENT_SECURITY_POLICY),
        "frame-ancestors 'self'"
    );

    let res = app
        .send("GET", &url(&txt, "?inline=true"), &ada, None)
        .await;
    assert!(h(&res, header::CONTENT_DISPOSITION).starts_with("inline;"));
    assert_eq!(
        h(&res, header::CONTENT_SECURITY_POLICY),
        "default-src 'none'; frame-ancestors 'self'; sandbox"
    );

    // Markdown is not on the inline list: still an attachment, never framed.
    let res = app.send("GET", &url(&md, "?inline=true"), &ada, None).await;
    assert!(h(&res, header::CONTENT_DISPOSITION).starts_with("attachment;"));
    assert_eq!(
        h(&res, header::CONTENT_SECURITY_POLICY),
        "default-src 'none'; sandbox"
    );
    assert_eq!(h(&res, header::X_FRAME_OPTIONS), "DENY");

    let res = app
        .send("GET", &url(&pdf, "?inline=false"), &ada, None)
        .await;
    assert!(h(&res, header::CONTENT_DISPOSITION).starts_with("attachment;"));
    let bad = app
        .send("GET", &url(&pdf, "?inline=maybe"), &ada, None)
        .await;
    assert_eq!(bad.status, StatusCode::BAD_REQUEST);
}
