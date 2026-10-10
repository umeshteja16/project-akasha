//! Tool results and errors, kept small enough for a model's context.

use rmcp::model::{CallToolResult, ContentBlock};

use crate::error::ApiError;

/// Largest tool result, in characters of JSON (roughly 8k tokens).
pub const MAX_RESULT_CHARS: usize = 32_000;
/// Appended where text was cut.
pub const TRUNCATED: &str = " …[truncated]";

/// A failure the calling model should read (and may fix, e.g. bad arguments).
#[derive(Debug)]
pub struct ToolError(pub String);

impl ToolError {
    pub fn new(message: impl Into<String>) -> Self {
        Self(message.into())
    }
}

impl From<akasha_core::Error> for ToolError {
    fn from(err: akasha_core::Error) -> Self {
        Self(err.message)
    }
}

impl From<ApiError> for ToolError {
    fn from(err: ApiError) -> Self {
        Self(err.0.message)
    }
}

impl From<sqlx::Error> for ToolError {
    fn from(err: sqlx::Error) -> Self {
        tracing::error!(%err, "database error in an MCP tool");
        Self("internal error".into())
    }
}

impl From<akasha_search::SearchError> for ToolError {
    fn from(err: akasha_search::SearchError) -> Self {
        ApiError::from(err).into()
    }
}

impl From<serde_json::Error> for ToolError {
    fn from(err: serde_json::Error) -> Self {
        Self(format!("invalid arguments: {err}"))
    }
}

pub type ToolResult = Result<serde_json::Value, ToolError>;

/// Render a tool outcome: JSON text on success, an MCP tool error (`isError`)
/// otherwise. Never a protocol error, so the model sees the message.
pub fn render(result: ToolResult) -> CallToolResult {
    match result {
        Ok(value) => {
            let text = serde_json::to_string(&value).unwrap_or_else(|_| "{}".to_owned());
            CallToolResult::success(vec![ContentBlock::text(text)])
        }
        Err(ToolError(message)) => CallToolResult::error(vec![ContentBlock::text(message)]),
    }
}

/// `text` cut to at most `max` characters (on a character boundary), with a
/// marker when cut.
pub fn clip(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_owned();
    }
    let mut out: String = text.chars().take(max).collect();
    out.push_str(TRUNCATED);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clip_marks_cuts_and_respects_characters() {
        assert_eq!(clip("héllo", 10), "héllo");
        assert_eq!(clip("héllo", 2), format!("hé{TRUNCATED}"));
    }
}
