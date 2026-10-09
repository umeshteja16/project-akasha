//! OpenAI-compatible chat completions (`POST {base}/chat/completions`, SSE).
//! The base URL includes the version path: `https://api.openai.com/v1`,
//! `http://localhost:1234/v1` (LM Studio), `http://localhost:8000/v1` (vLLM),
//! `http://localhost:8080/v1` (llama.cpp server), `https://openrouter.ai/api/v1`.

use std::collections::VecDeque;

use futures_util::future::BoxFuture;
use serde_json::{Value, json};

use crate::{
    ApiKey, ChatEvent, ChatModel, ChatRequest, ChatStream, LlmError, StopReason, Usage,
    http::{Http, shorten},
    ollama::count,
    stream::{self, Sse, SseDecoder, SseEvent},
};

pub const DEFAULT_BASE_URL: &str = "https://api.openai.com/v1";

pub struct OpenAi {
    http: Http,
    base_url: String,
    /// Optional: local servers usually need none.
    key: Option<ApiKey>,
    model: String,
}

impl OpenAi {
    pub(crate) fn new(http: Http, base_url: &str, key: Option<ApiKey>, model: &str) -> Self {
        Self {
            http,
            base_url: base_url.trim_end_matches('/').to_owned(),
            key,
            model: model.to_owned(),
        }
    }

    pub(crate) fn body(&self, req: &ChatRequest) -> Value {
        let mut messages = Vec::with_capacity(req.messages.len() + 1);
        if !req.system.is_empty() {
            messages.push(json!({ "role": "system", "content": req.system }));
        }
        messages.extend(
            req.messages
                .iter()
                .map(|m| json!({ "role": m.role, "content": m.content })),
        );
        // `max_tokens` (not `max_completion_tokens`): every compatible server knows it.
        let mut body = json!({
            "model": self.model,
            "messages": messages,
            "max_tokens": req.max_tokens,
            "stream": true,
            "stream_options": { "include_usage": true },
        });
        if let Some(t) = req.temperature {
            body["temperature"] = json!(t);
        }
        body
    }
}

impl ChatModel for OpenAi {
    fn provider(&self) -> &'static str {
        "openai"
    }

    fn model(&self) -> &str {
        &self.model
    }

    fn stream<'a>(&'a self, req: &'a ChatRequest) -> BoxFuture<'a, Result<ChatStream, LlmError>> {
        Box::pin(async move {
            let url = format!("{}/chat/completions", self.base_url);
            let body = self.body(req);
            let res = self
                .http
                .send(|c| {
                    let r = c.post(&url).json(&body);
                    match &self.key {
                        Some(key) => r.bearer_auth(key.expose()),
                        None => r,
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

/// `data: {"choices":[{"delta":{"content":"..."}}]}` chunks, one with
/// `finish_reason`, an optional usage-only chunk, then `data: [DONE]`.
#[derive(Default)]
pub(crate) struct Events {
    usage: Usage,
    stop: Option<StopReason>,
}

impl Events {
    fn done(&self) -> ChatEvent {
        ChatEvent::Done {
            stop: self.stop.unwrap_or(StopReason::EndTurn),
            usage: self.usage,
        }
    }
}

impl SseDecoder for Events {
    fn event(&mut self, event: SseEvent, out: &mut VecDeque<ChatEvent>) -> Result<(), LlmError> {
        if event.data.trim() == "[DONE]" {
            out.push_back(self.done());
            return Ok(());
        }
        let v = stream::json(&event.data)?;
        if let Some(err) = v.get("error") {
            let msg = err
                .get("message")
                .and_then(Value::as_str)
                .map_or_else(|| err.to_string(), str::to_owned);
            return Err(LlmError::Upstream(shorten(&msg)));
        }
        if let Some(choice) = v.pointer("/choices/0") {
            if let Some(text) = choice.pointer("/delta/content").and_then(Value::as_str)
                && !text.is_empty()
            {
                out.push_back(ChatEvent::Delta(text.to_owned()));
            }
            if let Some(reason) = choice.get("finish_reason").and_then(Value::as_str) {
                self.stop = Some(match reason {
                    "stop" => StopReason::EndTurn,
                    "length" => StopReason::MaxTokens,
                    "content_filter" => StopReason::Refusal,
                    _ => StopReason::Other,
                });
            }
        }
        if let Some(u) = v.get("usage").filter(|u| u.is_object()) {
            self.usage = Usage {
                input_tokens: count(u, "prompt_tokens"),
                output_tokens: count(u, "completion_tokens"),
            };
        }
        Ok(())
    }

    /// Some servers omit `[DONE]`; a finish reason is enough to call it complete.
    fn finish(&mut self, out: &mut VecDeque<ChatEvent>) -> Result<(), LlmError> {
        if self.stop.is_none() {
            return Err(LlmError::Protocol(
                "the stream ended before `[DONE]`".into(),
            ));
        }
        out.push_back(self.done());
        Ok(())
    }
}
