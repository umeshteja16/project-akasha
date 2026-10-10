//! The activity timeline and security log: written on each action, listed to
//! the owner only (cursor pagination, filters), search-history opt-out,
//! clearing, retention, and session-only access.

mod support;

use std::time::Duration;

use akasha_jobs::Job;
use axum::{
    body::Body,
    http::{Request, StatusCode, header},
};
use serde_json::{Value, json};
use sqlx::PgPool;
use support::{
    PW, TestApp,
    chat::{ask, conversation},
    mcp::{bearer, token},
    test_config,
};

async fn upload(app: &TestApp, cookie: &str, name: &str, bytes: &[u8]) -> String {
    app.upload(cookie, name, bytes).await.json()["id"]
        .as_str()
        .expect("id")
        .to_owned()
}

async fn activity(app: &TestApp, cookie: &str, query: &str) -> Value {
    let res = app
        .send("GET", &format!("/api/v1/activity{query}"), cookie, None)
        .await;
    assert_eq!(res.status, StatusCode::OK, "{:?}", res.json());
    res.json()
}

fn kinds(page: &Value) -> Vec<String> {
    page["items"]
        .as_array()
        .expect("items")
        .iter()
        .map(|e| e["kind"].as_str().expect("kind").to_owned())
        .collect()
}

async fn login(app: &TestApp, email: &str, password: &str, agent: &str) -> (StatusCode, String) {
    let req = Request::post("/api/v1/auth/login")
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::USER_AGENT, agent)
        .body(Body::from(
            json!({ "email": email, "password": password }).to_string(),
        ))
        .expect("request");
    let res = app.request(req).await;
    let cookie = res
        .headers
        .get(header::SET_COOKIE)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.split(';').next())
        .unwrap_or_default()
        .to_owned();
    (res.status, cookie)
}

#[sqlx::test(migrator = "akasha_db::MIGRATOR")]
async fn file_search_chat_and_collection_actions_are_recorded(pool: PgPool) {
    let app = TestApp::new(pool);
    let ada = app.user("ada@example.com").await;
    let bob = app.user("bob@example.com").await;
    let a = upload(
        &app,
        &ada,
        "burrows.txt",
        b"The aardvark digs a burrow at night.",
    )
    .await;
    let b = upload(&app, &ada, "diet.txt", b"Aardvark diet: termites and ants.").await;
    app.run_jobs().await;

    let file = format!("/api/v1/files/{a}");
    app.send("PATCH", &file, &ada, Some(json!({ "name": "nights.txt" })))
        .await;
    app.send("PATCH", &file, &ada, Some(json!({ "tags": ["Zoo"] })))
        .await;
    // Pinning is not worth a timeline entry.
    app.send("PATCH", &file, &ada, Some(json!({ "is_pinned": true })))
        .await;
    for _ in 0..3 {
        let opened = app.send("POST", &format!("{file}/open"), &ada, None).await;
        assert_eq!(opened.status, StatusCode::OK);
    }
    let detail = app.send("GET", &file, &ada, None).await.json();
    assert_eq!(detail["open_count"], 3);
    assert!(detail["last_opened_at"].is_string());
    // Bob cannot open (or see) Ada's file.
    let res = app.send("POST", &format!("{file}/open"), &bob, None).await;
    assert_eq!(res.status, StatusCode::NOT_FOUND);

    // Typing "aard" then "aardvark" is one search entry.
    for q in ["aard", "aardvark"] {
        app.send("GET", &format!("/api/v1/search?q={q}"), &ada, None)
            .await;
    }
    let conv = conversation(&app, &ada).await;
    ask(
        &app,
        &ada,
        &conv,
        json!({ "content": "What does the aardvark eat?" }),
    )
    .await;

    let c = app
        .send(
            "POST",
            "/api/v1/collections",
            &ada,
            Some(json!({ "name": "Zoo", "file_ids": [a] })),
        )
        .await
        .json();
    let cpath = format!("/api/v1/collections/{}", c["id"].as_str().expect("id"));
    app.send(
        "POST",
        &format!("{cpath}/files"),
        &ada,
        Some(json!({ "file_ids": [b] })),
    )
    .await;
    app.send(
        "POST",
        &format!("{cpath}/files/remove"),
        &ada,
        Some(json!({ "file_ids": [b] })),
    )
    .await;
    app.send(
        "PATCH",
        &cpath,
        &ada,
        Some(json!({ "name": "Zoo animals" })),
    )
    .await;
    app.send("DELETE", &cpath, &ada, None).await;
    app.send(
        "POST",
        "/api/v1/files/bulk-delete",
        &ada,
        Some(json!({ "ids": [a, b] })),
    )
    .await;

    let page = activity(&app, &ada, "?category=files,search,chat,collections").await;
    assert_eq!(
        kinds(&page),
        [
            "file.deleted",
            "collection.deleted",
            "collection.updated",
            "collection.files_removed",
            "collection.files_added",
            "collection.files_added",
            "collection.created",
            "chat.asked",
            "search.performed",
            "file.opened",
            "file.tagged",
            "file.renamed",
            "file.uploaded",
            "file.uploaded",
        ]
    );
    let items = page["items"].as_array().expect("items");
    let get = |kind: &str| {
        items
            .iter()
            .find(|e| e["kind"] == kind)
            .unwrap_or_else(|| panic!("{kind}"))
    };
    assert_eq!(get("file.deleted")["details"]["count"], 2);
    assert_eq!(
        get("file.renamed")["details"],
        json!({ "from": "burrows.txt", "to": "nights.txt" })
    );
    assert_eq!(get("file.tagged")["details"]["added"], json!(["zoo"]));
    assert_eq!(get("search.performed")["subject"], "aardvark");
    assert_eq!(get("chat.asked")["conversation_id"], json!(conv));
    assert_eq!(get("collection.updated")["details"]["to"], "Zoo animals");
    // Deleted files and collections are no longer linked; the names stay.
    assert_eq!(get("file.renamed")["file_id"], Value::Null);
    assert_eq!(get("file.renamed")["subject"], "nights.txt");
    assert_eq!(get("collection.created")["collection_id"], Value::Null);
    assert_eq!(get("file.uploaded")["via"], "session");

    // Bob sees none of it (only his own sign-up).
    let bobs = activity(&app, &bob, "").await;
    assert_eq!(kinds(&bobs), ["account.created"]);

    // Cursor pagination walks every event once.
    let mut seen = Vec::new();
    let mut cursor: Option<String> = None;
    loop {
        let q = match &cursor {
            Some(c) => format!("?limit=4&cursor={c}"),
            None => "?limit=4".to_owned(),
        };
        let page = activity(&app, &ada, &q).await;
        seen.extend(kinds(&page));
        match page["next_cursor"].as_str() {
            Some(c) => cursor = Some(c.to_owned()),
            None => break,
        }
    }
    // The 14 above plus the sign-up.
    assert_eq!(seen.len(), 15, "{seen:?}");
    let bad = app
        .send("GET", "/api/v1/activity?category=nope", &ada, None)
        .await;
    assert_eq!(bad.status, StatusCode::BAD_REQUEST);
}

