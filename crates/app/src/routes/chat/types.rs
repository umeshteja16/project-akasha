//! Request and response bodies for `/api/v1/conversations`.

use akasha_db::chat::{Conversation, Message};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use crate::{
    chat::{
        citations::Citation,
        events::{AnswerStatus, TokenUsage},
    },
    routes::files::types::FileCategory,
};
use akasha_core::Error;

/// Longest conversation title, in characters.
pub const MAX_TITLE_CHARS: usize = 200;
/// Characters of the first question used as an automatic title.
const AUTO_TITLE_CHARS: usize = 60;
/// Longest question, in characters.
pub const MAX_QUESTION_CHARS: usize = 4000;

#[derive(Debug, Serialize, ToSchema)]
pub struct ConversationResponse {
    pub id: Uuid,
    /// Empty until the first question (or a rename) names it.
    pub title: String,
    /// Files questions are answered from unless a question names its own;
    /// empty: all your files.
    pub file_ids: Vec<Uuid>,
    pub created_at: DateTime<Utc>,
    /// Last activity (new message, rename or new scope).
    pub updated_at: DateTime<Utc>,
}

impl From<Conversation> for ConversationResponse {
    fn from(c: Conversation) -> Self {
        Self {
            id: c.id,
            title: c.title,
            file_ids: c.file_ids,
            created_at: c.created_at,
            updated_at: c.updated_at,
        }
    }
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ConversationList {
    /// Most recently active first.
    pub items: Vec<ConversationResponse>,
    /// Pass as `cursor` to get the next page; `null` on the last page.
    pub next_cursor: Option<String>,
}

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct PageQuery {
    /// `next_cursor` from the previous page.
    pub cursor: Option<String>,
    /// Page size, 1–100 (default 30 conversations, 50 messages).
    pub limit: Option<i64>,
}

#[derive(Debug, Default, Deserialize, ToSchema)]
pub struct CreateConversation {
    /// Optional; otherwise the first question becomes the title.
    pub title: Option<String>,
    /// Answer only from these files (up to 100; ids that are not yours are dropped).
    pub file_ids: Option<Vec<Uuid>>,
}

/// Change the title, the file scope, or both.
#[derive(Debug, Deserialize, ToSchema)]
pub struct UpdateConversation {
    /// 1–200 characters.
    pub title: Option<String>,
    /// New file scope (up to 100 ids); `[]`: all your files.
    pub file_ids: Option<Vec<Uuid>>,
}

/// Longest file scope.
pub const MAX_FILE_IDS: usize = 100;

/// Rejects a scope over [`MAX_FILE_IDS`] files.
pub fn check_scope(file_ids: &[Uuid]) -> Result<(), Error> {
    if file_ids.len() > MAX_FILE_IDS {
        return Err(Error::bad_request(format!(
            "at most {MAX_FILE_IDS} file ids"
        )));
    }
    Ok(())
}

/// `user` or `assistant`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum MessageRole {
    User,
    Assistant,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct MessageResponse {
    pub id: Uuid,
    pub role: MessageRole,
    pub content: String,
    /// For questions always `answered`.
    pub status: AnswerStatus,
    /// Sources cited by an answer (empty for questions).
    pub citations: Vec<Citation>,
    /// `provider/model` that wrote an answer.
    pub model: Option<String>,
    pub usage: TokenUsage,
    pub latency_ms: Option<i32>,
    pub created_at: DateTime<Utc>,
}

impl From<Message> for MessageResponse {
    fn from(m: Message) -> Self {
        let status = match m.status.as_str() {
            "refused" => AnswerStatus::Refused,
            "no_llm" => AnswerStatus::NoLlm,
            "cancelled" => AnswerStatus::Cancelled,
            "error" => AnswerStatus::Error,
            _ => AnswerStatus::Answered,
        };
        let count = |n: Option<i32>| n.and_then(|n| u32::try_from(n).ok());
        Self {
            id: m.id,
            role: if m.role == "user" {
                MessageRole::User
            } else {
                MessageRole::Assistant
            },
            content: m.content,
            status,
            citations: serde_json::from_value(m.citations).unwrap_or_default(),
            model: m.model,
            usage: TokenUsage {
                input_tokens: count(m.input_tokens),
                output_tokens: count(m.output_tokens),
            },
            latency_ms: m.latency_ms,
            created_at: m.created_at,
        }
    }
}

#[derive(Debug, Serialize, ToSchema)]
pub struct MessageList {
    /// Oldest first within the page; pages go back in time.
    pub items: Vec<MessageResponse>,
    /// Pass as `cursor` to get older messages; `null` when there are none.
    pub next_cursor: Option<String>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct PostMessage {
    /// The question, 1–4000 characters.
    pub content: String,
    /// Only answer from these files (up to 100). Omitted: the conversation's
    /// own scope (`file_ids` on the conversation).
    pub file_ids: Option<Vec<Uuid>>,
    /// Only answer from files carrying all of these tags.
    pub tags: Option<Vec<String>>,
    /// Only answer from files of this kind.
    #[serde(rename = "type")]
    #[schema(rename = "type")]
    pub file_type: Option<FileCategory>,
}

/// A trimmed title of at most [`MAX_TITLE_CHARS`]; empty allowed when `allow_empty`.
pub fn clean_title(raw: &str, allow_empty: bool) -> Result<String, Error> {
    let title = raw.split_whitespace().collect::<Vec<_>>().join(" ");
    if title.is_empty() && !allow_empty {
        return Err(Error::bad_request("title must not be empty"));
    }
    if title.chars().count() > MAX_TITLE_CHARS {
        return Err(Error::bad_request(format!(
            "title must be at most {MAX_TITLE_CHARS} characters"
        )));
    }
    Ok(title)
}

/// A title from the first question: its start, cut at a word boundary.
pub fn auto_title(question: &str) -> String {
    let text = question.split_whitespace().collect::<Vec<_>>().join(" ");
    if text.chars().count() <= AUTO_TITLE_CHARS {
        return text;
    }
    let cut: String = text.chars().take(AUTO_TITLE_CHARS).collect();
    let cut = match cut.rfind(' ') {
        Some(i) if i > AUTO_TITLE_CHARS / 2 => &cut[..i],
        _ => cut.as_str(),
    };
    format!("{}…", cut.trim_end_matches([',', '.', ';', ':']))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn titles() {
        assert_eq!(auto_title("  What is\n the plan? "), "What is the plan?");
        let long = auto_title(
            "How do I configure the autovacuum scale factor for very large tables, and why?",
        );
        assert_eq!(
            long,
            "How do I configure the autovacuum scale factor for very…"
        );
        assert!(clean_title("   ", false).is_err());
        assert_eq!(clean_title("  ", true).ok().as_deref(), Some(""));
        assert!(clean_title(&"x".repeat(201), true).is_err());
    }
}
