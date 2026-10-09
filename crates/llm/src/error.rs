/// Why a generation failed. Messages never contain API keys.
#[derive(Debug, thiserror::Error)]
pub enum LlmError {
    /// The provider is misconfigured (missing key or model, strict offline mode).
    #[error("{0}")]
    Config(String),
    /// The provider could not be reached (connection refused, DNS, timeout).
    #[error("could not reach the language model: {0}")]
    Unreachable(String),
    /// The provider answered with an HTTP error.
    #[error("the language model returned HTTP {status}: {message}")]
    Status { status: u16, message: String },
    /// The provider reported an error in the middle of the stream.
    #[error("the language model failed: {0}")]
    Upstream(String),
    /// The response did not have the expected shape.
    #[error("unexpected response from the language model: {0}")]
    Protocol(String),
}

impl LlmError {
    /// Worth retrying the same request (before any output was streamed).
    pub fn is_retryable(&self) -> bool {
        match self {
            Self::Unreachable(_) => true,
            Self::Status { status, .. } => *status == 429 || *status >= 500,
            Self::Config(_) | Self::Upstream(_) | Self::Protocol(_) => false,
        }
    }

    /// A short stable code for clients: `llm_unavailable`, `llm_rate_limited`,
    /// `llm_error` or `llm_misconfigured`.
    pub fn code(&self) -> &'static str {
        match self {
            Self::Config(_) => "llm_misconfigured",
            Self::Unreachable(_) => "llm_unavailable",
            Self::Status { status: 429, .. } => "llm_rate_limited",
            Self::Status { status, .. } if *status >= 500 => "llm_unavailable",
            Self::Status { .. } | Self::Upstream(_) | Self::Protocol(_) => "llm_error",
        }
    }
}

impl From<reqwest::Error> for LlmError {
    fn from(err: reqwest::Error) -> Self {
        // `without_url`: URLs never carry keys here, but keep logs short anyway.
        let err = err.without_url();
        if err.is_connect() || err.is_timeout() || err.is_request() {
            Self::Unreachable(err.to_string())
        } else {
            Self::Protocol(err.to_string())
        }
    }
}
