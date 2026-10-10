//! Helpers for API-token and MCP tests.

use axum::{
    body::Body,
    http::{Request, header},
};
use serde_json::{Value, json};

use super::{Reply, TestApp};

/// Create an API token with the session `cookie`; returns the secret.
pub async fn token(app: &TestApp, cookie: &str, scopes: &[&str]) -> String {
    let reply = app
        .send(
            "POST",
            "/api/v1/me/tokens",
            cookie,
            Some(json!({ "name": "test", "scopes": scopes })),
        )
        .await;
    assert_eq!(reply.status, 201, "{:?}", reply.json());
    reply.json()["secret"].as_str().expect("secret").to_owned()
}

/// A request authenticated with a bearer token (no cookie).
pub async fn bearer(
    app: &TestApp,
    method: &str,
    path: &str,
    token: &str,
    body: Option<Value>,
) -> Reply {
    let req = Request::builder()
        .method(method)
        .uri(path)
        .header(header::AUTHORIZATION, format!("Bearer {token}"));
    let req = match body {
        Some(json) => req
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(json.to_string())),
        None => req.body(Body::empty()),
    };
    app.request(req.expect("request")).await
}

/// POST one JSON-RPC message to `/mcp`.
pub async fn rpc(app: &TestApp, token: Option<&str>, message: Value) -> Reply {
    let mut req = Request::post("/mcp")
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::ACCEPT, "application/json, text/event-stream")
        .header("MCP-Protocol-Version", "2025-06-18")
        .header(header::HOST, "akasha.example.com");
    if let Some(t) = token {
        req = req.header(header::AUTHORIZATION, format!("Bearer {t}"));
    }
    app.request(req.body(Body::from(message.to_string())).expect("request"))
        .await
}

/// Call a method; the JSON-RPC `result` (panics on a protocol error).
pub async fn call(app: &TestApp, token: &str, method: &str, params: Value) -> Value {
    let reply = rpc(
        app,
        Some(token),
        json!({ "jsonrpc": "2.0", "id": 1, "method": method, "params": params }),
    )
    .await;
    assert_eq!(
        reply.status,
        200,
        "{}",
        String::from_utf8_lossy(&reply.bytes)
    );
    let body = reply.json();
    assert!(body.get("error").is_none(), "{body}");
    body["result"].clone()
}

/// Call a tool: `(is_error, parsed JSON or raw text)`.
pub async fn tool(app: &TestApp, token: &str, name: &str, args: Value) -> (bool, Value) {
    let result = call(
        app,
        token,
        "tools/call",
        json!({ "name": name, "arguments": args }),
    )
    .await;
    let is_error = result["isError"].as_bool().unwrap_or(false);
    let text = result["content"][0]["text"]
        .as_str()
        .expect("text content")
        .to_owned();
    let value = serde_json::from_str(&text).unwrap_or(Value::String(text));
    (is_error, value)
}
