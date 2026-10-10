//! `akasha mcp`: a stdio ⇄ Streamable HTTP bridge.
//!
//! MCP clients that only launch local processes (Claude Desktop, many IDEs)
//! run `akasha mcp`; it reads newline-delimited JSON-RPC from stdin, POSTs each
//! message to a running Akasha server's `/mcp` with the API token, and writes
//! the replies to stdout. All tools run on the server, so the bridge works the
//! same from a laptop against a home server as on the server itself, needs no
//! database or models, and can never disagree with the HTTP endpoint.
//! Logs go to stderr only (stdout is the protocol).

use std::{sync::Arc, time::Duration};

use reqwest::{
    Client, StatusCode,
    header::{ACCEPT, AUTHORIZATION, CONTENT_TYPE},
};
use serde_json::{Value, json};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    sync::{Mutex, RwLock},
    task::JoinSet,
};

const PROTOCOL_HEADER: &str = "MCP-Protocol-Version";
const TIMEOUT: Duration = Duration::from_secs(180);

pub struct BridgeOptions {
    /// Base URL of the Akasha server (`https://akasha.example.com`), or the full
    /// `/mcp` endpoint.
    pub url: String,
    pub token: String,
}

/// The `/mcp` endpoint for a base URL.
pub fn endpoint(url: &str) -> String {
    let url = url.trim().trim_end_matches('/');
    if url.ends_with(super::PATH) {
        url.to_owned()
    } else {
        format!("{url}{}", super::PATH)
    }
}

struct Bridge {
    client: Client,
    endpoint: String,
    auth: String,
    /// Negotiated in `initialize`, sent on every later request.
    protocol: RwLock<Option<String>>,
    stdout: Mutex<tokio::io::Stdout>,
}

pub async fn run(opts: BridgeOptions) -> anyhow::Result<()> {
    if opts.token.trim().is_empty() {
        anyhow::bail!("an API token is required: --token or AKASHA_TOKEN");
    }
    let bridge = Arc::new(Bridge {
        client: Client::builder().timeout(TIMEOUT).build()?,
        endpoint: endpoint(&opts.url),
        auth: format!("Bearer {}", opts.token.trim()),
        protocol: RwLock::new(None),
        stdout: Mutex::new(tokio::io::stdout()),
    });
    eprintln!("akasha mcp: forwarding stdio to {}", bridge.endpoint);
    let mut lines = BufReader::new(tokio::io::stdin()).lines();
    let mut tasks = JoinSet::new();
    while let Some(line) = lines.next_line().await? {
        if line.trim().is_empty() {
            continue;
        }
        let Ok(message) = serde_json::from_str::<Value>(&line) else {
            bridge
                .write(&json!({"jsonrpc": "2.0", "id": null,
                    "error": {"code": -32700, "message": "parse error"}}))
                .await;
            continue;
        };
        // `initialize` settles the protocol version before anything else runs.
        if message.get("method").and_then(Value::as_str) == Some("initialize") {
            bridge.forward(message).await;
        } else {
            let bridge = bridge.clone();
            tasks.spawn(async move { bridge.forward(message).await });
        }
        while tasks.try_join_next().is_some() {}
    }
    while tasks.join_next().await.is_some() {}
    Ok(())
}

impl Bridge {
    async fn forward(&self, message: Value) {
        let id = message.get("id").cloned();
        let is_request = id.is_some() && message.get("method").is_some();
        match self.post(&message).await {
            Ok(replies) => {
                for reply in replies {
                    self.remember_protocol(&reply).await;
                    self.write(&reply).await;
                }
            }
            Err(why) => {
                eprintln!("akasha mcp: {why}");
                if is_request {
                    self.write(&json!({"jsonrpc": "2.0", "id": id,
                        "error": {"code": -32603, "message": why}}))
                        .await;
                }
            }
        }
    }

    /// POST one message; the JSON-RPC messages that came back.
    async fn post(&self, message: &Value) -> Result<Vec<Value>, String> {
        let mut req = self
            .client
            .post(&self.endpoint)
            .header(AUTHORIZATION, &self.auth)
            .header(CONTENT_TYPE, "application/json")
            .header(ACCEPT, "application/json, text/event-stream")
            .body(message.to_string());
        if let Some(v) = self.protocol.read().await.as_deref() {
            req = req.header(PROTOCOL_HEADER, v);
        }
        let res = req
            .send()
            .await
            .map_err(|e| format!("cannot reach {}: {e}", self.endpoint))?;
        let status = res.status();
        let sse = res
            .headers()
            .get(CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .is_some_and(|v| v.starts_with("text/event-stream"));
        let body = res
            .text()
            .await
            .map_err(|e| format!("reading the reply failed: {e}"))?;
        if status == StatusCode::ACCEPTED || status == StatusCode::NO_CONTENT {
            return Ok(Vec::new());
        }
        if !status.is_success() {
            return Err(http_error(status, &body));
        }
        if sse {
            return Ok(parse_sse(&body));
        }
        if body.trim().is_empty() {
            return Ok(Vec::new());
        }
        serde_json::from_str(&body)
            .map(|v| vec![v])
            .map_err(|e| format!("the server sent invalid JSON: {e}"))
    }

    async fn remember_protocol(&self, reply: &Value) {
        if let Some(v) = reply
            .pointer("/result/protocolVersion")
            .and_then(Value::as_str)
        {
            *self.protocol.write().await = Some(v.to_owned());
        }
    }

    async fn write(&self, value: &Value) {
        let mut line = value.to_string();
        line.push('\n');
        let mut out = self.stdout.lock().await;
        if out.write_all(line.as_bytes()).await.is_err() || out.flush().await.is_err() {
            eprintln!("akasha mcp: stdout closed");
        }
    }
}

/// `401: invalid, expired or revoked API token`, from our error shape if possible.
fn http_error(status: StatusCode, body: &str) -> String {
    let message = serde_json::from_str::<Value>(body)
        .ok()
        .and_then(|v| {
            v.pointer("/error/message")
                .and_then(Value::as_str)
                .map(str::to_owned)
        })
        .unwrap_or_else(|| body.chars().take(300).collect());
    format!("server answered {}: {message}", status.as_u16())
}

/// The JSON payloads of an SSE body (`data:` lines, joined per event).
pub fn parse_sse(body: &str) -> Vec<Value> {
    let mut out = Vec::new();
    for event in body.split("\n\n") {
        let data: Vec<&str> = event
            .lines()
            .filter_map(|l| l.strip_prefix("data:"))
            .map(str::trim_start)
            .collect();
        if data.is_empty() {
            continue;
        }
        if let Ok(v) = serde_json::from_str::<Value>(&data.join("\n")) {
            out.push(v);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn endpoints_and_sse() {
        assert_eq!(endpoint("http://h:8080/"), "http://h:8080/mcp");
        assert_eq!(endpoint("https://h/mcp"), "https://h/mcp");
        let body =
            "id: 0\nretry: 3000\ndata:\n\ndata: {\"id\":1}\n\nevent: message\ndata: {\"id\":2}\n\n";
        let got = parse_sse(body);
        assert_eq!(got, vec![json!({"id": 1}), json!({"id": 2})]);
    }

    #[test]
    fn http_errors_use_the_api_message() {
        let body = r#"{"error":{"code":"unauthorized","message":"bad token"}}"#;
        assert_eq!(
            http_error(StatusCode::UNAUTHORIZED, body),
            "server answered 401: bad token"
        );
    }
}
