//! The MCP server at `/mcp`, driven with raw JSON-RPC over the Streamable HTTP
//! transport: handshake, tools, scopes, owner isolation, resources.

mod support;

use serde_json::{Value, json};
use sqlx::PgPool;
use support::{
    TestApp,
    mcp::{call, rpc, token, tool},
};

const APOLLO: &[u8] = b"The Apollo 11 lunar module Eagle landed in the Sea of Tranquility in 1969.";

async fn seeded(pool: PgPool) -> (TestApp, String) {
    let app = TestApp::new(pool);
    let cookie = app.user("a@example.com").await;
    let r = app.upload(&cookie, "apollo.txt", APOLLO).await;
    assert_eq!(r.status, 201);
    app.run_jobs().await;
    (app, cookie)
}

fn names(tools: &Value) -> Vec<String> {
    tools["tools"]
        .as_array()
        .expect("tools")
        .iter()
        .map(|t| t["name"].as_str().expect("name").to_owned())
        .collect()
}

#[sqlx::test(migrator = "akasha_db::MIGRATOR")]
async fn handshake_and_tool_listing(pool: PgPool) {
    let (app, cookie) = seeded(pool).await;
    let read = token(&app, &cookie, &["read"]).await;
    let init = call(
        &app,
        &read,
        "initialize",
        json!({
            "protocolVersion": "2025-06-18",
            "capabilities": {},
            "clientInfo": { "name": "test", "version": "1" }
        }),
    )
    .await;
    assert_eq!(init["serverInfo"]["name"], "akasha");
    assert!(init["capabilities"]["tools"].is_object());
    assert!(
        init["instructions"]
            .as_str()
            .expect("instructions")
            .contains("search")
    );

    let tools = call(&app, &read, "tools/list", json!({})).await;
    assert_eq!(
        names(&tools),
        ["search", "get_file", "read_file", "list_files", "ask"]
    );
    let search = &tools["tools"][0];
    assert_eq!(search["inputSchema"]["required"], json!(["query"]));
    assert_eq!(search["annotations"]["readOnlyHint"], true);

    let write = token(&app, &cookie, &["read", "write"]).await;
    let tools = call(&app, &write, "tools/list", json!({})).await;
    assert!(names(&tools).contains(&"add_note".to_owned()));
    assert!(names(&tools).contains(&"tag_file".to_owned()));
}

#[sqlx::test(migrator = "akasha_db::MIGRATOR")]
async fn missing_or_invalid_tokens_get_401(pool: PgPool) {
    let (app, cookie) = seeded(pool).await;
    let ping = json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/list" });
    let r = rpc(&app, None, ping.clone()).await;
    assert_eq!(r.status, 401);
    assert!(r.headers.contains_key("www-authenticate"));
    let r = rpc(&app, Some("akasha_pat_wrong"), ping.clone()).await;
    assert_eq!(r.status, 401);
    // The session cookie is not enough here.
    let req = axum::http::Request::post("/mcp")
        .header("host", "localhost")
        .header("cookie", &cookie)
        .header("content-type", "application/json")
        .header("accept", "application/json, text/event-stream")
        .body(axum::body::Body::from(ping.to_string()))
        .expect("request");
    assert_eq!(app.request(req).await.status, 401);
}

#[sqlx::test(migrator = "akasha_db::MIGRATOR")]
async fn search_read_and_browse(pool: PgPool) {
    let (app, cookie) = seeded(pool).await;
    let t = token(&app, &cookie, &["read"]).await;

    let (err, res) = tool(&app, &t, "search", json!({ "query": "lunar module eagle" })).await;
    assert!(!err, "{res}");
    let hit = &res["results"][0];
    assert_eq!(hit["file_name"], "apollo.txt");
    assert!(hit["text"].as_str().expect("text").contains("Tranquility"));
    let file_id = hit["file_id"].as_str().expect("id").to_owned();

    let (err, res) = tool(
        &app,
        &t,
        "read_file",
        json!({ "file_id": file_id, "offset": 4, "max_chars": 100 }),
    )
    .await;
    assert!(!err, "{res}");
    assert!(res["text"].as_str().expect("text").starts_with("Apollo 11"));
    assert_eq!(res["next_offset"], Value::Null);

    let (err, res) = tool(&app, &t, "get_file", json!({ "file_id": file_id })).await;
    assert!(!err, "{res}");
    assert_eq!(res["name"], "apollo.txt");
    assert_eq!(res["text"]["char_count"], APOLLO.len());

    let (err, res) = tool(&app, &t, "list_files", json!({ "limit": 5 })).await;
    assert!(!err, "{res}");
    assert_eq!(res["files"][0]["file_id"], file_id.as_str());

    // Bad arguments are tool errors the model can read, not protocol errors.
    let (err, msg) = tool(&app, &t, "search", json!({ "query": "x", "limit": 99 })).await;
    assert!(err);
    assert!(msg.as_str().expect("message").contains("limit"));
    let (err, _) = tool(&app, &t, "read_file", json!({ "nope": 1 })).await;
    assert!(err);

    // Unknown tools are protocol errors.
    let r = rpc(
        &app,
        Some(&t),
        json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/call",
                "params": { "name": "rm_rf", "arguments": {} } }),
    )
    .await;
    assert!(r.json()["error"].is_object());

    // ask without a language model on the server returns the passages.
    let app2 = TestApp::with_llm(app.db.clone(), support::test_config(), None);
    let (err, res) = tool(
        &app2,
        &t,
        "ask",
        json!({ "question": "Where did the lunar module Eagle land?" }),
    )
    .await;
    assert!(!err, "{res}");
    assert_eq!(res["status"], "no_llm");
    assert_eq!(res["passages"][0]["file_name"], "apollo.txt");
}

