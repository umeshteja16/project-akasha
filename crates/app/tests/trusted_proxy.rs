//! Client addresses behind a reverse proxy (`AKASHA_TRUSTED_PROXIES`): the
//! credential rate limiter, sessions and the security log all see the real
//! client, and only when the TCP peer is a trusted proxy.
//! (The test router's TCP peer is always 127.0.0.1.)

mod support;

use axum::{
    body::Body,
    http::{Request, StatusCode, header},
};
use serde_json::{Value, json};
use sqlx::PgPool;
use support::{PW, Reply, TestApp, test_config};

async fn login(app: &TestApp, email: &str, password: &str, headers: &[(&str, &str)]) -> Reply {
    let mut req =
        Request::post("/api/v1/auth/login").header(header::CONTENT_TYPE, "application/json");
    for (name, value) in headers {
        req = req.header(*name, *value);
    }
    let body = Body::from(json!({ "email": email, "password": password }).to_string());
    app.request(req.body(body).expect("request")).await
}

fn cookie(reply: &Reply) -> String {
    let raw = reply.headers[header::SET_COOKIE].to_str().expect("cookie");
    raw.split(';').next().expect("pair").to_owned()
}

async fn session_ips(app: &TestApp, cookie: &str) -> Vec<Value> {
    let list = app
        .send("GET", "/api/v1/me/sessions", cookie, None)
        .await
        .json();
    let items = list["items"].as_array().expect("items");
    items.iter().map(|s| s["ip"].clone()).collect()
}

async fn sign_in_ips(app: &TestApp, cookie: &str) -> Vec<Value> {
    let path = "/api/v1/activity?category=security";
    let list = app.send("GET", path, cookie, None).await.json();
    let items = list["items"].as_array().expect("items");
    items
        .iter()
        .filter(|e| e["kind"] == "auth.signed_in")
        .map(|e| e["ip"].clone())
        .collect()
}

#[sqlx::test(migrator = "akasha_db::MIGRATOR")]
async fn forwarding_headers_from_untrusted_peers_are_ignored(pool: PgPool) {
    let app = TestApp::new(pool); // trusts nobody (the default)
    app.user("ada@example.com").await;

    let spoofed = [
        ("x-forwarded-for", "198.51.100.7"),
        ("forwarded", "for=198.51.100.8;proto=https"),
        ("x-forwarded-proto", "https"),
    ];
    let ok = login(&app, "ada@example.com", PW, &spoofed).await;
    assert_eq!(ok.status, StatusCode::OK);
    let set_cookie = ok.headers[header::SET_COOKIE].to_str().expect("cookie");
    assert!(!set_cookie.contains("Secure"), "{set_cookie}");
    let session = cookie(&ok);
    for ip in session_ips(&app, &session).await {
        assert_eq!(ip, "127.0.0.1");
    }
    assert_eq!(sign_in_ips(&app, &session).await, ["127.0.0.1"]);

    // A different forged address on every attempt still shares one bucket.
    // (register + 1 login above used 2 of the burst of 10.)
    let mut last = StatusCode::OK;
    for n in 0..9 {
        let xff = format!("203.0.113.{n}");
        let reply = login(
            &app,
            "ada@example.com",
            "wrong",
            &[("x-forwarded-for", &xff)],
        )
        .await;
        last = reply.status;
    }
    assert_eq!(last, StatusCode::TOO_MANY_REQUESTS);
}

#[sqlx::test(migrator = "akasha_db::MIGRATOR")]
async fn trusted_proxies_reveal_the_client_and_its_scheme(pool: PgPool) {
    let config = akasha_core::Config {
        trusted_proxies: vec!["127.0.0.1".into(), "10.0.0.0/8".into()],
        ..test_config()
    };
    let app = TestApp::with_config(pool, config);
    app.user("ada@example.com").await;

    // Client → CDN edge (10.1.1.1) → our proxy (127.0.0.1). The leftmost entry was
    // sent by the client and is ignored.
    let chain = [
        ("x-forwarded-for", "6.6.6.6, 198.51.100.7, 10.1.1.1"),
        ("x-forwarded-proto", "https"),
    ];
    let ok = login(&app, "ada@example.com", PW, &chain).await;
    assert_eq!(ok.status, StatusCode::OK);
    let set_cookie = ok.headers[header::SET_COOKIE].to_str().expect("cookie");
    assert!(
        set_cookie.contains("Secure"),
        "https via proxy: {set_cookie}"
    );
    let session = cookie(&ok);
    let ips = session_ips(&app, &session).await;
    assert!(ips.contains(&json!("198.51.100.7")), "{ips:?}");
    assert_eq!(sign_in_ips(&app, &session).await, ["198.51.100.7"]);

    // RFC 7239 `Forwarded` works when there is no X-Forwarded-For.
    let fwd = [("forwarded", "for=\"[2001:db8::7]:5000\";proto=http")];
    let ok = login(&app, "ada@example.com", PW, &fwd).await;
    assert_eq!(ok.status, StatusCode::OK);
    let set_cookie = ok.headers[header::SET_COOKIE].to_str().expect("cookie");
    assert!(!set_cookie.contains("Secure"), "{set_cookie}");
    let ips = session_ips(&app, &cookie(&ok)).await;
    assert!(ips.contains(&json!("2001:db8::7")), "{ips:?}");
}

#[sqlx::test(migrator = "akasha_db::MIGRATOR")]
async fn behind_a_trusted_proxy_rate_limits_are_per_client(pool: PgPool) {
    let config = akasha_core::Config {
        trusted_proxies: vec!["127.0.0.0/8".into()],
        ..test_config()
    };
    let app = TestApp::with_config(pool, config);
    app.user("ada@example.com").await; // the proxy itself (no header): another bucket

    let mallory = [("x-forwarded-for", "203.0.113.66")];
    let mut statuses = Vec::new();
    for _ in 0..11 {
        statuses.push(
            login(&app, "ada@example.com", "wrong", &mallory)
                .await
                .status,
        );
    }
    assert_eq!(statuses[..10], [StatusCode::UNAUTHORIZED; 10]);
    assert_eq!(statuses[10], StatusCode::TOO_MANY_REQUESTS);

    // Everyone else behind the same proxy can still sign in.
    let ada = [("x-forwarded-for", "198.51.100.7")];
    assert_eq!(
        login(&app, "ada@example.com", PW, &ada).await.status,
        StatusCode::OK
    );
    // Forging a header in front of the proxy's entry does not escape the limit.
    let forged = [("x-forwarded-for", "198.51.100.7, 203.0.113.66")];
    assert_eq!(
        login(&app, "ada@example.com", PW, &forged).await.status,
        StatusCode::TOO_MANY_REQUESTS
    );
}
