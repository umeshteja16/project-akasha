//! Personal API tokens: management (session only), bearer auth, scopes, revocation.

mod support;

use serde_json::json;
use sqlx::PgPool;
use support::{
    TestApp,
    mcp::{bearer, token},
};

#[sqlx::test(migrator = "akasha_db::MIGRATOR")]
async fn tokens_are_created_once_listed_and_revoked(pool: PgPool) {
    let app = TestApp::new(pool);
    let cookie = app.user("t@example.com").await;

    let created = app
        .send(
            "POST",
            "/api/v1/me/tokens",
            &cookie,
            Some(json!({ "name": " Laptop ", "scopes": ["read", "write"], "expires_in_days": 30 })),
        )
        .await;
    assert_eq!(created.status, 201);
    let body = created.json();
    let secret = body["secret"].as_str().expect("secret");
    assert!(secret.starts_with("akasha_pat_"));
    assert_eq!(body["token"]["name"], "Laptop");
    assert_eq!(body["token"]["scopes"], json!(["read", "write"]));
    assert!(body["token"]["expires_at"].is_string());
    assert!(secret.starts_with(body["token"]["prefix"].as_str().expect("prefix")));

    let list = app
        .send("GET", "/api/v1/me/tokens", &cookie, None)
        .await
        .json();
    assert_eq!(list["items"].as_array().expect("items").len(), 1);
    assert!(list["items"][0].get("secret").is_none());
    assert!(!list.to_string().contains(secret));

    // The token works like a session for the API.
    let me = bearer(&app, "GET", "/api/v1/me", secret, None).await;
    assert_eq!(me.status, 200);
    assert_eq!(me.json()["email"], "t@example.com");
    assert!(
        app.send("GET", "/api/v1/me/tokens", &cookie, None)
            .await
            .json()["items"][0]["last_used_at"]
            .is_string()
    );

    let id = body["token"]["id"].as_str().expect("id");
    let revoked = app
        .send("DELETE", &format!("/api/v1/me/tokens/{id}"), &cookie, None)
        .await;
    assert_eq!(revoked.status, 204);
    let me = bearer(&app, "GET", "/api/v1/me", secret, None).await;
    assert_eq!(me.status, 401);
    let again = app
        .send("DELETE", &format!("/api/v1/me/tokens/{id}"), &cookie, None)
        .await;
    assert_eq!(again.status, 204);
}

#[sqlx::test(migrator = "akasha_db::MIGRATOR")]
async fn bad_requests_and_bad_tokens(pool: PgPool) {
    let app = TestApp::new(pool);
    let cookie = app.user("t@example.com").await;
    for body in [
        json!({ "name": "" }),
        json!({ "name": "x", "scopes": ["write"] }),
        json!({ "name": "x", "scopes": ["admin"] }),
        json!({ "name": "x", "expires_in_days": 0 }),
    ] {
        let r = app
            .send("POST", "/api/v1/me/tokens", &cookie, Some(body.clone()))
            .await;
        assert_eq!(r.status, 400, "{body}");
    }
    // Default scope is read.
    let r = app
        .send(
            "POST",
            "/api/v1/me/tokens",
            &cookie,
            Some(json!({ "name": "x" })),
        )
        .await;
    assert_eq!(r.json()["token"]["scopes"], json!(["read"]));

    for t in ["akasha_pat_nope", "not-a-token", ""] {
        let r = bearer(&app, "GET", "/api/v1/files", t, None).await;
        assert_eq!(r.status, 401, "{t}");
        assert_eq!(r.code(), "unauthorized");
    }
    // Another user's token id cannot be revoked.
    let other = app.user("o@example.com").await;
    let id = r.json()["token"]["id"].as_str().expect("id").to_owned();
    let r = app
        .send("DELETE", &format!("/api/v1/me/tokens/{id}"), &other, None)
        .await;
    assert_eq!(r.status, 404);
}

#[sqlx::test(migrator = "akasha_db::MIGRATOR")]
async fn scopes_limit_what_a_token_can_do(pool: PgPool) {
    let app = TestApp::new(pool);
    let cookie = app.user("t@example.com").await;
    let read = token(&app, &cookie, &["read"]).await;
    let write = token(&app, &cookie, &["read", "write"]).await;
    let uploaded = app.upload(&cookie, "a.txt", b"hello tokens").await;
    let id = uploaded.json()["id"].as_str().expect("id").to_owned();

    let get = bearer(&app, "GET", &format!("/api/v1/files/{id}"), &read, None).await;
    assert_eq!(get.status, 200);
    let patch = json!({ "tags": ["x"] });
    let denied = bearer(
        &app,
        "PATCH",
        &format!("/api/v1/files/{id}"),
        &read,
        Some(patch.clone()),
    )
    .await;
    assert_eq!(denied.status, 403);
    assert_eq!(denied.code(), "forbidden");
    let denied = bearer(&app, "DELETE", &format!("/api/v1/files/{id}"), &read, None).await;
    assert_eq!(denied.status, 403);
    let ok = bearer(
        &app,
        "PATCH",
        &format!("/api/v1/files/{id}"),
        &write,
        Some(patch),
    )
    .await;
    assert_eq!(ok.status, 200);

    // No token can manage tokens or the account, whatever its scopes.
    for (method, path) in [
        ("GET", "/api/v1/me/tokens"),
        ("POST", "/api/v1/me/tokens"),
        ("PATCH", "/api/v1/me"),
        ("POST", "/api/v1/auth/logout"),
    ] {
        let r = bearer(&app, method, path, &write, Some(json!({ "name": "x" }))).await;
        assert_eq!(r.status, 403, "{method} {path}");
    }
}
