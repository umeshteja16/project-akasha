//! End-to-end auth flows over HTTP against a real (per-test) database.

use std::net::SocketAddr;

use akasha::{AppState, app};
use akasha_core::Config;
use akasha_storage::Storage;
use axum::{
    Extension, Router,
    body::Body,
    extract::ConnectInfo,
    http::{Request, StatusCode, header},
};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use sqlx::PgPool;
use tower::ServiceExt;

struct TestApp {
    router: Router,
}

struct Reply {
    status: StatusCode,
    body: Value,
    set_cookie: Option<String>,
}

impl Reply {
    /// The session token from `Set-Cookie`, as a ready-to-send `Cookie` header value.
    fn session(&self) -> String {
        let raw = self.set_cookie.as_deref().expect("set-cookie header");
        raw.split(';').next().expect("cookie pair").to_owned()
    }
}

impl TestApp {
    fn new(pool: PgPool) -> Self {
        Self::with_config(pool, Config::default())
    }

    fn with_config(pool: PgPool, config: Config) -> Self {
        // What `into_make_service_with_connect_info` provides in production.
        let addr = SocketAddr::from(([127, 0, 0, 1], 40000));
        let router = app(AppState::new(pool, config, Storage::in_memory()))
            .layer(Extension(ConnectInfo(addr)));
        Self { router }
    }

    async fn send(
        &self,
        method: &str,
        path: &str,
        cookie: Option<&str>,
        body: Option<Value>,
    ) -> Reply {
        let mut req = Request::builder().method(method).uri(path);
        if let Some(cookie) = cookie {
            req = req.header(header::COOKIE, cookie);
        }
        let req = match body {
            Some(json) => req
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(json.to_string())),
            None => req.body(Body::empty()),
        }
        .expect("request");

        let res = self.router.clone().oneshot(req).await.expect("response");
        let status = res.status();
        let set_cookie = res
            .headers()
            .get(header::SET_COOKIE)
            .and_then(|v| v.to_str().ok())
            .map(str::to_owned);
        let bytes = res.into_body().collect().await.expect("body").to_bytes();
        let body = if bytes.is_empty() {
            Value::Null
        } else {
            serde_json::from_slice(&bytes).expect("json body")
        };
        Reply {
            status,
            body,
            set_cookie,
        }
    }

    async fn register(&self, email: &str, password: &str) -> Reply {
        let body = json!({ "email": email, "password": password });
        self.send("POST", "/api/v1/auth/register", None, Some(body))
            .await
    }

    async fn me_status(&self, cookie: &str) -> StatusCode {
        self.send("GET", "/api/v1/me", Some(cookie), None)
            .await
            .status
    }

    async fn login(&self, email: &str, password: &str) -> Reply {
        let body = json!({ "email": email, "password": password });
        self.send("POST", "/api/v1/auth/login", None, Some(body))
            .await
    }
}

const PW: &str = "correct horse battery";

#[sqlx::test(migrator = "akasha_db::MIGRATOR")]
async fn register_me_logout(pool: PgPool) {
    let app = TestApp::new(pool);

    let reg = app.register("ada@example.com", PW).await;
    assert_eq!(reg.status, StatusCode::CREATED);
    assert_eq!(reg.body["email"], "ada@example.com");
    assert!(
        reg.body.get("password_hash").is_none(),
        "never leak the hash"
    );
    let raw_cookie = reg.set_cookie.clone().expect("cookie");
    assert!(raw_cookie.contains("HttpOnly") && raw_cookie.contains("SameSite=Lax"));
    let cookie = reg.session();

    let me = app.send("GET", "/api/v1/me", Some(&cookie), None).await;
    assert_eq!(me.status, StatusCode::OK);
    assert_eq!(me.body["id"], reg.body["id"]);

    let out = app
        .send("POST", "/api/v1/auth/logout", Some(&cookie), None)
        .await;
    assert_eq!(out.status, StatusCode::NO_CONTENT);

    let after = app.send("GET", "/api/v1/me", Some(&cookie), None).await;
    assert_eq!(after.status, StatusCode::UNAUTHORIZED);
    assert_eq!(after.body["error"]["code"], "unauthorized");
}

#[sqlx::test(migrator = "akasha_db::MIGRATOR")]
async fn me_requires_a_session(pool: PgPool) {
    let app = TestApp::new(pool);
    let anon = app.send("GET", "/api/v1/me", None, None).await;
    assert_eq!(anon.status, StatusCode::UNAUTHORIZED);
    let forged = app
        .send("GET", "/api/v1/me", Some("akasha_session=forged"), None)
        .await;
    assert_eq!(forged.status, StatusCode::UNAUTHORIZED);
}

#[sqlx::test(migrator = "akasha_db::MIGRATOR")]
async fn duplicate_email_conflicts_case_insensitively(pool: PgPool) {
    let app = TestApp::new(pool);
    assert_eq!(
        app.register("ada@example.com", PW).await.status,
        StatusCode::CREATED
    );
    let dup = app.register("ADA@example.com", PW).await;
    assert_eq!(dup.status, StatusCode::CONFLICT);
    assert_eq!(dup.body["error"]["code"], "conflict");
}

