//! Google Gemini: `POST /v1beta/models/{model}:streamGenerateContent?alt=sse`.
//!
//! The API key goes in the `x-goog-api-key` header. Never in the URL: the legacy
//! code put it in the query string, where it ends up in proxy and server logs.

use std::collections::VecDeque;

use futures_util::future::BoxFuture;
use serde_json::{Value, json};

use crate::{
    ApiKey, ChatEvent, ChatModel, ChatRequest, ChatStream, LlmError, Role, StopReason, Usage,
    http::{Http, shorten},
    ollama::count,
    stream::{self, Sse, SseDecoder, SseEvent},
};

/// Used when `AKASHA_LLM_MODEL` is empty.
pub const DEFAULT_MODEL: &str = "gemini-2.5-flash";
pub const DEFAULT_BASE_URL: &str = "https://generativelanguage.googleapis.com";

pub struct Gemini {
    http: Http,
    base_url: String,
    key: ApiKey,
    model: String,
}

impl Gemini {
    pub(crate) fn new(
        http: Http,
        base_url: &str,
        key: ApiKey,
        model: &str,
    ) -> Result<Self, LlmError> {
        let model = model.strip_prefix("models/").unwrap_or(model);
        // The model id becomes a path segment: allow only what ids look like.
        let valid = !model.is_empty()
            && model
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '.' | '_'));
        if !valid {
            return Err(LlmError::Config(format!(
                "invalid Gemini model id `{model}`"
            )));
        }
        Ok(Self {
            http,
            base_url: base_url.trim_end_matches('/').to_owned(),
            key,
            model: model.to_owned(),
        })
    }

    pub(crate) fn url(&self) -> String {
        format!(
            "{}/v1beta/models/{}:streamGenerateContent?alt=sse",
            self.base_url, self.model
        )
    }

    pub(crate) fn body(&self, req: &ChatRequest) -> Value {
        let contents: Vec<Value> = req
            .messages
            .iter()
            .map(|m| {
                let role = match m.role {
                    Role::User => "user",
                    Role::Assistant => "model",
                };
                json!({ "role": role, "parts": [{ "text": m.content }] })
            })
            .collect();
        let mut config = json!({ "maxOutputTokens": req.max_tokens });
        if let Some(t) = req.temperature {
            config["temperature"] = json!(t);
        }
        if req.json {
            config["responseMimeType"] = json!("application/json");
        }
        let mut body = json!({ "contents": contents, "generationConfig": config });
        if !req.system.is_empty() {
            body["systemInstruction"] = json!({ "parts": [{ "text": req.system }] });
        }
        body
    }
}

impl ChatModel for Gemini {
    fn provider(&self) -> &'static str {
        "gemini"
    }

    fn model(&self) -> &str {
        &self.model
    }

    fn stream<'a>(&'a self, req: &'a ChatRequest) -> BoxFuture<'a, Result<ChatStream, LlmError>> {
        Box::pin(async move {
            let url = self.url();
            let body = self.body(req);
            let res = self
                .http
                .send(|c| {
                    c.post(&url)
                        .header("x-goog-api-key", self.key.expose())
                        .json(&body)
                })
                .await?;
            Ok(stream::drive(
                stream::body(res),
                Sse::new(Events::default()),
            ))
        })
    }
}

/// Each event is a `GenerateContentResponse`; the last one carries
/// `finishReason`. The stream simply ends (there is no terminator event).
#[derive(Default)]
pub(crate) struct Events {
    usage: Usage,
    stop: Option<StopReason>,
}

impl SseDecoder for Events {
    fn event(&mut self, event: SseEvent, out: &mut VecDeque<ChatEvent>) -> Result<(), LlmError> {
        let v = stream::json(&event.data)?;
        if let Some(msg) = v.pointer("/error/message").and_then(Value::as_str) {
            return Err(LlmError::Upstream(shorten(msg)));
        }
        if v.pointer("/promptFeedback/blockReason").is_some() {
            self.stop = Some(StopReason::Refusal);
        }
        if let Some(candidate) = v.pointer("/candidates/0") {
            let parts = candidate
                .pointer("/content/parts")
                .and_then(Value::as_array);
            for part in parts.into_iter().flatten() {
                // Thought summaries are not part of the answer.
                if part.get("thought").and_then(Value::as_bool) == Some(true) {
                    continue;
                }
                if let Some(text) = part.get("text").and_then(Value::as_str)
                    && !text.is_empty()
                {
                    out.push_back(ChatEvent::Delta(text.to_owned()));
                }
            }
            if let Some(reason) = candidate.get("finishReason").and_then(Value::as_str) {
                self.stop = Some(match reason {
                    "STOP" => StopReason::EndTurn,
                    "MAX_TOKENS" => StopReason::MaxTokens,
                    "SAFETY" | "RECITATION" | "BLOCKLIST" | "PROHIBITED_CONTENT" | "SPII" => {
                        StopReason::Refusal
                    }
                    _ => StopReason::Other,
                });
            }
        }
        if let Some(u) = v.get("usageMetadata") {
            self.usage = Usage {
                input_tokens: count(u, "promptTokenCount").or(self.usage.input_tokens),
                output_tokens: count(u, "candidatesTokenCount").or(self.usage.output_tokens),
            };
        }
        Ok(())
    }

    fn finish(&mut self, out: &mut VecDeque<ChatEvent>) -> Result<(), LlmError> {
        let stop = self
            .stop
            .ok_or_else(|| LlmError::Protocol("the stream ended without a finish reason".into()))?;
        out.push_back(ChatEvent::Done {
            stop,
            usage: self.usage,
        });
        Ok(())
    }
}
