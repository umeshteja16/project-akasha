//! Collections: CRUD, files in and out, owner isolation, token scopes, and
//! collection-scoped listing, search, chat and MCP.

mod support;

use axum::http::StatusCode;
use serde_json::{Value, json};
use sqlx::PgPool;
use support::{
    TestApp,
    chat::{ask, find},
    mcp::{bearer, token, tool},
};

const BURROWS: &[u8] = b"The aardvark digs a burrow at night and eats termites.";
const DIET: &[u8] = b"Aardvark diet notes: termites, ants and the odd cucumber.";

async fn upload(app: &TestApp, cookie: &str, name: &str, bytes: &[u8]) -> String {
    app.upload(cookie, name, bytes).await.json()["id"]
        .as_str()
        .expect("id")
        .to_owned()
}

async fn collection(app: &TestApp, cookie: &str, body: Value) -> Value {
    let res = app
        .send("POST", "/api/v1/collections", cookie, Some(body))
        .await;
    assert_eq!(res.status, StatusCode::CREATED, "{:?}", res.json());
    res.json()
}

fn ids(list: &Value) -> Vec<String> {
    list["items"]
        .as_array()
        .expect("items")
        .iter()
        .map(|f| f["id"].as_str().expect("id").to_owned())
        .collect()
}

#[sqlx::test(migrator = "akasha_db::MIGRATOR")]
async fn crud_and_files(pool: PgPool) {
    let app = TestApp::new(pool);
    let ada = app.user("ada@example.com").await;
    let a = upload(&app, &ada, "burrows.txt", BURROWS).await;
    let b = upload(&app, &ada, "diet.txt", DIET).await;

    let c = collection(
        &app,
        &ada,
        json!({ "name": "  Field   notes ", "description": "Aardvarks", "color": "ochre",
                "icon": "book", "file_ids": [a] }),
    )
    .await;
    assert_eq!(c["name"], "Field notes");
    assert_eq!(c["color"], "ochre");
    assert_eq!(c["icon"], "book");
    assert_eq!(c["file_count"], 1);
    let id = c["id"].as_str().expect("id").to_owned();
    let path = format!("/api/v1/collections/{id}");

    // Names are unique per owner, ignoring case; bad input is a 400.
    let dup = app
        .send(
            "POST",
            "/api/v1/collections",
            &ada,
            Some(json!({ "name": "FIELD NOTES" })),
        )
        .await;
    assert_eq!(dup.status, StatusCode::CONFLICT);
    for bad in [
        json!({ "name": " " }),
        json!({ "name": "x", "color": "#ff0000" }),
    ] {
        let res = app
            .send("POST", "/api/v1/collections", &ada, Some(bad))
            .await;
        assert_eq!(res.status, StatusCode::BAD_REQUEST);
    }

    let added = app
        .send(
            "POST",
            &format!("{path}/files"),
            &ada,
            Some(json!({ "file_ids": [a, b] })),
        )
        .await
        .json();
    assert_eq!(added["file_ids"], json!([b]));
    assert_eq!(added["collection"]["file_count"], 2);

    let listed = app
        .send(
            "GET",
            &format!("/api/v1/files?collection_id={id}"),
            &ada,
            None,
        )
        .await;
    assert_eq!(listed.status, StatusCode::OK);
    assert_eq!(ids(&listed.json()).len(), 2);
    let detail = app
        .send("GET", &format!("/api/v1/files/{a}"), &ada, None)
        .await
        .json();
    assert_eq!(detail["collections"][0]["name"], "Field notes");

    let renamed = app
        .send(
            "PATCH",
            &path,
            &ada,
            Some(json!({ "name": "Aardvarks", "color": "plum" })),
        )
        .await
        .json();
    assert_eq!(
        (renamed["name"].as_str(), renamed["color"].as_str()),
        (Some("Aardvarks"), Some("plum"))
    );

    let removed = app
        .send(
            "POST",
            &format!("{path}/files/remove"),
            &ada,
            Some(json!({ "file_ids": [a] })),
        )
        .await
        .json();
    assert_eq!(removed["file_ids"], json!([a]));
    assert_eq!(removed["collection"]["file_count"], 1);

    let all = app
        .send("GET", "/api/v1/collections", &ada, None)
        .await
        .json();
    assert_eq!(all["items"].as_array().expect("items").len(), 1);

    assert_eq!(
        app.send("DELETE", &path, &ada, None).await.status,
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        app.send("GET", &path, &ada, None).await.status,
        StatusCode::NOT_FOUND
    );
    // The files stay.
    let files = app.send("GET", "/api/v1/files", &ada, None).await.json();
    assert_eq!(ids(&files).len(), 2);
}

