//! Choosing and building the configured provider.

use std::{sync::Arc, time::Duration};

use crate::{
    ChatModel, LlmError, anthropic::Anthropic, fake::FakeChatModel, gemini::Gemini, http::Http,
    local::is_local_url, ollama::Ollama, openai::OpenAi,
};

/// Which provider answers chat questions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Provider {
    /// No language model: chat returns the retrieved passages only.
    None,
    Ollama,
    Anthropic,
    Gemini,
    OpenAi,
    /// The deterministic test model.
    Fake,
}

/// An API key; `Debug` never shows it.
#[derive(Clone)]
pub struct ApiKey(String);

impl ApiKey {
    /// `None` for an empty or blank key.
    pub fn new(key: impl Into<String>) -> Option<Self> {
        let key = key.into();
        (!key.trim().is_empty()).then(|| Self(key.trim().to_owned()))
    }

    pub(crate) fn expose(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Debug for ApiKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("[redacted]")
    }
}

/// Everything needed to build a provider.
#[derive(Debug, Clone)]
pub struct LlmOptions {
    pub provider: Provider,
    /// Model id; empty: the provider's default (OpenAI-compatible has none).
    pub model: String,
    pub ollama_url: String,
    /// Context window Ollama is asked for, in tokens.
    pub ollama_num_ctx: u32,
    pub anthropic_base_url: String,
    pub anthropic_api_key: Option<ApiKey>,
    /// Claude `output_config.effort` (`low`..`max`); empty: the model default.
    pub anthropic_effort: String,
    pub gemini_base_url: String,
    pub gemini_api_key: Option<ApiKey>,
    pub openai_base_url: String,
    pub openai_api_key: Option<ApiKey>,
    pub connect_timeout: Duration,
    /// Longest silence while waiting for the next piece of a response.
    pub read_timeout: Duration,
    /// Extra attempts after a connection error, 429 or 5xx.
    pub max_retries: u32,
    /// First retry delay (doubles each time) unless `Retry-After` says otherwise.
    pub retry_base: Duration,
    /// Only providers on this machine or the local network are allowed.
    pub strict_offline: bool,
}

impl Default for LlmOptions {
    fn default() -> Self {
        Self {
            provider: Provider::Ollama,
            model: String::new(),
            ollama_url: crate::ollama::DEFAULT_URL.into(),
            ollama_num_ctx: 8192,
            anthropic_base_url: crate::anthropic::DEFAULT_BASE_URL.into(),
            anthropic_api_key: None,
            anthropic_effort: "low".into(),
            gemini_base_url: crate::gemini::DEFAULT_BASE_URL.into(),
            gemini_api_key: None,
            openai_base_url: crate::openai::DEFAULT_BASE_URL.into(),
            openai_api_key: None,
            connect_timeout: Duration::from_secs(10),
            read_timeout: Duration::from_secs(120),
            max_retries: 2,
            retry_base: Duration::from_millis(500),
            strict_offline: false,
        }
    }
}

impl LlmOptions {
    fn model_or(&self, default: &str) -> String {
        match self.model.trim() {
            "" => default.to_owned(),
            m => m.to_owned(),
        }
    }

    /// Reject settings strict offline mode forbids.
    fn check_offline(&self) -> Result<(), LlmError> {
        if !self.strict_offline {
            return Ok(());
        }
        let refuse = |what: &str| {
            Err(LlmError::Config(format!(
                "strict offline mode is on (AKASHA_STRICT_OFFLINE): {what}; use Ollama (or an \
                 OpenAI-compatible server) on this machine or the local network, or set \
                 AKASHA_LLM_PROVIDER=none"
            )))
        };
        match self.provider {
            Provider::None | Provider::Fake => Ok(()),
            Provider::Anthropic => refuse("Anthropic is a cloud provider"),
            Provider::Gemini => refuse("Gemini is a cloud provider"),
            Provider::Ollama if !is_local_url(&self.ollama_url) => {
                refuse("AKASHA_OLLAMA_URL is not a local address")
            }
            Provider::OpenAi if !is_local_url(&self.openai_base_url) => {
                refuse("AKASHA_OPENAI_BASE_URL is not a local address")
            }
            Provider::Ollama | Provider::OpenAi => Ok(()),
        }
    }
}

