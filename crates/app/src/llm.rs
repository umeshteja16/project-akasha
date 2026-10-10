//! The configured chat model (`akasha-llm`), built from [`Config`].

use std::{sync::Arc, time::Duration};

use akasha_core::{Config, LlmProvider, Secret};
use akasha_llm::{ApiKey, ChatModel, LlmError, LlmOptions, Provider};

/// Provider settings from the configuration. API keys fall back to the
/// providers' usual variables (`ANTHROPIC_API_KEY`, `GEMINI_API_KEY`,
/// `OPENAI_API_KEY`).
pub fn options(config: &Config) -> LlmOptions {
    let key = |configured: &Option<Secret>, fallback: &str| {
        configured
            .as_ref()
            .and_then(|s| ApiKey::new(s.expose()))
            .or_else(|| std::env::var(fallback).ok().and_then(ApiKey::new))
    };
    LlmOptions {
        provider: match config.llm_provider {
            LlmProvider::None => Provider::None,
            LlmProvider::Ollama => Provider::Ollama,
            LlmProvider::Anthropic => Provider::Anthropic,
            LlmProvider::Gemini => Provider::Gemini,
            LlmProvider::Openai => Provider::OpenAi,
            LlmProvider::Fake => Provider::Fake,
        },
        model: config.llm_model.clone(),
        ollama_url: config.ollama_url.clone(),
        ollama_num_ctx: config.ollama_num_ctx,
        anthropic_base_url: config.anthropic_base_url.clone(),
        anthropic_api_key: key(&config.anthropic_api_key, "ANTHROPIC_API_KEY"),
        anthropic_effort: config.anthropic_effort.clone(),
        gemini_base_url: config.gemini_base_url.clone(),
        gemini_api_key: key(&config.gemini_api_key, "GEMINI_API_KEY"),
        openai_base_url: config.openai_base_url.clone(),
        openai_api_key: key(&config.openai_api_key, "OPENAI_API_KEY"),
        connect_timeout: Duration::from_secs(config.llm_connect_timeout_secs.max(1)),
        read_timeout: Duration::from_secs(config.llm_read_timeout_secs.max(1)),
        max_retries: config.llm_max_retries,
        strict_offline: config.strict_offline,
        ..LlmOptions::default()
    }
}

/// The chat model, or `None` without one. Errors are settings that can never
/// work (strict offline mode with a cloud provider, a missing key): `serve`
/// refuses to start on them.
pub fn build(config: &Config) -> Result<Option<Arc<dyn ChatModel>>, LlmError> {
    let model = akasha_llm::build(&options(config))?
        .map(|m| Arc::new(crate::metrics::llm::Metered(m)) as Arc<dyn ChatModel>);
    match &model {
        Some(m) => tracing::info!(
            provider = m.provider(),
            model = m.model(),
            strict_offline = config.strict_offline,
            "chat model configured"
        ),
        None => tracing::info!("no chat model configured; chat returns passages only"),
    }
    Ok(model)
}
