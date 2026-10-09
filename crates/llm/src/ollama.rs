//! Ollama (`POST /api/chat`, newline-delimited JSON). Runs on the user's own
//! machine, so it is the provider strict offline mode allows (ADR 0003).

use std::collections::VecDeque;

use futures_util::future::BoxFuture;
use serde_json::{Value, json};

use crate::{
    ChatEvent, ChatModel, ChatRequest, ChatStream, LlmError, StopReason, Usage,
    http::{Http, shorten},
    stream::{self, Decoder},
};

/// Used when `AKASHA_LLM_MODEL` is empty.
pub const DEFAULT_MODEL: &str = "llama3.1:8b";
pub const DEFAULT_URL: &str = "http://localhost:11434";

pub struct Ollama {
    http: Http,
    url: String,
    model: String,
    num_ctx: u32,
}

impl Ollama {
    /// `num_ctx`: context window to request; Ollama's own default (2-4k tokens)
    /// is too small for several retrieved passages.
    pub(crate) fn new(http: Http, url: &str, model: &str, num_ctx: u32) -> Self {
        Self {
            http,
            url: url.trim_end_matches('/').to_owned(),
            model: model.to_owned(),
            num_ctx,
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
        let mut options = json!({ "num_predict": req.max_tokens, "num_ctx": self.num_ctx });
        if let Some(t) = req.temperature {
            options["temperature"] = json!(t);
        }
        json!({ "model": self.model, "messages": messages, "stream": true, "options": options })
    }
}

impl ChatModel for Ollama {
    fn provider(&self) -> &'static str {
        "ollama"
    }

    fn model(&self) -> &str {
        &self.model
    }

    fn stream<'a>(&'a self, req: &'a ChatRequest) -> BoxFuture<'a, Result<ChatStream, LlmError>> {
        Box::pin(async move {
            let url = format!("{}/api/chat", self.url);
            let body = self.body(req);
            let res = self.http.send(|c| c.post(&url).json(&body)).await?;
            Ok(stream::drive(stream::body(res), NdJson))
        })
    }
}

/// One JSON object per line: `{"message":{"content":"..."},"done":false}`, then a
/// final `{"done":true,"done_reason":"stop","prompt_eval_count":..,"eval_count":..}`.
pub(crate) struct NdJson;

impl Decoder for NdJson {
    fn line(&mut self, line: &str, out: &mut VecDeque<ChatEvent>) -> Result<(), LlmError> {
        if line.trim().is_empty() {
            return Ok(());
        }
        let v = stream::json(line)?;
        if let Some(err) = v.get("error") {
            let msg = err.as_str().map_or_else(|| err.to_string(), str::to_owned);
            return Err(LlmError::Upstream(shorten(&msg)));
        }
        if let Some(text) = v.pointer("/message/content").and_then(Value::as_str)
            && !text.is_empty()
        {
            out.push_back(ChatEvent::Delta(text.to_owned()));
        }
        if v.get("done").and_then(Value::as_bool) == Some(true) {
            let stop = match v.get("done_reason").and_then(Value::as_str) {
                Some("length") => StopReason::MaxTokens,
                Some("stop") | None => StopReason::EndTurn,
                Some(_) => StopReason::Other,
            };
            let usage = Usage {
                input_tokens: count(&v, "prompt_eval_count"),
                output_tokens: count(&v, "eval_count"),
            };
            out.push_back(ChatEvent::Done { stop, usage });
        }
        Ok(())
    }

    fn finish(&mut self, _out: &mut VecDeque<ChatEvent>) -> Result<(), LlmError> {
        Err(LlmError::Protocol("the stream ended before `done`".into()))
    }
}

pub(crate) fn count(v: &Value, key: &str) -> Option<u32> {
    v.get(key)?.as_u64().and_then(|n| u32::try_from(n).ok())
}