/// Build the configured provider; `None` when chat runs without a model.
/// Fails on settings that can never work (missing key, strict offline
/// violations), so call it at startup.
pub fn build(opts: &LlmOptions) -> Result<Option<Arc<dyn ChatModel>>, LlmError> {
    opts.check_offline()?;
    let http = || {
        Http::new(
            opts.connect_timeout,
            opts.read_timeout,
            opts.max_retries,
            opts.retry_base,
        )
    };
    let missing = |var: &str| LlmError::Config(format!("{var} is required for this provider"));
    let model: Arc<dyn ChatModel> = match opts.provider {
        Provider::None => return Ok(None),
        Provider::Fake => Arc::new(FakeChatModel::new()),
        Provider::Ollama => Arc::new(Ollama::new(
            http()?,
            &opts.ollama_url,
            &opts.model_or(crate::ollama::DEFAULT_MODEL),
            opts.ollama_num_ctx,
        )),
        Provider::Anthropic => Arc::new(Anthropic::new(
            http()?,
            &opts.anthropic_base_url,
            opts.anthropic_api_key
                .clone()
                .ok_or_else(|| missing("AKASHA_ANTHROPIC_API_KEY"))?,
            &opts.model_or(crate::anthropic::DEFAULT_MODEL),
            &opts.anthropic_effort,
        )),
        Provider::Gemini => Arc::new(Gemini::new(
            http()?,
            &opts.gemini_base_url,
            opts.gemini_api_key
                .clone()
                .ok_or_else(|| missing("AKASHA_GEMINI_API_KEY"))?,
            &opts.model_or(crate::gemini::DEFAULT_MODEL),
        )?),
        Provider::OpenAi => {
            if opts.model.trim().is_empty() {
                return Err(missing("AKASHA_LLM_MODEL"));
            }
            Arc::new(OpenAi::new(
                http()?,
                &opts.openai_base_url,
                opts.openai_api_key.clone(),
                opts.model.trim(),
            ))
        }
    };
    Ok(Some(model))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn opts(provider: Provider) -> LlmOptions {
        LlmOptions {
            provider,
            anthropic_api_key: ApiKey::new("sk-ant-secret"),
            gemini_api_key: ApiKey::new("AIza-secret"),
            model: "m".into(),
            ..LlmOptions::default()
        }
    }

    #[test]
    fn builds_each_provider_with_defaults() {
        assert!(build(&opts(Provider::None)).expect("none").is_none());
        for (p, name) in [
            (Provider::Ollama, "ollama"),
            (Provider::Anthropic, "anthropic"),
            (Provider::Gemini, "gemini"),
            (Provider::OpenAi, "openai"),
            (Provider::Fake, "fake"),
        ] {
            let model = build(&opts(p)).expect("build").expect("some");
            assert_eq!(model.provider(), name);
        }
        let default_model = LlmOptions {
            provider: Provider::Anthropic,
            anthropic_api_key: ApiKey::new("k"),
            ..LlmOptions::default()
        };
        let m = build(&default_model).expect("build").expect("some");
        assert_eq!(m.model(), crate::anthropic::DEFAULT_MODEL);
    }

    #[test]
    fn missing_keys_and_models_are_config_errors() {
        let mut o = opts(Provider::Anthropic);
        o.anthropic_api_key = ApiKey::new("  ");
        assert!(matches!(build(&o), Err(LlmError::Config(m)) if m.contains("ANTHROPIC_API_KEY")));
        let mut o = opts(Provider::OpenAi);
        o.model.clear();
        assert!(matches!(build(&o), Err(LlmError::Config(m)) if m.contains("AKASHA_LLM_MODEL")));
        let mut o = opts(Provider::Gemini);
        o.model = "../evil?key=".into();
        assert!(matches!(build(&o), Err(LlmError::Config(_))));
    }

    #[test]
    fn strict_offline_allows_only_local_providers() {
        let strict = |p: Provider| LlmOptions {
            strict_offline: true,
            ..opts(p)
        };
        for p in [Provider::Anthropic, Provider::Gemini] {
            let err = build(&strict(p)).err().expect("refused");
            assert!(err.to_string().contains("strict offline"), "{err}");
        }
        assert!(build(&strict(Provider::Ollama)).is_ok());
        assert!(build(&strict(Provider::None)).is_ok());
        let mut remote_ollama = strict(Provider::Ollama);
        remote_ollama.ollama_url = "https://ollama.example.com".into();
        assert!(build(&remote_ollama).is_err());
        let mut lm_studio = strict(Provider::OpenAi);
        lm_studio.openai_base_url = "http://127.0.0.1:1234/v1".into();
        assert!(build(&lm_studio).is_ok());
        assert!(
            build(&strict(Provider::OpenAi)).is_err(),
            "api.openai.com is remote"
        );
    }

    #[test]
    fn keys_are_redacted() {
        let o = opts(Provider::Anthropic);
        let debug = format!("{o:?}");
        assert!(!debug.contains("sk-ant-secret") && !debug.contains("AIza-secret"));
        assert!(debug.contains("[redacted]"));
    }
}
