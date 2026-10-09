//! Conversation CRUD, paging and owner isolation, and the strict offline
//! startup check.

mod support;

use akasha_core::Config;
use axum::http::StatusCode;
use serde_json::{Value, json};
use sqlx::PgPool;
use support::{TestApp, chat::conversation, test_config};

#[sqlx::test(migrator = "akasha_db::MIGRATOR")]
async fn conversations_crud_paging_and_owner_isolation(pool: PgPool) {
    let app = TestApp::new(pool);
    let ada = app.user("ada@example.com").await;
    let bob = app.user("bob@example.com").await;

    let res = app
        .send(
            "POST",
            "/api/v1/conversations",
            &ada,
            Some(json!({ "title": "  Trip   plans " })),
        )
        .await;
    assert_eq!(res.status, StatusCode::CREATED);
    let first = res.json()["id"].as_str().expect("id").to_owned();
    assert_eq!(res.json()["title"], "Trip plans");
    let second = conversation(&app, &ada).await;

    let res = app
        .send("GET", "/api/v1/conversations?limit=1", &ada, None)
        .await;
    let page = res.json();
    assert_eq!(page["items"][0]["id"], second.as_str(), "most recent first");
    let cursor = page["next_cursor"].as_str().expect("cursor").to_owned();
    let res = app
        .send(
            "GET",
            &format!("/api/v1/conversations?limit=1&cursor={cursor}"),
            &ada,
            None,
        )
        .await;
    assert_eq!(res.json()["items"][0]["id"], first.as_str());
    assert_eq!(res.json()["next_cursor"], Value::Null);

    let path = format!("/api/v1/conversations/{first}");
    let res = app
        .send("PATCH", &path, &ada, Some(json!({ "title": "Japan" })))
        .await;
    assert_eq!(res.json()["title"], "Japan");
    let res = app
        .send("PATCH", &path, &ada, Some(json!({ "title": " " })))
        .await;
    assert_eq!(res.status, StatusCode::BAD_REQUEST);

    // Bob sees nothing of Ada's.
    let res = app.send("GET", "/api/v1/conversations", &bob, None).await;
    assert_eq!(res.json()["items"], json!([]));
    let messages = format!("{path}/messages");
    for (method, uri, body) in [
        ("GET", path.as_str(), None),
        ("PATCH", path.as_str(), Some(json!({ "title": "mine" }))),
        ("DELETE", path.as_str(), None),
        ("GET", messages.as_str(), None),
        ("POST", messages.as_str(), Some(json!({ "content": "hi" }))),
    ] {
        let res = app.send(method, uri, &bob, body).await;
        assert_eq!(res.status, StatusCode::NOT_FOUND, "{method} {uri}");
        assert_eq!(res.code(), "not_found");
    }
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM messages")
        .fetch_one(&app.db)
        .await
        .expect("count");
    assert_eq!(count, 0, "nothing was stored for bob's attempt");

    assert_eq!(
        app.send("DELETE", &path, &ada, None).await.status,
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        app.send("GET", &path, &ada, None).await.status,
        StatusCode::NOT_FOUND
    );
    let res = app.send("GET", "/api/v1/conversations", &ada, None).await;
    assert_eq!(res.json()["items"].as_array().map(Vec::len), Some(1));

    // Deleting the account removes the rest.
    let res = app
        .send(
            "DELETE",
            "/api/v1/me",
            &ada,
            Some(json!({ "password": support::PW })),
        )
        .await;
    assert!(res.status.is_success(), "{:?}", res.json());
    let left: i64 = sqlx::query_scalar("SELECT count(*) FROM conversations")
        .fetch_one(&app.db)
        .await
        .expect("count");
    assert_eq!(left, 0);
}

#[test]
fn strict_offline_refuses_cloud_providers_at_startup() {
    let cloud = Config {
        strict_offline: true,
        llm_provider: akasha_core::LlmProvider::Anthropic,
        anthropic_api_key: Some(akasha_core::Secret::new("sk-ant-x")),
        ..test_config()
    };
    let err = akasha::llm::build(&cloud).err().expect("refused");
    assert!(err.to_string().contains("strict offline"), "{err}");
    let local = Config {
        strict_offline: true,
        llm_provider: akasha_core::LlmProvider::Ollama,
        ..test_config()
    };
    assert!(akasha::llm::build(&local).expect("allowed").is_some());
    let remote_ollama = Config {
        ollama_url: "https://llm.example.com".into(),
        ..local
    };
    assert!(akasha::llm::build(&remote_ollama).is_err());
}
