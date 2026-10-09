//! Helpers for the chat tests: SSE parsing and conversation requests.

use axum::http::{StatusCode, header};
use serde_json::{Value, json};

use super::TestApp;

/// Parse an SSE body into `(event, data)` pairs.
pub fn sse(bytes: &[u8]) -> Vec<(String, Value)> {
    let text = std::str::from_utf8(bytes).expect("utf-8");
    text.split("\n\n")
        .filter_map(|block| {
            let mut event = None;
            let mut data = String::new();
            for line in block.lines() {
                if let Some(e) = line.strip_prefix("event:") {
                    event = Some(e.trim().to_owned());
                } else if let Some(d) = line.strip_prefix("data:") {
                    data.push_str(d.trim_start());
                }
            }
            Some((event?, serde_json::from_str(&data).expect("json data")))
        })
        .collect()
}

pub fn find<'a>(events: &'a [(String, Value)], name: &str) -> &'a Value {
    &events
        .iter()
        .find(|(e, _)| e == name)
        .unwrap_or_else(|| panic!("no `{name}` event in {events:?}"))
        .1
}

pub async fn conversation(app: &TestApp, cookie: &str) -> String {
    let res = app
        .send("POST", "/api/v1/conversations", cookie, Some(json!({})))
        .await;
    assert_eq!(res.status, StatusCode::CREATED);
    res.json()["id"].as_str().expect("id").to_owned()
}

pub async fn ask(
    app: &TestApp,
    cookie: &str,
    conv: &str,
    body: Value,
) -> (StatusCode, Vec<(String, Value)>) {
    let res = app
        .send(
            "POST",
            &format!("/api/v1/conversations/{conv}/messages"),
            cookie,
            Some(body),
        )
        .await;
    if res.status != StatusCode::OK {
        return (res.status, vec![("http".into(), res.json())]);
    }
    assert_eq!(res.headers[header::CONTENT_TYPE], "text/event-stream");
    (res.status, sse(&res.bytes))
}
