//! Maps job kinds to typed handlers.

use std::{collections::HashMap, future::Future, pin::Pin, sync::Arc};

use serde_json::Value;

use crate::{Job, JobError};

pub(crate) type HandlerFuture = Pin<Box<dyn Future<Output = Result<(), JobError>> + Send>>;

/// Decodes the payload (failing fast on a bad one) and starts the handler.
type Erased<C> = Arc<dyn Fn(C, Value) -> Result<HandlerFuture, serde_json::Error> + Send + Sync>;

/// The handlers a worker runs, keyed by [`Job::KIND`]. `C` is the context every
/// handler receives (database pool, storage, ...).
pub struct Registry<C> {
    handlers: HashMap<&'static str, Erased<C>>,
}

impl<C> Default for Registry<C> {
    fn default() -> Self {
        Self {
            handlers: HashMap::new(),
        }
    }
}

impl<C: Clone + Send + Sync + 'static> Registry<C> {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register the handler for `J`. Handlers must be idempotent (see the crate docs).
    /// Registering a kind twice replaces the earlier handler.
    pub fn register<J, F, Fut>(mut self, handler: F) -> Self
    where
        J: Job,
        F: Fn(C, J) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<(), JobError>> + Send + 'static,
    {
        let handler = Arc::new(handler);
        let erased: Erased<C> = Arc::new(move |ctx, payload| {
            let job: J = serde_json::from_value(payload)?;
            let handler = Arc::clone(&handler);
            Ok(Box::pin(async move { handler(ctx, job).await }))
        });
        self.handlers.insert(J::KIND, erased);
        self
    }

    /// The registered kinds; the worker only claims these.
    pub fn kinds(&self) -> Vec<String> {
        let mut kinds: Vec<String> = self.handlers.keys().map(|k| (*k).to_owned()).collect();
        kinds.sort();
        kinds
    }

    /// `None` if `kind` has no handler; `Some(Err)` if the payload does not decode.
    pub(crate) fn start(
        &self,
        kind: &str,
        ctx: C,
        payload: Value,
    ) -> Option<Result<HandlerFuture, serde_json::Error>> {
        self.handlers.get(kind).map(|h| h(ctx, payload))
    }
}
