//! Listing and revoking your sign-in sessions (Settings → Security).

mod support;

use axum::{
    body::Body,
    http::{Request, StatusCode, header},
};
use serde_json::json;
use sqlx::PgPool;
use support::{PW, TestApp};

async fn login(app: &TestApp, email: &str, agent: &str) -> String {
    let req = Request::post("/api/v1/auth/login")
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::USER_AGENT, agent)
        .body(Body::from(
            json!({ "email": email, "password": PW }).to_string(),
        ))
        .expect("request");
    let res = app.request(req).await;
    assert_eq!(res.status, StatusCode::OK);
    res.headers[header::SET_COOKIE]
        .to_str()
        .expect("cookie")
        .split(';')
        .next()
        .expect("pair")
        .to_owned()
}

#[sqlx::test(migrator = "akasha_db::MIGRATOR")]
async fn sessions_are_listed_and_revoked_individually(pool: PgPool) {
    let app = TestApp::new(pool);
    let first = app.user("ada@example.com").await;
    let phone = login(&app, "ada@example.com", "Phone/1.0").await;
    let laptop = login(&app, "ada@example.com", "Laptop/2.0").await;
    let bob = app.user("bob@example.com").await;

    let list = app
        .send("GET", "/api/v1/me/sessions", &laptop, None)
        .await
        .json();
    let items = list["items"].as_array().expect("items");
    assert_eq!(items.len(), 3);
    let current: Vec<_> = items.iter().filter(|s| s["current"] == true).collect();
    assert_eq!(current.len(), 1);
    assert_eq!(current[0]["user_agent"], "Laptop/2.0");
    assert_eq!(current[0]["ip"], "127.0.0.1");
    let phone_id = items
        .iter()
        .find(|s| s["user_agent"] == "Phone/1.0")
        .expect("phone")["id"]
        .as_str()
        .expect("id")
        .to_owned();
    let current_id = current[0]["id"].as_str().expect("id").to_owned();

    // Not Bob's to revoke, and not the current one (that is sign out).
    let path = format!("/api/v1/me/sessions/{phone_id}");
    assert_eq!(
        app.send("DELETE", &path, &bob, None).await.status,
        StatusCode::NOT_FOUND
    );
    let own = format!("/api/v1/me/sessions/{current_id}");
    assert_eq!(
        app.send("DELETE", &own, &laptop, None).await.status,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        app.send("GET", "/api/v1/me", &phone, None).await.status,
        StatusCode::OK
    );

    assert_eq!(
        app.send("DELETE", &path, &laptop, None).await.status,
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        app.send("GET", "/api/v1/me", &phone, None).await.status,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        app.send("DELETE", &path, &laptop, None).await.status,
        StatusCode::NOT_FOUND
    );

    let res = app
        .send("POST", "/api/v1/me/sessions/revoke-others", &laptop, None)
        .await;
    assert_eq!(res.json()["revoked"], 1);
    assert_eq!(
        app.send("GET", "/api/v1/me", &first, None).await.status,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        app.send("GET", "/api/v1/me", &laptop, None).await.status,
        StatusCode::OK
    );
    // Bob was never affected.
    assert_eq!(
        app.send("GET", "/api/v1/me", &bob, None).await.status,
        StatusCode::OK
    );

    let log = app
        .send(
            "GET",
            "/api/v1/activity?kind=session.revoked",
            &laptop,
            None,
        )
        .await
        .json();
    assert_eq!(log["items"].as_array().expect("items").len(), 2);
}