#[sqlx::test(migrator = "akasha_db::MIGRATOR")]
async fn search_history_can_be_turned_off_and_cleared(pool: PgPool) {
    let app = TestApp::new(pool);
    let ada = app.user("ada@example.com").await;
    let me = app
        .send(
            "PATCH",
            "/api/v1/me",
            &ada,
            Some(json!({ "record_search_history": false })),
        )
        .await;
    assert_eq!(me.status, StatusCode::OK);
    assert_eq!(me.json()["record_search_history"], false);
    app.send("GET", "/api/v1/search?q=secret", &ada, None).await;
    assert!(kinds(&activity(&app, &ada, "?category=search").await).is_empty());

    app.send(
        "PATCH",
        "/api/v1/me",
        &ada,
        Some(json!({ "record_search_history": true })),
    )
    .await;
    app.send("GET", "/api/v1/search?q=budget", &ada, None).await;
    // A later page of the same search is not another entry.
    app.send("GET", "/api/v1/search?q=budget&page=2", &ada, None)
        .await;
    assert_eq!(
        kinds(&activity(&app, &ada, "?category=search").await),
        ["search.performed"]
    );

    // The display name survives a preference-only update.
    app.send(
        "PATCH",
        "/api/v1/me",
        &ada,
        Some(json!({ "display_name": "Ada" })),
    )
    .await;
    let me = app
        .send(
            "PATCH",
            "/api/v1/me",
            &ada,
            Some(json!({ "record_search_history": true })),
        )
        .await
        .json();
    assert_eq!(me["display_name"], "Ada");

    // Clearing removes everything but the security log.
    let bad = app
        .send("DELETE", "/api/v1/activity?category=security", &ada, None)
        .await;
    assert_eq!(bad.status, StatusCode::BAD_REQUEST);
    let cleared = app
        .send("DELETE", "/api/v1/activity", &ada, None)
        .await
        .json();
    assert_eq!(cleared["deleted"], 1);
    assert_eq!(kinds(&activity(&app, &ada, "").await), ["account.created"]);
}