#[sqlx::test(migrator = "akasha_db::MIGRATOR")]
async fn other_users_cannot_see_or_touch_a_collection(pool: PgPool) {
    let app = TestApp::new(pool);
    let ada = app.user("ada@example.com").await;
    let bob = app.user("bob@example.com").await;
    let a = upload(&app, &ada, "burrows.txt", BURROWS).await;
    let bobs = upload(&app, &bob, "bob.txt", b"Bob's aardvark notes.").await;
    app.run_jobs().await;
    let c = collection(&app, &ada, json!({ "name": "Mine", "file_ids": [a] })).await;
    let id = c["id"].as_str().expect("id");
    let path = format!("/api/v1/collections/{id}");

    // Ada cannot put Bob's file into her collection.
    let res = app
        .send(
            "POST",
            &format!("{path}/files"),
            &ada,
            Some(json!({ "file_ids": [bobs] })),
        )
        .await
        .json();
    assert_eq!(res["file_ids"], json!([]));

    let body = Some(json!({ "file_ids": [bobs] }));
    for (method, url, body) in [
        ("GET", path.clone(), None),
        ("PATCH", path.clone(), Some(json!({ "name": "Stolen" }))),
        ("POST", format!("{path}/files"), body.clone()),
        (
            "POST",
            format!("{path}/files/remove"),
            Some(json!({ "file_ids": [a] })),
        ),
        ("GET", format!("/api/v1/files?collection_id={id}"), None),
        (
            "GET",
            format!("/api/v1/search?q=aardvark&collection_id={id}"),
            None,
        ),
        (
            "GET",
            format!("/api/v1/search/chunks?q=aardvark&collection_id={id}"),
            None,
        ),
        (
            "POST",
            "/api/v1/conversations".to_owned(),
            Some(json!({ "collection_id": id })),
        ),
        ("DELETE", path.clone(), None),
    ] {
        let res = app.send(method, &url, &bob, body).await;
        assert_eq!(res.status, StatusCode::NOT_FOUND, "{method} {url}");
    }
    let listed = app
        .send("GET", "/api/v1/collections", &bob, None)
        .await
        .json();
    assert_eq!(listed["items"], json!([]));
    // Still intact for Ada.
    let mine = app.send("GET", &path, &ada, None).await.json();
    assert_eq!(
        (mine["name"].as_str(), mine["file_count"].as_i64()),
        (Some("Mine"), Some(1))
    );
}

#[sqlx::test(migrator = "akasha_db::MIGRATOR")]
async fn search_and_chat_stay_within_a_collection(pool: PgPool) {
    let app = TestApp::new(pool);
    let ada = app.user("ada@example.com").await;
    let burrows = upload(&app, &ada, "burrows.txt", BURROWS).await;
    let diet = upload(&app, &ada, "diet.txt", DIET).await;
    app.run_jobs().await;
    let c = collection(&app, &ada, json!({ "name": "Diet", "file_ids": [diet] })).await;
    let id = c["id"].as_str().expect("id").to_owned();

    let everywhere = app
        .send("GET", "/api/v1/search?q=aardvark&mode=keyword", &ada, None)
        .await
        .json();
    assert_eq!(everywhere["results"].as_array().expect("results").len(), 2);
    for mode in ["keyword", "semantic", "hybrid"] {
        let url =
            format!("/api/v1/search?q=aardvark&mode={mode}&include_weak=true&collection_id={id}");
        let res = app.send("GET", &url, &ada, None).await.json();
        let found: Vec<_> = res["results"]
            .as_array()
            .expect("results")
            .iter()
            .map(|r| r["file"]["id"].as_str().expect("id").to_owned())
            .collect();
        assert_eq!(found, std::slice::from_ref(&diet), "{mode}");
    }

    // A conversation scoped to the collection only cites its files.
    let conv = app
        .send(
            "POST",
            "/api/v1/conversations",
            &ada,
            Some(json!({ "collection_id": id })),
        )
        .await;
    assert_eq!(conv.status, StatusCode::CREATED);
    assert_eq!(conv.json()["collection_id"], json!(id));
    let conv = conv.json()["id"].as_str().expect("id").to_owned();
    let (_, events) = ask(
        &app,
        &ada,
        &conv,
        json!({ "content": "aardvark burrow termites" }),
    )
    .await;
    let sources: Vec<_> = find(&events, "sources")["sources"]
        .as_array()
        .expect("sources")
        .iter()
        .map(|s| s["file_id"].as_str().expect("id").to_owned())
        .collect();
    assert!(!sources.is_empty());
    assert!(sources.iter().all(|f| *f == diet), "{sources:?}");

    // Clearing the scope answers from everything again.
    let cleared = app
        .send(
            "PATCH",
            &format!("/api/v1/conversations/{conv}"),
            &ada,
            Some(json!({ "collection_id": null })),
        )
        .await
        .json();
    assert_eq!(cleared["collection_id"], Value::Null);
    let (_, events) = ask(
        &app,
        &ada,
        &conv,
        json!({ "content": "aardvark burrow termites" }),
    )
    .await;
    let sources = find(&events, "sources")["sources"].to_string();
    assert!(sources.contains(&burrows));
}

