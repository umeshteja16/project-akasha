//! Language model metrics: a [`ChatModel`] wrapper that counts calls, tokens (as the
//! provider reports them) and time to the whole answer.

use std::{sync::Arc, time::Instant};

use akasha_llm::{ChatEvent, ChatModel, ChatRequest, ChatStream, LlmError};
use futures_util::{FutureExt, StreamExt, future::BoxFuture};
use metrics::{counter, histogram};

/// Wraps a model; behaves exactly like it.
pub struct Metered(pub Arc<dyn ChatModel>);

fn outcome(provider: &'static str, model: &str, outcome: &'static str) {
    counter!(
        "akasha_llm_requests_total",
        "provider" => provider,
        "model" => model.to_owned(),
        "outcome" => outcome
    )
    .increment(1);
}

impl ChatModel for Metered {
    fn provider(&self) -> &'static str {
        self.0.provider()
    }

    fn model(&self) -> &str {
        self.0.model()
    }

    fn stream<'a>(&'a self, req: &'a ChatRequest) -> BoxFuture<'a, Result<ChatStream, LlmError>> {
        let started = Instant::now();
        let provider = self.0.provider();
        let model = self.0.model().to_owned();
        self.0
            .stream(req)
            .map(move |result| {
                let stream = match result {
                    Ok(stream) => stream,
                    Err(err) => {
                        outcome(provider, &model, "error");
                        return Err(err);
                    }
                };
                let observed = stream.inspect(move |event| match event {
                    Ok(ChatEvent::Done { usage, .. }) => {
                        outcome(provider, &model, "ok");
                        histogram!(
                            "akasha_llm_duration_seconds",
                            "provider" => provider,
                            "model" => model.clone()
                        )
                        .record(started.elapsed().as_secs_f64());
                        let tokens = [
                            ("input", usage.input_tokens),
                            ("output", usage.output_tokens),
                        ];
                        for (direction, n) in tokens {
                            if let Some(n) = n {
                                counter!(
                                    "akasha_llm_tokens_total",
                                    "provider" => provider,
                                    "model" => model.clone(),
                                    "direction" => direction
                                )
                                .increment(u64::from(n));
                            }
                        }
                    }
                    Err(_) => outcome(provider, &model, "error"),
                    Ok(ChatEvent::Delta(_)) => {}
                });
                Ok(Box::pin(observed) as ChatStream)
            })
            .boxed()
    }
}
