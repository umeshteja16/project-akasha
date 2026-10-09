//! Anthropic Claude: the Messages API (`POST /v1/messages`) with SSE streaming.
//!
//! Current Claude models reject sampling parameters, so `temperature` is never
//! sent; thinking cannot be switched off on them either, so answers use a low
//! `effort` by default (configurable). Thinking blocks are not shown: only
//! `text_delta`s become output. For the models that support it, requests opt
//! into server-side refusal fallbacks (`fallbacks: "default"`).

use std::collections::VecDeque;

use futures_util::future::BoxFuture;
use serde_json::{Value, json};

use crate::{
    ApiKey, ChatEvent, ChatModel, ChatRequest, ChatStream, LlmError, StopReason, Usage,
    http::{Http, shorten},
    ollama::count,
    stream::{self, Sse, SseDecoder, SseEvent},
};

/// Used when `AKASHA_LLM_MODEL` is empty.
pub const DEFAULT_MODEL: &str = "claude-opus-5-5";
pub const DEFAULT_BASE_URL: &str = "https://api.anthropic.com";
pub const API_VERSION: &str = "2023-06-01";
/// Beta header that enables `fallbacks: "default"`.
pub const FALLBACK_BETA: &str = "server-side-fallback-2026-07-01";
/// Models that accept server-side refusal fallbacks.
const FALLBACK_MODELS: [&str; 4] = [
    "claude-opus-5-5",
    "claude-opus-5",
    "claude-fable-5-1",
    "claude-sonnet-5-5",
];

pub struct Anthropic {
    http: Http,
    base_url: String,
    key: ApiKey,
    model: String,
    /// `output_config.effort`; empty: the model's default.
    effort: String,
}

impl Anthropic {
    pub(crate) fn new(http: Http, base_url: &str, key: ApiKey, model: &str, effort: &str) -> Self {
        Self {
            http,
            base_url: base_url.trim_end_matches('/').to_owned(),
            key,
            model: model.to_owned(),
            effort: effort.trim().to_owned(),
        }
    }

    fn fallbacks(&self) -> bool {
        FALLBACK_MODELS.contains(&self.model.as_str())
    }

    pub(crate) fn body(&self, req: &ChatRequest) -> Value {
        let messages: Vec<Value> = req
            .messages
            .iter()
            .map(|m| json!({ "role": m.role, "content": m.content }))
            .collect();
        let mut body = json!({
            "model": self.model,
            "max_tokens": req.max_tokens,
            "messages": messages,
            "stream": true,
        });
        if !req.system.is_empty() {
            body["system"] = json!(req.system);
        }
        if !self.effort.is_empty() {
            body["output_config"] = json!({ "effort": self.effort });
        }
        if self.fallbacks() {
            body["fallbacks"] = json!("default");
        }
        body
    }
}

impl ChatModel for Anthropic {
    fn provider(&self) -> &'static str {
        "anthropic"
    }

    fn model(&self) -> &str {
        &self.model
    }

    fn stream<'a>(&'a self, req: &'a ChatRequest) -> BoxFuture<'a, Result<ChatStream, LlmError>> {
        Box::pin(async move {
            let url = format!("{}/v1/messages", self.base_url);
            let body = self.body(req);
            let fallbacks = self.fallbacks();
            let res = self
                .http
                .send(|c| {
                    let r = c
                        .post(&url)
                        .header("x-api-key", self.key.expose())
                        .header("anthropic-version", API_VERSION)
                        .json(&body);
                    if fallbacks {
                        r.header("anthropic-beta", FALLBACK_BETA)
                    } else {
                        r
                    }
                })
                .await?;
            Ok(stream::drive(
                stream::body(res),
                Sse::new(Events::default()),
            ))
        })
    }
}

/// `message_start` (input tokens) → `content_block_delta`s → `message_delta`
/// (stop reason, output tokens) → `message_stop`. `ping` and thinking or other
/// non-text deltas are ignored; an `error` event fails the stream.
#[derive(Default)]
pub(crate) struct Events {
    usage: Usage,
    stop: Option<StopReason>,
}

impl SseDecoder for Events {
    fn event(&mut self, event: SseEvent, out: &mut VecDeque<ChatEvent>) -> Result<(), LlmError> {
        let v = stream::json(&event.data)?;
        match v.get("type").and_then(Value::as_str).unwrap_or_default() {
            "message_start" => {
                if let Some(u) = v.pointer("/message/usage") {
                    self.usage.input_tokens = count(u, "input_tokens");
                }
            }
            "content_block_delta" => {
                if v.pointer("/delta/type").and_then(Value::as_str) == Some("text_delta")
                    && let Some(text) = v.pointer("/delta/text").and_then(Value::as_str)
                    && !text.is_empty()
                {
                    out.push_back(ChatEvent::Delta(text.to_owned()));
                }
            }
            "message_delta" => {
                if let Some(reason) = v.pointer("/delta/stop_reason").and_then(Value::as_str) {
                    self.stop = Some(match reason {
                        "end_turn" | "stop_sequence" => StopReason::EndTurn,
                        "max_tokens" => StopReason::MaxTokens,
                        "refusal" => StopReason::Refusal,
                        _ => StopReason::Other,
                    });
                }
                if let Some(u) = v.get("usage") {
                    if let Some(n) = count(u, "output_tokens") {
                        self.usage.output_tokens = Some(n);
                    }
                    if let Some(n) = count(u, "input_tokens") {
                        self.usage.input_tokens = Some(n);
                    }
                }
            }
            "message_stop" => out.push_back(ChatEvent::Done {
                stop: self.stop.unwrap_or(StopReason::EndTurn),
                usage: self.usage,
            }),
            "error" => {
                let msg = v
                    .pointer("/error/message")
                    .and_then(Value::as_str)
                    .unwrap_or("unknown error");
                return Err(LlmError::Upstream(shorten(msg)));
            }
            _ => {}
        }
        Ok(())
    }

    fn finish(&mut self, _out: &mut VecDeque<ChatEvent>) -> Result<(), LlmError> {
        Err(LlmError::Protocol(
            "the stream ended before `message_stop`".into(),
        ))
    }
}
