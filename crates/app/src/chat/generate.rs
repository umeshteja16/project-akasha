//! Streaming the model's answer to the client.

use akasha_llm::{ChatEvent, ChatModel, ChatRequest, StopReason};
use futures_util::StreamExt;

use super::{
    citations::{Source, cited},
    events::{AnswerStatus, ChatDelta, event},
    prompt,
    turn::{Outcome, Tx},
};

/// Stream the model's answer to the client, stopping when the client leaves.
pub(super) async fn generate(
    llm: &dyn ChatModel,
    request: &ChatRequest,
    sources: &[Source],
    tx: &Tx,
) -> Outcome {
    let started = tokio::select! {
        r = llm.stream(request) => r,
        () = tx.closed() => return Outcome::new(AnswerStatus::Cancelled, ""),
    };
    let mut stream = match started {
        Ok(s) => s,
        Err(err) => {
            tracing::warn!(%err, provider = llm.provider(), "chat generation failed");
            return Outcome::failed(err.code(), err.to_string(), String::new());
        }
    };
    let mut text = String::new();
    loop {
        let next = tokio::select! {
            n = stream.next() => n,
            () = tx.closed() => return Outcome::new(AnswerStatus::Cancelled, text),
        };
        match next {
            Some(Ok(ChatEvent::Delta(d))) => {
                text.push_str(&d);
                if tx
                    .send(event("delta", &ChatDelta { text: d }))
                    .await
                    .is_err()
                {
                    return Outcome::new(AnswerStatus::Cancelled, text);
                }
            }
            Some(Ok(ChatEvent::Done { stop, usage })) => {
                let mut o = finished(stop, text, sources);
                o.usage = usage;
                return o;
            }
            Some(Err(err)) => {
                tracing::warn!(%err, provider = llm.provider(), "chat generation failed");
                return Outcome::failed(err.code(), err.to_string(), text);
            }
            None => {
                return Outcome::failed("llm_error", "the answer stopped unexpectedly", text);
            }
        }
    }
}

fn finished(stop: StopReason, text: String, sources: &[Source]) -> Outcome {
    let declined = stop == StopReason::Refusal || is_not_found(&text);
    if declined {
        return Outcome::new(AnswerStatus::Refused, prompt::NOT_FOUND);
    }
    let citations = cited(&text, sources);
    let mut o = Outcome::new(AnswerStatus::Answered, text);
    o.citations = citations;
    o
}

/// The model gave the "not found" answer it was told to give.
pub(crate) fn is_not_found(text: &str) -> bool {
    let norm = |s: &str| {
        s.chars()
            .filter(|c| c.is_alphanumeric() || c.is_whitespace())
            .collect::<String>()
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .to_lowercase()
    };
    norm(text) == norm(prompt::NOT_FOUND)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn not_found_answers_are_recognised_loosely() {
        assert!(is_not_found("I couldn't find this in your files."));
        assert!(is_not_found("  i couldnt find this in your files "));
        assert!(!is_not_found(
            "I couldn't find this in your files, but [1] says X."
        ));
    }
}