#[sqlx::test(migrator = "akasha_db::MIGRATOR")]
async fn tokens_need_write_scope_to_change_collections(pool: PgPool) {
    let app = TestApp::new(pool);
    let ada = app.user("ada@example.com").await;
    let read = token(&app, &ada, &["read"]).await;
    let write = token(&app, &ada, &["read", "write"]).await;

    let denied = bearer(
        &app,
        "POST",
        "/api/v1/collections",
        &read,
        Some(json!({ "name": "X" })),
    )
    .await;
    assert_eq!(denied.status, StatusCode::FORBIDDEN);
    let made = bearer(
        &app,
        "POST",
        "/api/v1/collections",
        &write,
        Some(json!({ "name": "X" })),
    )
    .await;
    assert_eq!(made.status, StatusCode::CREATED);
    let listed = bearer(&app, "GET", "/api/v1/collections", &read, None).await;
    assert_eq!(listed.json()["items"][0]["name"], "X");
    let id = made.json()["id"].as_str().expect("id").to_owned();
    let gone = bearer(
        &app,
        "DELETE",
        &format!("/api/v1/collections/{id}"),
        &read,
        None,
    )
    .await;
    assert_eq!(gone.status, StatusCode::FORBIDDEN);
}

#[sqlx::test(migrator = "akasha_db::MIGRATOR")]
async fn mcp_tools_filter_by_collection(pool: PgPool) {
    let app = TestApp::new(pool);
    let ada = app.user("ada@example.com").await;
    let bob = app.user("bob@example.com").await;
    upload(&app, &ada, "burrows.txt", BURROWS).await;
    let diet = upload(&app, &ada, "diet.txt", DIET).await;
    app.run_jobs().await;
    collection(&app, &ada, json!({ "name": "Diet", "file_ids": [diet] })).await;
    collection(&app, &bob, json!({ "name": "Bob's" })).await;
    let t = token(&app, &ada, &["read"]).await;

    let (err, listed) = tool(&app, &t, "list_collections", json!({})).await;
    assert!(!err);
    assert_eq!(listed["collections"].as_array().expect("list").len(), 1);
    assert_eq!(listed["collections"][0]["name"], "Diet");

    let (err, found) = tool(
        &app,
        &t,
        "search",
        json!({ "query": "aardvark", "collection": "diet" }),
    )
    .await;
    assert!(!err, "{found}");
    let files: Vec<_> = found["results"]
        .as_array()
        .expect("results")
        .iter()
        .map(|r| r["file_id"].as_str().expect("id").to_owned())
        .collect();
    assert!(
        !files.is_empty() && files.iter().all(|f| *f == diet),
        "{files:?}"
    );

    let (err, page) = tool(&app, &t, "list_files", json!({ "collection": "Diet" })).await;
    assert!(!err);
    assert_eq!(page["files"].as_array().expect("files").len(), 1);

    // Another user's collection does not resolve.
    let (err, _) = tool(&app, &t, "list_files", json!({ "collection": "Bob's" })).await;
    assert!(err);
}
