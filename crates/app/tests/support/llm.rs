//! A scripted chat model for job tests: records every request and answers
//! with a fixed text (or like the fake model when none is set).

use std::sync::{Arc, Mutex};

use akasha_llm::{
    ChatEvent, ChatModel, ChatRequest, ChatStream, LlmError, StopReason, Usage, fake::FakeChatModel,
};
use futures_util::{future::BoxFuture, stream};

#[derive(Default)]
pub struct Scripted {
    /// The answer; `None`: what [`FakeChatModel`] would say.
    pub reply: Option<String>,
    pub requests: Mutex<Vec<ChatRequest>>,
}

impl Scripted {
    pub fn new(reply: Option<&str>) -> Arc<Self> {
        Arc::new(Self {
            reply: reply.map(str::to_owned),
            requests: Mutex::new(Vec::new()),
        })
    }

    pub fn calls(&self) -> usize {
        self.requests.lock().expect("lock").len()
    }

    pub fn last(&self) -> ChatRequest {
        self.requests
            .lock()
            .expect("lock")
            .last()
            .cloned()
            .expect("a request")
    }
}

impl ChatModel for Scripted {
    fn provider(&self) -> &'static str {
        "fake"
    }

    fn model(&self) -> &str {
        "scripted"
    }

    fn stream<'a>(&'a self, req: &'a ChatRequest) -> BoxFuture<'a, Result<ChatStream, LlmError>> {
        self.requests.lock().expect("lock").push(req.clone());
        let text = self
            .reply
            .clone()
            .unwrap_or_else(|| FakeChatModel::answer(req));
        let events = vec![
            Ok(ChatEvent::Delta(text)),
            Ok(ChatEvent::Done {
                stop: StopReason::EndTurn,
                usage: Usage::default(),
            }),
        ];
        Box::pin(async move { Ok(Box::pin(stream::iter(events)) as ChatStream) })
    }
}