#[sqlx::test(migrator = "akasha_db::MIGRATOR")]
async fn ask_answers_with_citations(pool: PgPool) {
    let (app, cookie) = seeded(pool).await;
    let t = token(&app, &cookie, &["read"]).await;
    let (err, res) = tool(
        &app,
        &t,
        "ask",
        json!({ "question": "Where did the lunar module Eagle land?" }),
    )
    .await;
    assert!(!err, "{res}");
    assert_eq!(res["status"], "answered", "{res}");
    assert_eq!(res["citations"][0]["file_name"], "apollo.txt");
    let (_, res) = tool(
        &app,
        &t,
        "ask",
        json!({ "question": "best sourdough hydration ratio" }),
    )
    .await;
    assert_eq!(res["status"], "not_found", "{res}");
}

#[sqlx::test(migrator = "akasha_db::MIGRATOR")]
async fn write_tools_need_the_write_scope(pool: PgPool) {
    let (app, cookie) = seeded(pool).await;
    let read = token(&app, &cookie, &["read"]).await;
    let write = token(&app, &cookie, &["read", "write"]).await;
    let note = json!({ "title": "Garden plan", "content": "Plant zucchini by the north fence.", "tags": ["Garden"] });

    let (err, msg) = tool(&app, &read, "add_note", note.clone()).await;
    assert!(err);
    assert!(msg.as_str().expect("message").contains("read-only"));

    let (err, res) = tool(&app, &write, "add_note", note.clone()).await;
    assert!(!err, "{res}");
    assert_eq!(res["name"], "Garden plan.md");
    assert_eq!(res["created"], true);
    assert_eq!(res["tags"], json!(["garden"]));
    let id = res["file_id"].as_str().expect("id").to_owned();
    app.run_jobs().await;
    let (_, res) = tool(&app, &read, "search", json!({ "query": "zucchini" })).await;
    assert_eq!(res["results"][0]["file_id"], id.as_str());
    // Same content again: nothing new.
    let (_, res) = tool(&app, &write, "add_note", note).await;
    assert_eq!(res["created"], false);

    let (err, _) = tool(
        &app,
        &read,
        "tag_file",
        json!({ "file_id": id, "add": ["x"] }),
    )
    .await;
    assert!(err);
    let (err, res) = tool(
        &app,
        &write,
        "tag_file",
        json!({ "file_id": id, "add": ["Veg", "x"], "remove": ["garden"] }),
    )
    .await;
    assert!(!err, "{res}");
    assert_eq!(res["tags"], json!(["veg", "x"]));
}

#[sqlx::test(migrator = "akasha_db::MIGRATOR")]
async fn tokens_only_see_their_owners_files(pool: PgPool) {
    let (app, cookie_a) = seeded(pool).await;
    let cookie_b = app.user("b@example.com").await;
    let token_b = token(&app, &cookie_b, &["read", "write"]).await;
    let token_a = token(&app, &cookie_a, &["read"]).await;
    let (_, res) = tool(&app, &token_a, "list_files", json!({})).await;
    let a_file = res["files"][0]["file_id"].as_str().expect("id").to_owned();

    let (_, res) = tool(
        &app,
        &token_b,
        "search",
        json!({ "query": "lunar module eagle", "mode": "keyword" }),
    )
    .await;
    assert_eq!(res["results"], json!([]));
    let (_, res) = tool(&app, &token_b, "list_files", json!({})).await;
    assert_eq!(res["files"], json!([]));
    for (name, args) in [
        ("get_file", json!({ "file_id": a_file })),
        ("read_file", json!({ "file_id": a_file })),
        ("tag_file", json!({ "file_id": a_file, "add": ["mine"] })),
    ] {
        let (err, msg) = tool(&app, &token_b, name, args).await;
        assert!(err, "{name}");
        assert!(
            msg.as_str().expect("message").contains("no file"),
            "{name}: {msg}"
        );
    }
    let r = rpc(
        &app,
        Some(&token_b),
        json!({ "jsonrpc": "2.0", "id": 3, "method": "resources/read",
                "params": { "uri": format!("akasha://file/{a_file}") } }),
    )
    .await;
    assert!(r.json()["error"].is_object());
}

#[sqlx::test(migrator = "akasha_db::MIGRATOR")]
async fn files_are_resources(pool: PgPool) {
    let (app, cookie) = seeded(pool).await;
    let t = token(&app, &cookie, &["read"]).await;
    let templates = call(&app, &t, "resources/templates/list", json!({})).await;
    assert_eq!(
        templates["resourceTemplates"][0]["uriTemplate"],
        "akasha://file/{id}"
    );
    let list = call(&app, &t, "resources/list", json!({})).await;
    let uri = list["resources"][0]["uri"]
        .as_str()
        .expect("uri")
        .to_owned();
    assert_eq!(list["resources"][0]["name"], "apollo.txt");
    let read = call(&app, &t, "resources/read", json!({ "uri": uri })).await;
    assert!(
        read["contents"][0]["text"]
            .as_str()
            .expect("text")
            .contains("Eagle")
    );
}