#[sqlx::test(migrator = "akasha_db::MIGRATOR")]
async fn security_events_carry_address_and_browser(pool: PgPool) {
    let app = TestApp::new(pool);
    let ada = app.user("ada@example.com").await;
    let (status, _) = login(&app, "ada@example.com", "wrong password!", "Evil/1.0").await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let (status, laptop) = login(&app, "ada@example.com", PW, "Firefox/140.0").await;
    assert_eq!(status, StatusCode::OK);
    let res = app
        .send(
            "POST",
            "/api/v1/me/password",
            &laptop,
            Some(json!({ "current_password": "nope nope nope", "new_password": "x".repeat(12) })),
        )
        .await;
    assert_eq!(res.status, StatusCode::UNAUTHORIZED);
    let t = app
        .send(
            "POST",
            "/api/v1/me/tokens",
            &laptop,
            Some(json!({ "name": "CLI" })),
        )
        .await
        .json();
    let tid = t["token"]["id"].as_str().expect("id");
    app.send("DELETE", &format!("/api/v1/me/tokens/{tid}"), &laptop, None)
        .await;
    app.send("DELETE", &format!("/api/v1/me/tokens/{tid}"), &laptop, None)
        .await;

    // The failed sign-in is recorded in the background.
    let mut page = Value::Null;
    for _ in 0..50 {
        page = activity(&app, &ada, "?category=security").await;
        if kinds(&page).contains(&"auth.sign_in_failed".to_owned()) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let got = kinds(&page);
    assert_eq!(
        got.iter()
            .filter(|k| k.as_str() != "auth.sign_in_failed")
            .collect::<Vec<_>>(),
        [
            "token.revoked",
            "token.created",
            "auth.password_change_failed",
            "auth.signed_in",
            "account.created",
        ]
    );
    let items = page["items"].as_array().expect("items");
    let failed = items
        .iter()
        .find(|e| e["kind"] == "auth.sign_in_failed")
        .expect("failed");
    assert_eq!(failed["ip"], "127.0.0.1");
    assert_eq!(failed["user_agent"], "Evil/1.0");
    let signed_in = items
        .iter()
        .find(|e| e["kind"] == "auth.signed_in")
        .expect("signed in");
    assert_eq!(signed_in["user_agent"], "Firefox/140.0");
    let created = items
        .iter()
        .find(|e| e["kind"] == "token.created")
        .expect("token");
    assert_eq!(created["subject"], "CLI");

    // The timeline and the session list are account data: no API tokens.
    let t = token(&app, &ada, &["read", "write"]).await;
    for path in ["/api/v1/activity", "/api/v1/me/sessions"] {
        let res = bearer(&app, "GET", path, &t, None).await;
        assert_eq!(res.status, StatusCode::FORBIDDEN, "{path}");
    }
}

#[sqlx::test(migrator = "akasha_db::MIGRATOR")]
async fn rate_limit_hits_are_logged_once(pool: PgPool) {
    let config = akasha_core::Config {
        search_rate_per_minute: 1,
        ..test_config()
    };
    let app = TestApp::with_config(pool, config);
    let ada = app.user("ada@example.com").await;
    assert_eq!(
        app.send("GET", "/api/v1/search?q=a", &ada, None)
            .await
            .status,
        StatusCode::OK
    );
    for _ in 0..3 {
        let res = app.send("GET", "/api/v1/search?q=b", &ada, None).await;
        assert_eq!(res.status, StatusCode::TOO_MANY_REQUESTS);
    }
    for _ in 0..50 {
        if !kinds(&activity(&app, &ada, "?kind=rate.limited").await).is_empty() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    // The other two hits within the window add nothing.
    tokio::time::sleep(Duration::from_millis(100)).await;
    let got = kinds(&activity(&app, &ada, "?kind=rate.limited").await);
    assert_eq!(got, ["rate.limited"]);
}

#[sqlx::test(migrator = "akasha_db::MIGRATOR")]
async fn retention_job_prunes_old_events(pool: PgPool) {
    let app = TestApp::new(pool.clone());
    let ada = app.user("ada@example.com").await;
    upload(&app, &ada, "a.txt", b"hello there").await;
    sqlx::query(
        "UPDATE activity_events SET created_at = now() - interval '400 days'
         WHERE kind = 'file.uploaded'",
    )
    .execute(&pool)
    .await
    .expect("age");
    let prune = akasha::jobs::kinds::PruneActivity {};

    // Retention 0 keeps everything.
    let keep = akasha_core::Config {
        activity_retention_days: 0,
        ..test_config()
    };
    akasha_jobs::enqueue(&mut pool.acquire().await.expect("conn"), &prune)
        .await
        .expect("enqueue");
    app.run_jobs_with(&keep).await;
    assert_eq!(
        activity(&app, &ada, "").await["items"]
            .as_array()
            .expect("items")
            .len(),
        2
    );

    akasha_jobs::enqueue(&mut pool.acquire().await.expect("conn"), &prune)
        .await
        .expect("enqueue");
    app.run_jobs().await;
    assert_eq!(kinds(&activity(&app, &ada, "").await), ["account.created"]);
    let statuses: Vec<String> = sqlx::query_scalar("SELECT status FROM jobs WHERE kind = $1")
        .bind(akasha::jobs::kinds::PruneActivity::KIND)
        .fetch_all(&pool)
        .await
        .expect("jobs");
    assert_eq!(statuses, ["succeeded", "succeeded"]);
}
