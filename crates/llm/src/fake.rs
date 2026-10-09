//! A deterministic chat model for tests and model-less development
//! (`AKASHA_LLM_PROVIDER=fake`). It never touches the network.
//!
//! If the last user message contains numbered sources (lines starting with
//! `[n]`), the answer cites each of them once: `Fact one [1]. Fact two [2].`
//! Otherwise it echoes the last line of the last user message (which makes
//! "rewrite this question" prompts return the question). Output arrives one
//! word per delta, optionally with a delay, so streaming and cancellation can
//! be tested.
//!
//! With `json: true` it answers `{"summary": .., "tags": [..]}` about the last
//! user message after its first line (the header line, e.g. a file name): the
//! first sentence as the summary and the three most frequent words of five or
//! more letters as tags.

use std::time::Duration;

use futures_util::{StreamExt, future::BoxFuture, stream};

use crate::{ChatEvent, ChatModel, ChatRequest, ChatStream, LlmError, Role, StopReason, Usage};

pub const MODEL: &str = "fake-echo";

#[derive(Debug, Clone, Default)]
pub struct FakeChatModel {
    /// Pause before each delta.
    pub delay: Duration,
    /// Fail with an upstream error after this many deltas.
    pub fail_after: Option<usize>,
}

impl FakeChatModel {
    pub fn new() -> Self {
        Self::default()
    }

    /// The answer this model gives to `req`.
    pub fn answer(req: &ChatRequest) -> String {
        let last = req
            .messages
            .iter()
            .rev()
            .find(|m| m.role == Role::User)
            .map_or("", |m| m.content.as_str());
        if req.json {
            return describe(last);
        }
        let sources = source_numbers(last);
        if sources.is_empty() {
            return last
                .lines()
                .rev()
                .find(|l| !l.trim().is_empty())
                .unwrap_or("")
                .trim()
                .to_owned();
        }
        sources
            .iter()
            .map(|n| format!("Fact from source {n} [{n}]."))
            .collect::<Vec<_>>()
            .join(" ")
    }
}

/// Numbers `n` of lines that start with `[n]`, in order, without repeats.
fn source_numbers(text: &str) -> Vec<u32> {
    let mut found = Vec::new();
    for line in text.lines() {
        let Some(rest) = line.trim_start().strip_prefix('[') else {
            continue;
        };
        let Some((num, _)) = rest.split_once(']') else {
            continue;
        };
        if let Ok(n) = num.parse::<u32>()
            && !found.contains(&n)
        {
            found.push(n);
        }
    }
    found
}

/// The JSON-mode answer: a summary and tags for `text` without its first line.
fn describe(text: &str) -> String {
    let body = text.split_once('\n').map_or("", |(_, rest)| rest).trim();
    let summary = body
        .split_inclusive(['.', '!', '?'])
        .next()
        .unwrap_or("")
        .split_whitespace()
        .take(40)
        .collect::<Vec<_>>()
        .join(" ");
    let mut counts: Vec<(String, usize)> = Vec::new();
    for word in body.split(|c: char| !c.is_alphanumeric()) {
        if word.chars().count() < 5 {
            continue;
        }
        let word = word.to_lowercase();
        match counts.iter_mut().find(|(w, _)| *w == word) {
            Some((_, n)) => *n += 1,
            None => counts.push((word, 1)),
        }
    }
    // Most frequent first; ties keep first-seen order (the sort is stable).
    counts.sort_by_key(|c| std::cmp::Reverse(c.1));
    let tags: Vec<String> = counts.into_iter().take(3).map(|(w, _)| w).collect();
    serde_json::json!({ "summary": summary, "tags": tags }).to_string()
}

fn words(text: &str) -> u32 {
    u32::try_from(text.split_whitespace().count()).unwrap_or(u32::MAX)
}

impl ChatModel for FakeChatModel {
    fn provider(&self) -> &'static str {
        "fake"
    }

    fn model(&self) -> &str {
        MODEL
    }

    fn stream<'a>(&'a self, req: &'a ChatRequest) -> BoxFuture<'a, Result<ChatStream, LlmError>> {
        let answer = Self::answer(req);
        let input =
            words(&req.system) + req.messages.iter().map(|m| words(&m.content)).sum::<u32>();
        let pieces: Vec<String> = answer.split_inclusive(' ').map(str::to_owned).collect();
        let output = u32::try_from(pieces.len()).unwrap_or(u32::MAX);
        let (delay, fail_after) = (self.delay, self.fail_after);
        let max = usize::try_from(req.max_tokens).unwrap_or(usize::MAX);
        let truncated = pieces.len() > max;
        let deltas = pieces.into_iter().take(max).enumerate().map(move |(i, p)| {
            if fail_after.is_some_and(|n| i >= n) {
                Err(LlmError::Upstream("fake failure".into()))
            } else {
                Ok(ChatEvent::Delta(p))
            }
        });
        let done = Ok(ChatEvent::Done {
            stop: if truncated {
                StopReason::MaxTokens
            } else {
                StopReason::EndTurn
            },
            usage: Usage {
                input_tokens: Some(input),
                output_tokens: Some(output),
            },
        });
        let events = stream::iter(deltas.chain(std::iter::once(done))).then(move |e| async move {
            if !delay.is_zero() {
                tokio::time::sleep(delay).await;
            }
            e
        });
        // Stop after the first error, like the real providers.
        let mut failed = false;
        let events = events.take_while(move |e| {
            let keep = !failed;
            failed |= e.is_err();
            futures_util::future::ready(keep)
        });
        Box::pin(async move { Ok(Box::pin(events) as ChatStream) })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Message;

    fn req(text: &str) -> ChatRequest {
        ChatRequest {
            system: "be good".into(),
            messages: vec![Message::user(text)],
            max_tokens: 100,
            temperature: None,
            json: false,
        }
    }

    #[tokio::test]
    async fn cites_numbered_sources_or_echoes_the_last_line() {
        let model = FakeChatModel::new();
        let c = model
            .complete(&req(
                "Sources:\n[1] a.txt\nalpha\n[2] b.txt\n[1] again\nQuestion: why?",
            ))
            .await
            .expect("complete");
        assert_eq!(c.text, "Fact from source 1 [1]. Fact from source 2 [2].");
        assert_eq!(c.stop, StopReason::EndTurn);
        assert_eq!(c.usage.output_tokens, Some(10));
        let echo = model
            .complete(&req("history\nWhat is X?\n"))
            .await
            .expect("echo");
        assert_eq!(echo.text, "What is X?");
    }

    #[tokio::test]
    async fn json_mode_describes_the_text_after_the_header() {
        let mut request = req("notes.txt\nKittens sleep a lot. Kittens play with string.");
        request.json = true;
        let c = FakeChatModel::new().complete(&request).await.expect("json");
        let v: serde_json::Value = serde_json::from_str(&c.text).expect("valid json");
        assert_eq!(v["summary"], "Kittens sleep a lot.");
        assert_eq!(v["tags"], serde_json::json!(["kittens", "sleep", "string"]));
    }

    #[tokio::test]
    async fn failures_end_the_stream() {
        let model = FakeChatModel {
            fail_after: Some(1),
            ..FakeChatModel::default()
        };
        let events: Vec<_> = model
            .stream(&req("one two three"))
            .await
            .expect("stream")
            .collect()
            .await;
        assert_eq!(events.len(), 2);
        assert!(events[0].is_ok() && events[1].is_err());
    }
}
