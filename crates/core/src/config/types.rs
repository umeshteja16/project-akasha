//! Value types used by [`super::Config`].

use serde::{Deserialize, Deserializer, Serialize};

/// A list given either as a TOML array or as one string separated by commas
/// and/or whitespace (the natural form for an environment variable).
pub(super) fn string_list<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<String>, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum List {
        One(String),
        Many(Vec<String>),
    }
    let items = match List::deserialize(d)? {
        List::One(s) => vec![s],
        List::Many(v) => v,
    };
    Ok(items
        .iter()
        .flat_map(|s| s.split(|c: char| c == ',' || c.is_whitespace()))
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .collect())
}

/// Blob storage backend.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum StorageBackend {
    /// A directory on the local filesystem.
    Local,
    /// Amazon S3 or an S3-compatible service.
    S3,
}

/// A configuration value that must never appear in logs (`Debug` prints `[redacted]`).
#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(transparent)]
pub struct Secret(String);

impl Secret {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    /// The secret value. Only call this where the value is actually used.
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Debug for Secret {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("[redacted]")
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum LogFormat {
    /// Human-readable, for local development.
    Pretty,
    /// One JSON object per line, for production log shipping.
    Json,
}

/// Which language model writes chat answers.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum LlmProvider {
    /// No model: chat answers with the retrieved passages only.
    None,
    /// A local Ollama server (the offline default).
    Ollama,
    /// Anthropic Claude (cloud).
    Anthropic,
    /// Google Gemini (cloud).
    Gemini,
    /// Any OpenAI-compatible chat completions server (vLLM, LM Studio,
    /// llama.cpp, OpenRouter, OpenAI).
    Openai,
    /// Deterministic test model; never use it for real answers.
    Fake,
}
