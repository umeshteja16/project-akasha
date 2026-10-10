//! Tool definitions (names, descriptions written for models, JSON schemas) and
//! dispatch. Arguments are parsed strictly; bad arguments are tool errors the
//! model can read and fix.

use rmcp::{
    handler::server::common::schema_for_type,
    model::{Tool, ToolAnnotations},
};
use schemars::JsonSchema;
use serde::{Deserialize, de::DeserializeOwned};
use uuid::Uuid;

use super::{
    Caller, ask,
    output::{ToolError, ToolResult},
    read, write,
};
use crate::state::AppState;

/// Broad file kinds, as in the REST API.
#[derive(Debug, Clone, Copy, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Pdf,
    Image,
    Audio,
    Video,
    Text,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SearchArgs {
    /// What to look for: keywords or a natural-language question (1-500 characters).
    /// Web-search syntax works: "exact phrase", `or`, `-word` to exclude.
    pub query: String,
    /// `hybrid` (default: keywords + meaning), `keyword` or `semantic`.
    pub mode: Option<String>,
    /// Passages to return, 1-20 (default 8).
    pub limit: Option<usize>,
    /// Only files of this kind.
    pub kind: Option<Kind>,
    /// Only files carrying all of these tags.
    pub tags: Option<Vec<String>>,
    /// Only files uploaded on or after this date (YYYY-MM-DD) or RFC 3339 time.
    pub from: Option<String>,
    /// Only files uploaded on or before this date (inclusive) or RFC 3339 time.
    pub to: Option<String>,
    /// Search only within these files (up to 100 ids from earlier results).
    pub file_ids: Option<Vec<Uuid>>,
    /// Also return passages only loosely related by meaning (default false).
    pub include_weak: Option<bool>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FileArgs {
    /// The file's id (`file_id` from search or list_files results).
    pub file_id: Uuid,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ReadArgs {
    /// The file's id.
    pub file_id: Uuid,
    /// 1-based PDF page to read (reads that page, up to `max_chars`). Overrides `offset`.
    pub page: Option<u32>,
    /// Character offset to start at (default 0). Use `next_offset` from the previous
    /// call to continue, or `char_start` from a search result to jump to a passage.
    pub offset: Option<u32>,
    /// Characters to return, 100-20000 (default 8000).
    pub max_chars: Option<u32>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ListArgs {
    /// Only files of this kind.
    pub kind: Option<Kind>,
    /// Only files with this tag (own or suggested).
    pub tag: Option<String>,
    /// Only pinned (true) or unpinned (false) files.
    pub pinned: Option<bool>,
    /// `newest` (default), `oldest`, `name` or `size`.
    pub sort: Option<String>,
    /// Files per page, 1-50 (default 20).
    pub limit: Option<i64>,
    /// `next_cursor` from the previous call, to get the next page.
    pub cursor: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AskArgs {
    /// The question, in natural language (1-2000 characters).
    pub question: String,
    /// Answer only from these files (up to 100 ids).
    pub file_ids: Option<Vec<Uuid>>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct NoteArgs {
    /// Title of the note; becomes the file name (`<title>.md`).
    pub title: String,
    /// The note's text, Markdown or plain text (at most 200000 characters).
    pub content: String,
    /// Tags to put on the new note.
    pub tags: Option<Vec<String>>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TagArgs {
    /// The file's id.
    pub file_id: Uuid,
    /// Tags to add.
    pub add: Option<Vec<String>>,
    /// Tags to remove.
    pub remove: Option<Vec<String>>,
}

fn tool<T: JsonSchema + 'static>(
    name: &'static str,
    title: &str,
    description: &'static str,
) -> Tool {
    Tool::new(name, description, schema_for_type::<T>()).with_title(title)
}

fn read_only(title: &str) -> ToolAnnotations {
    ToolAnnotations::with_title(title)
        .read_only(true)
        .destructive(false)
        .open_world(false)
}

/// The tools this caller may use (write tools only with the `write` scope).
pub fn list(can_write: bool) -> Vec<Tool> {
    let mut tools = vec![
        tool::<SearchArgs>(
            "search",
            "Search the library",
            "Search the user's personal library (their uploaded PDFs, notes, documents and \
             OCR'd images) by keywords and meaning. Returns the best matching passages with \
             file name, file_id, page, character offsets and the passage text. Use it first \
             to find information; then read_file for more context around a passage.",
        )
        .annotate(read_only("Search the library")),
        tool::<FileArgs>(
            "get_file",
            "Get file details",
            "Metadata of one file in the user's library: name, type, size, processing \
             status, tags, model-written summary, page and character counts.",
        )
        .annotate(read_only("Get file details")),
        tool::<ReadArgs>(
            "read_file",
            "Read a file's text",
            "Read the extracted text of a file, a bounded window at a time (by page for \
             PDFs, or by character offset). Returns `next_offset` to continue reading.",
        )
        .annotate(read_only("Read a file's text")),
        tool::<ListArgs>(
            "list_files",
            "List files",
            "List files in the user's library (newest first by default) with optional \
             filters, a page at a time. Returns `next_cursor` for the next page.",
        )
        .annotate(read_only("List files")),
        tool::<AskArgs>(
            "ask",
            "Ask the library",
            "Answer a question from the user's own files, with numbered citations to the \
             passages used. Says so when the files do not contain the answer. When no \
             language model is configured on the server it returns the most relevant \
             passages instead of an answer.",
        )
        .annotate(read_only("Ask the library")),
    ];
    if can_write {
        tools.push(
            tool::<NoteArgs>(
                "add_note",
                "Add a note",
                "Save a new Markdown note to the user's library. It is indexed in the \
                 background and becomes searchable within moments. Identical content \
                 already in the library is not duplicated.",
            )
            .annotate(
                ToolAnnotations::with_title("Add a note")
                    .read_only(false)
                    .destructive(false)
                    .idempotent(true)
                    .open_world(false),
            ),
        );
        tools.push(
            tool::<TagArgs>(
                "tag_file",
                "Tag a file",
                "Add or remove tags on a file in the user's library. Returns the file's tags.",
            )
            .annotate(
                ToolAnnotations::with_title("Tag a file")
                    .read_only(false)
                    .destructive(false)
                    .idempotent(true)
                    .open_world(false),
            ),
        );
    }
    tools
}

fn parse<T: DeserializeOwned>(args: Option<rmcp::model::JsonObject>) -> Result<T, ToolError> {
    let value = serde_json::Value::Object(args.unwrap_or_default());
    Ok(serde_json::from_value(value)?)
}

/// Run a tool. `None`: no such tool.
pub async fn call(
    state: &AppState,
    caller: Caller,
    name: &str,
    args: Option<rmcp::model::JsonObject>,
) -> Option<ToolResult> {
    let needs_write = matches!(name, "add_note" | "tag_file");
    if needs_write && !caller.can_write {
        return Some(Err(ToolError::new(
            "this API token is read-only; create a token with the write scope to use this tool",
        )));
    }
    Some(match name {
        "search" => run(args, |a| read::search(state, caller, a)).await,
        "get_file" => run(args, |a| read::get_file(state, caller, a)).await,
        "read_file" => run(args, |a| read::read_file(state, caller, a)).await,
        "list_files" => run(args, |a| read::list_files(state, caller, a)).await,
        "ask" => run(args, |a| ask::ask(state, caller, a)).await,
        "add_note" => run(args, |a| write::add_note(state, caller, a)).await,
        "tag_file" => run(args, |a| write::tag_file(state, caller, a)).await,
        _ => return None,
    })
}

async fn run<T, F, Fut>(args: Option<rmcp::model::JsonObject>, f: F) -> ToolResult
where
    T: DeserializeOwned,
    F: FnOnce(T) -> Fut,
    Fut: std::future::Future<Output = ToolResult>,
{
    f(parse(args)?).await
}