#[sqlx::test(migrator = "akasha_db::MIGRATOR")]
async fn login_checks_password_without_revealing_accounts(pool: PgPool) {
    let app = TestApp::new(pool);
    app.register("ada@example.com", PW).await;

    let wrong = app.login("ada@example.com", "not the password").await;
    let unknown = app.login("nobody@example.com", PW).await;
    assert_eq!(wrong.status, StatusCode::UNAUTHORIZED);
    assert_eq!(unknown.status, StatusCode::UNAUTHORIZED);
    assert_eq!(wrong.body, unknown.body, "same response for both failures");

    let ok = app.login("Ada@Example.com", PW).await;
    assert_eq!(ok.status, StatusCode::OK);
    let me = app
        .send("GET", "/api/v1/me", Some(&ok.session()), None)
        .await;
    assert_eq!(me.status, StatusCode::OK);
}

#[sqlx::test(migrator = "akasha_db::MIGRATOR")]
async fn input_validation_uses_error_shape(pool: PgPool) {
    let app = TestApp::new(pool);
    for (email, password) in [("ada@example.com", "short"), ("not-an-email", PW)] {
        let res = app.register(email, password).await;
        assert_eq!(res.status, StatusCode::BAD_REQUEST, "{email} / {password}");
        assert_eq!(res.body["error"]["code"], "bad_request");
    }
    let malformed = app
        .send(
            "POST",
            "/api/v1/auth/login",
            None,
            Some(json!({ "email": 1 })),
        )
        .await;
    assert_eq!(malformed.status, StatusCode::BAD_REQUEST);
    assert_eq!(malformed.body["error"]["code"], "bad_request");
}

#[sqlx::test(migrator = "akasha_db::MIGRATOR")]
async fn registration_can_be_disabled(pool: PgPool) {
    let config = Config {
        allow_registration: false,
        ..Config::default()
    };
    let app = TestApp::with_config(pool, config);
    let res = app.register("ada@example.com", PW).await;
    assert_eq!(res.status, StatusCode::FORBIDDEN);
}

#[sqlx::test(migrator = "akasha_db::MIGRATOR")]
async fn update_display_name(pool: PgPool) {
    let app = TestApp::new(pool);
    let cookie = app.register("ada@example.com", PW).await.session();

    let set = app
        .send(
            "PATCH",
            "/api/v1/me",
            Some(&cookie),
            Some(json!({ "display_name": "  Ada  " })),
        )
        .await;
    assert_eq!(set.status, StatusCode::OK);
    assert_eq!(set.body["display_name"], "Ada");

    let clear = app
        .send(
            "PATCH",
            "/api/v1/me",
            Some(&cookie),
            Some(json!({ "display_name": " " })),
        )
        .await;
    assert_eq!(clear.body["display_name"], Value::Null);
}

#[sqlx::test(migrator = "akasha_db::MIGRATOR")]
async fn password_change_revokes_other_sessions(pool: PgPool) {
    let app = TestApp::new(pool);
    let current = app.register("ada@example.com", PW).await.session();
    let other = app.login("ada@example.com", PW).await.session();

    let bad = app
        .send(
            "POST",
            "/api/v1/me/password",
            Some(&current),
            Some(json!({ "current_password": "wrong password", "new_password": "a new password" })),
        )
        .await;
    assert_eq!(bad.status, StatusCode::UNAUTHORIZED);

    let ok = app
        .send(
            "POST",
            "/api/v1/me/password",
            Some(&current),
            Some(json!({ "current_password": PW, "new_password": "a new password" })),
        )
        .await;
    assert_eq!(ok.status, StatusCode::NO_CONTENT);

    assert_eq!(app.me_status(&current).await, StatusCode::OK);
    assert_eq!(app.me_status(&other).await, StatusCode::UNAUTHORIZED);
    assert_eq!(
        app.login("ada@example.com", PW).await.status,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        app.login("ada@example.com", "a new password").await.status,
        StatusCode::OK
    );
}

#[sqlx::test(migrator = "akasha_db::MIGRATOR")]
async fn delete_account_requires_password(pool: PgPool) {
    let app = TestApp::new(pool);
    let cookie = app.register("ada@example.com", PW).await.session();

    let wrong = app
        .send(
            "DELETE",
            "/api/v1/me",
            Some(&cookie),
            Some(json!({ "password": "nope nope" })),
        )
        .await;
    assert_eq!(wrong.status, StatusCode::UNAUTHORIZED);

    let ok = app
        .send(
            "DELETE",
            "/api/v1/me",
            Some(&cookie),
            Some(json!({ "password": PW })),
        )
        .await;
    assert_eq!(ok.status, StatusCode::NO_CONTENT);

    assert_eq!(
        app.send("GET", "/api/v1/me", Some(&cookie), None)
            .await
            .status,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        app.login("ada@example.com", PW).await.status,
        StatusCode::UNAUTHORIZED
    );
}

#[sqlx::test(migrator = "akasha_db::MIGRATOR")]
async fn login_is_rate_limited_per_ip(pool: PgPool) {
    let app = TestApp::new(pool);
    let mut last = StatusCode::OK;
    for _ in 0..11 {
        last = app.login("ada@example.com", "guessing guess").await.status;
    }
    assert_eq!(last, StatusCode::TOO_MANY_REQUESTS);
    let res = app.login("ada@example.com", "guessing guess").await;
    assert_eq!(res.body["error"]["code"], "rate_limited");
}
