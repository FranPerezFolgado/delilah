//! Claude Code transcripts (`~/.claude/projects/<folder>/<session>.jsonl`) read as the
//! entries of a conversation: what the user said, what Claude answered and which tools
//! it ran. One mapping over the `message` objects (`parse_message`) serves both the
//! file and the live stream. The parser is tolerant: what it does not know is skipped
//! and counted, never an error.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use chrono::{DateTime, SecondsFormat, Utc};
use serde::Serialize;
use serde_json::{Map, Value};
use similar::{ChangeTag, TextDiff};

mod background;
pub use background::{BackgroundInfo, BackgroundKind, BackgroundSource};

/// Tool results and long input strings are cut to this many bytes in entries.
pub const MAX_TEXT: usize = 4096;
/// Longest one-line summary, in characters.
const MAX_SUMMARY: usize = 120;
/// A diff is not computed when old and new text together exceed this many bytes.
const MAX_DIFF_INPUT: usize = 256 * 1024;
/// Most rows a diff keeps.
const MAX_DIFF_ROWS: usize = 2000;
/// Longest time spent computing one diff.
const DIFF_TIMEOUT: std::time::Duration = std::time::Duration::from_millis(500);
/// Longest tool_use id or session id accepted from a request.
pub const MAX_ID_LEN: usize = 128;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum UserState {
    Queued,
    Sent,
    Delivered,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum UserFrom {
    View,
    Terminal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum MarkerKind {
    Compacted,
    Stopped,
    /// A turn Claude started by itself because a background task finished.
    #[serde(rename = "background-done")]
    BackgroundDone,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum DiffKind {
    Add,
    Del,
    Ctx,
    Hunk,
}

/// One row of a diff, the shape the Changes view renders.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DiffRow {
    pub kind: DiffKind,
    pub old: Option<u32>,
    pub new: Option<u32>,
    pub text: String,
}

/// One item of a conversation, as the API returns it. Cards are stored by the daemon
/// and never come from a transcript.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Entry {
    User {
        id: String,
        at: String,
        text: String,
        state: UserState,
        from: UserFrom,
    },
    Claude {
        id: String,
        at: String,
        text: String,
        /// The assistant message this came from (`message.id`), when it has one.
        #[serde(skip_serializing_if = "Option::is_none")]
        message_id: Option<String>,
    },
    Tool {
        id: String,
        at: String,
        tool: String,
        tool_use_id: String,
        summary: String,
        input: Value,
        /// `None` while the tool has not answered yet.
        result: Option<String>,
        is_error: bool,
        truncated: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        diff: Option<Vec<DiffRow>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        path: Option<String>,
        /// The assistant message this call came from (`message.id`), when it has one.
        #[serde(skip_serializing_if = "Option::is_none")]
        message_id: Option<String>,
    },
    Marker {
        id: String,
        at: String,
        marker: MarkerKind,
        /// Why, when known (the first line of a task notification's summary).
        #[serde(skip_serializing_if = "Option::is_none")]
        detail: Option<String>,
        /// What a background-task notification said, when this marker is one.
        #[serde(skip_serializing_if = "Option::is_none")]
        background: Option<BackgroundInfo>,
    },
}

/// What `parse` read.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Parsed {
    pub entries: Vec<Entry>,
    /// Byte offset of the first unread complete line: the cursor for the next call.
    pub next_offset: u64,
    /// Lines or blocks that were not understood (unknown types, bad JSON).
    pub skipped: usize,
    /// Results whose `tool_use` is not in the bytes read (it came before the offset):
    /// `(tool_use_id, result cut like any entry's, is_error)`. A caller reading with a
    /// cursor re-emits the earlier tool entry with its result (same id), so the client
    /// replaces it by id instead of leaving the tool pending forever.
    pub orphan_results: Vec<(String, Option<String>, bool)>,
}

/// The untruncated input and result of one tool call.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ToolDetail {
    pub input: Value,
    pub result: Option<String>,
    pub is_error: bool,
}

/// Whether `s` is safe as a file name part: non-empty, at most `MAX_ID_LEN` characters
/// of `[A-Za-z0-9_-]`.
pub fn is_safe_id(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= MAX_ID_LEN
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

/// Where Claude saves the transcript of `session_id` started in `workspace`
/// (`None` without `$HOME`, or when `session_id` is not a plain id).
pub fn transcript_path(workspace: &Path, session_id: &str) -> Option<PathBuf> {
    let home = std::env::var_os("HOME")?;
    transcript_path_in(Path::new(&home), workspace, session_id)
}

/// [`transcript_path`] under an explicit home directory. The folder is named after the
/// workspace path.
pub fn transcript_path_in(home: &Path, workspace: &Path, session_id: &str) -> Option<PathBuf> {
    transcript_path_under(&projects_root_in(home), workspace, session_id)
}

/// [`transcript_path`] under an explicit projects folder (what the daemon configures).
pub fn transcript_path_under(
    projects_root: &Path,
    workspace: &Path,
    session_id: &str,
) -> Option<PathBuf> {
    if !is_safe_id(session_id) {
        return None;
    }
    let dir = workspace
        .canonicalize()
        .unwrap_or_else(|_| workspace.to_path_buf());
    // Claude names a project's folder after its path, every other character a dash.
    let folder: String = dir
        .to_string_lossy()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    Some(
        projects_root
            .join(folder)
            .join(format!("{session_id}.jsonl")),
    )
}

/// Where Claude keeps every project's transcripts under `home`.
pub fn projects_root_in(home: &Path) -> PathBuf {
    home.join(".claude/projects")
}

/// `path` resolved (symlinks followed) when it is inside `root` once resolved too;
/// `None` when it is elsewhere or does not exist. Callers use the returned path.
pub fn within(root: &Path, path: &Path) -> Option<PathBuf> {
    let root = root.canonicalize().ok()?;
    let path = path.canonicalize().ok()?;
    path.starts_with(&root).then_some(path)
}

/// Whether `line` mentions `id` anywhere: a cheap test before parsing it as JSON.
pub fn line_mentions(line: &[u8], id: &str) -> bool {
    let id = id.as_bytes();
    !id.is_empty() && line.windows(id.len()).any(|w| w == id)
}

/// Something one line yields: an entry, or the result of an earlier tool call.
#[allow(clippy::large_enum_variant)]
enum Item {
    Entry(Entry),
    Result {
        tool_use_id: String,
        text: String,
        is_error: bool,
    },
}

/// Reads the complete lines of `bytes` from `from_offset` on. A last line without its
/// `\n` is left for the next call (`next_offset` points at its start). A tool result
/// fills the entry of its `tool_use` when both are in `bytes`; one whose `tool_use` was
/// before `from_offset` is listed in `orphan_results`.
pub fn parse(bytes: &[u8], from_offset: u64) -> Parsed {
    parse_page(bytes, from_offset, usize::MAX)
}

/// Like [`parse`], but stops after the line that brings the entries to `max_entries`
/// or more; `next_offset` is then the start of the first line not read.
pub fn parse_page(bytes: &[u8], from_offset: u64, max_entries: usize) -> Parsed {
    let mut pos = usize::try_from(from_offset)
        .unwrap_or(usize::MAX)
        .min(bytes.len());
    let mut entries: Vec<Entry> = Vec::new();
    let mut by_tool: HashMap<String, usize> = HashMap::new();
    let mut skipped = 0;
    let mut orphan_results = Vec::new();
    while entries.len() < max_entries
        && let Some(nl) = bytes[pos..].iter().position(|b| *b == b'\n')
    {
        let line = &bytes[pos..pos + nl];
        pos += nl + 1;
        if line.iter().all(u8::is_ascii_whitespace) {
            continue;
        }
        let Ok(value) = serde_json::from_slice::<Value>(line) else {
            skipped += 1;
            continue;
        };
        for item in line_items(&value, &mut skipped) {
            match item {
                Item::Entry(entry) => {
                    if let Entry::Tool { tool_use_id, .. } = &entry
                        && !tool_use_id.is_empty()
                    {
                        by_tool.insert(tool_use_id.clone(), entries.len());
                    }
                    entries.push(entry);
                }
                Item::Result {
                    tool_use_id,
                    text,
                    is_error,
                } => {
                    if let Some(i) = by_tool.get(&tool_use_id) {
                        fill_result(&mut entries[*i], &text, is_error);
                    } else {
                        let (cut, _) = truncate(&text, MAX_TEXT);
                        orphan_results.push((tool_use_id, Some(cut), is_error));
                    }
                }
            }
        }
    }
    Parsed {
        entries,
        next_offset: pos as u64,
        skipped,
        orphan_results,
    }
}

/// The shared mapping over one `message` object (`{role, content}`), for the file and
/// the live stream. Ids use the message id (`t:<message id>:<block index>`) and `at`
/// is now; results pair only with tool calls of the same message.
pub fn parse_message(message: &Value) -> Vec<Entry> {
    let uuid = message.get("id").and_then(Value::as_str).unwrap_or("msg");
    let at = Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true);
    let mut skipped = 0;
    let mut entries: Vec<Entry> = Vec::new();
    for item in message_items(message, uuid, &at, &mut skipped) {
        match item {
            Item::Entry(entry) => entries.push(entry),
            Item::Result {
                tool_use_id,
                text,
                is_error,
            } => {
                let target = entries
                    .iter_mut()
                    .find(|e| matches!(e, Entry::Tool { tool_use_id: t, .. } if *t == tool_use_id));
                if let Some(entry) = target {
                    fill_result(entry, &text, is_error);
                }
            }
        }
    }
    entries
}

/// The full input and result of the tool call `tool_use_id`, as saved in `bytes`.
pub fn tool_detail(bytes: &[u8], tool_use_id: &str) -> Option<ToolDetail> {
    let mut detail: Option<ToolDetail> = None;
    for line in bytes.split(|b| *b == b'\n') {
        if !line_mentions(line, tool_use_id) {
            continue;
        }
        let Ok(value) = serde_json::from_slice::<Value>(line) else {
            continue;
        };
        let Some(blocks) = value.pointer("/message/content").and_then(Value::as_array) else {
            continue;
        };
        for block in blocks {
            match block.get("type").and_then(Value::as_str) {
                Some("tool_use")
                    if block.get("id").and_then(Value::as_str) == Some(tool_use_id) =>
                {
                    detail = Some(ToolDetail {
                        input: block.get("input").cloned().unwrap_or(Value::Null),
                        result: None,
                        is_error: false,
                    });
                }
                Some("tool_result")
                    if block.get("tool_use_id").and_then(Value::as_str) == Some(tool_use_id) =>
                {
                    if let Some(d) = detail.as_mut() {
                        d.result = Some(result_text(block.get("content")));
                        d.is_error = block
                            .get("is_error")
                            .and_then(Value::as_bool)
                            .unwrap_or(false);
                    }
                }
                _ => {}
            }
        }
    }
    detail
}

fn line_items(line: &Value, skipped: &mut usize) -> Vec<Item> {
    let kind = line.get("type").and_then(Value::as_str).unwrap_or("");
    match kind {
        "user" | "assistant" => {}
        // Known lines that are not part of the conversation.
        "summary" | "system" | "file-history-snapshot" | "queue-operation" => return vec![],
        _ => {
            *skipped += 1;
            return vec![];
        }
    }
    let flag = |name: &str| line.get(name).and_then(Value::as_bool).unwrap_or(false);
    // A hand-back is a hidden (`isMeta`) line; it is the one meta line that is shown.
    let handback = if !flag("isSidechain") && flag("isMeta") {
        background::read_handback(&line["origin"])
    } else {
        None
    };
    if flag("isSidechain") || (flag("isMeta") && handback.is_none()) {
        return vec![];
    }
    let Some(uuid) = line.get("uuid").and_then(Value::as_str) else {
        *skipped += 1;
        return vec![];
    };
    let Some(at) = line
        .get("timestamp")
        .and_then(Value::as_str)
        .and_then(|t| DateTime::parse_from_rfc3339(t).ok())
        .map(|t| {
            t.with_timezone(&Utc)
                .to_rfc3339_opts(SecondsFormat::Millis, true)
        })
    else {
        *skipped += 1;
        return vec![];
    };
    if let Some(info) = handback {
        return vec![Item::Entry(Entry::Marker {
            id: format!("t:{uuid}:0"),
            at,
            marker: MarkerKind::BackgroundDone,
            detail: info.label.clone(),
            background: Some(info),
        })];
    }
    if flag("isCompactSummary") {
        return vec![Item::Entry(Entry::Marker {
            id: format!("t:{uuid}:0"),
            at,
            marker: MarkerKind::Compacted,
            detail: None,
            background: None,
        })];
    }
    if line.get("type").and_then(Value::as_str) == Some("user")
        && let Some(message) = line.get("message")
    {
        let from_origin =
            line.pointer("/origin/kind").and_then(Value::as_str) == Some("task-notification");
        let text = user_text(message);
        if from_origin || text.trim_start().starts_with("<task-notification>") {
            return vec![Item::Entry(Entry::Marker {
                id: format!("t:{uuid}:0"),
                at,
                marker: MarkerKind::BackgroundDone,
                detail: notification_summary(&text),
                background: Some(background::read_notification(notification_block(&text))),
            })];
        }
    }
    match line.get("message") {
        Some(message) => message_items(message, uuid, &at, skipped),
        None => {
            *skipped += 1;
            vec![]
        }
    }
}

/// The text of a user message, its text blocks joined.
fn user_text(message: &Value) -> String {
    match message.get("content") {
        Some(Value::String(t)) => t.clone(),
        Some(Value::Array(blocks)) => blocks
            .iter()
            .filter(|b| b.get("type").and_then(Value::as_str) == Some("text"))
            .filter_map(|b| b.get("text").and_then(Value::as_str))
            .collect::<Vec<_>>()
            .join("\n"),
        _ => String::new(),
    }
}

/// The `<task-notification>` block of a text: from its opening tag to the closing tag
/// or the end. Without an opening tag, the whole text.
fn notification_block(text: &str) -> &str {
    let from = text.find("<task-notification>").unwrap_or(0);
    let block = &text[from..];
    match block.find("</task-notification>") {
        Some(end) => &block[..end],
        None => block,
    }
}

/// First line of the `<summary>` of a task notification, at most 200 characters.
fn notification_summary(text: &str) -> Option<String> {
    let start = text.find("<summary>")? + "<summary>".len();
    let rest = &text[start..];
    let body = &rest[..rest.find("</summary>").unwrap_or(rest.len())];
    let first = body.trim().lines().next()?.trim();
    (!first.is_empty()).then(|| first.chars().take(200).collect())
}

/// What a tool call shows as a step: "Leyendo <file>", "Buscando <pattern>", the first
/// 40 characters of a Bash command, else the tool name.
pub fn tool_step_summary(tool: &str, input: &Value) -> Option<String> {
    let field = |name: &str| input[name].as_str().filter(|s| !s.is_empty());
    Some(match tool {
        "Read" => format!("Leyendo {}", field("file_path")?),
        "Grep" | "Glob" => format!("Buscando {}", field("pattern")?),
        "Bash" => field("command")?.chars().take(40).collect(),
        other => other.to_string(),
    })
}

/// Text that Claude Code or its hooks put in user messages without the user typing it.
fn is_injected(text: &str) -> bool {
    let t = text.trim_start();
    t.is_empty()
        || t.starts_with("<system-reminder>")
        || t.starts_with("<user-prompt-submit-hook>")
        || t.starts_with("<command-")
        || t.starts_with("<local-command-")
        || t.starts_with("<hook")
        || t.starts_with("Caveat: The messages below were generated")
}

fn message_items(message: &Value, uuid: &str, at: &str, skipped: &mut usize) -> Vec<Item> {
    let user = message.get("role").and_then(Value::as_str) == Some("user");
    let blocks: Vec<Value> = match message.get("content") {
        Some(Value::String(text)) => vec![serde_json::json!({"type": "text", "text": text})],
        Some(Value::Array(blocks)) => blocks.clone(),
        _ => {
            *skipped += 1;
            return vec![];
        }
    };
    let mut items = Vec::new();
    for (n, block) in blocks.iter().enumerate() {
        let id = format!("t:{uuid}:{n}");
        let at = at.to_string();
        match block.get("type").and_then(Value::as_str) {
            Some("text") => {
                let text = block.get("text").and_then(Value::as_str).unwrap_or("");
                if user {
                    if !is_injected(text) {
                        items.push(Item::Entry(Entry::User {
                            id,
                            at,
                            text: text.to_string(),
                            state: UserState::Delivered,
                            from: UserFrom::Terminal,
                        }));
                    }
                } else if !text.trim().is_empty() {
                    items.push(Item::Entry(Entry::Claude {
                        id,
                        at,
                        text: text.to_string(),
                        message_id: None,
                    }));
                }
            }
            Some("thinking" | "redacted_thinking") => {}
            Some("tool_use") => {
                let name = block.get("name").and_then(Value::as_str).unwrap_or("");
                let tool_use_id = block.get("id").and_then(Value::as_str).unwrap_or("");
                let input = block.get("input").cloned().unwrap_or(Value::Null);
                items.push(Item::Entry(new_tool(id, at, name, tool_use_id, &input)));
            }
            Some("tool_result") => {
                if let Some(tool_use_id) = block.get("tool_use_id").and_then(Value::as_str) {
                    items.push(Item::Result {
                        tool_use_id: tool_use_id.to_string(),
                        text: result_text(block.get("content")),
                        is_error: block
                            .get("is_error")
                            .and_then(Value::as_bool)
                            .unwrap_or(false),
                    });
                } else {
                    *skipped += 1;
                }
            }
            Some("image") => items.push(Item::Entry(Entry::Tool {
                id,
                at,
                tool: "Image".into(),
                tool_use_id: String::new(),
                summary: "Image".into(),
                input: Value::Object(Map::new()),
                result: Some("[image]".into()),
                is_error: false,
                truncated: false,
                diff: None,
                path: None,
                message_id: None,
            })),
            _ => *skipped += 1,
        }
    }
    // Which assistant message the entries came from, for a client to tell when the
    // streamed text of that message has been replaced by its final entries.
    if let Some(mid) = message.get("id").and_then(Value::as_str).filter(|_| !user) {
        for item in &mut items {
            if let Item::Entry(Entry::Claude { message_id, .. } | Entry::Tool { message_id, .. }) =
                item
            {
                *message_id = Some(mid.to_string());
            }
        }
    }
    items
}

/// A `tool_result` content (a string or blocks of text and images) as text.
fn result_text(content: Option<&Value>) -> String {
    match content {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Array(blocks)) => blocks
            .iter()
            .map(|b| match b.get("type").and_then(Value::as_str) {
                Some("text") => b
                    .get("text")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
                Some("image") => "[image]".to_string(),
                _ => String::new(),
            })
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join("\n"),
        _ => String::new(),
    }
}

fn new_tool(id: String, at: String, name: &str, tool_use_id: &str, input: &Value) -> Entry {
    let path = ["file_path", "notebook_path", "path"]
        .iter()
        .find_map(|k| input.get(*k).and_then(Value::as_str))
        .filter(|_| {
            matches!(
                name,
                "Edit" | "MultiEdit" | "Write" | "Read" | "NotebookEdit"
            )
        })
        .map(str::to_string);
    let (diff, diff_cut) = match tool_diff(name, input) {
        Some((rows, cut)) => (Some(rows), cut),
        None => (None, false),
    };
    let mut stored = input.clone();
    let truncated = truncate_strings(&mut stored) | diff_cut;
    Entry::Tool {
        id,
        at,
        tool: name.to_string(),
        tool_use_id: tool_use_id.to_string(),
        summary: summary(name, &stored, None, diff.as_deref()),
        input: stored,
        result: None,
        is_error: false,
        truncated,
        diff,
        path,
        message_id: None,
    }
}

fn fill_result(entry: &mut Entry, text: &str, error: bool) {
    if let Entry::Tool {
        tool,
        input,
        summary: s,
        result,
        is_error,
        truncated,
        diff,
        ..
    } = entry
    {
        let (cut, was_cut) = truncate(text, MAX_TEXT);
        *s = summary(tool, input, Some(text), diff.as_deref());
        *result = Some(cut);
        *is_error = error;
        *truncated |= was_cut;
    }
}

/// `s` cut to at most `max` bytes at a character boundary.
fn truncate(s: &str, max: usize) -> (String, bool) {
    if s.len() <= max {
        return (s.to_string(), false);
    }
    let mut end = max;
    while !s.is_char_boundary(end) {
        end -= 1;
    }
    (s[..end].to_string(), true)
}

/// Cuts every long string of `value`; whether any was cut.
fn truncate_strings(value: &mut Value) -> bool {
    match value {
        Value::String(s) => {
            let (cut, was_cut) = truncate(s, MAX_TEXT);
            if was_cut {
                *s = cut;
            }
            was_cut
        }
        Value::Array(items) => items.iter_mut().fold(false, |a, v| truncate_strings(v) | a),
        Value::Object(map) => map.values_mut().fold(false, |a, v| truncate_strings(v) | a),
        _ => false,
    }
}

fn first_line(s: &str) -> &str {
    s.lines()
        .find(|l| !l.trim().is_empty())
        .unwrap_or("")
        .trim()
}

fn one_line(s: &str) -> String {
    let line = first_line(s);
    if line.chars().count() > MAX_SUMMARY {
        let cut: String = line.chars().take(MAX_SUMMARY).collect();
        format!("{cut}…")
    } else {
        line.to_string()
    }
}

fn str_of<'a>(input: &'a Value, key: &str) -> &'a str {
    input.get(key).and_then(Value::as_str).unwrap_or("")
}

/// The one-line summary of a tool call; `result` refines it once known.
fn summary(name: &str, input: &Value, result: Option<&str>, diff: Option<&[DiffRow]>) -> String {
    let text = match name {
        "Bash" => {
            let description = str_of(input, "description");
            if description.is_empty() {
                str_of(input, "command")
            } else {
                description
            }
            .to_string()
        }
        "Edit" | "MultiEdit" | "Write" => {
            let path = str_of(input, "file_path");
            match diff {
                Some(rows) => {
                    let count = |k| rows.iter().filter(|r| r.kind == k).count();
                    format!(
                        "{path} +{} \u{2212}{}",
                        count(DiffKind::Add),
                        count(DiffKind::Del)
                    )
                }
                None => path.to_string(),
            }
        }
        "NotebookEdit" => str_of(input, "file_path").to_string(),
        "Read" => {
            let path = str_of(input, "file_path");
            match (
                input.get("offset").and_then(Value::as_u64),
                input.get("limit").and_then(Value::as_u64),
            ) {
                (Some(o), Some(l)) => format!("{path}:{o}-{}", o.saturating_add(l)),
                (Some(o), None) => format!("{path}:{o}-"),
                (None, Some(l)) => format!("{path}:1-{l}"),
                _ => path.to_string(),
            }
        }
        "Grep" | "Glob" => {
            let pattern = str_of(input, "pattern");
            let place = str_of(input, "path");
            let head = if place.is_empty() {
                pattern.to_string()
            } else {
                format!("{pattern} in {place}")
            };
            match result {
                Some(r) => {
                    let n = r.lines().filter(|l| !l.trim().is_empty()).count();
                    format!("{head} ({n})")
                }
                None => head,
            }
        }
        "TodoWrite" => {
            let todos = input
                .get("todos")
                .and_then(Value::as_array)
                .map(Vec::as_slice)
                .unwrap_or_default();
            let done = todos
                .iter()
                .filter(|t| str_of(t, "status") == "completed")
                .count();
            let items: Vec<&str> = todos.iter().map(|t| str_of(t, "content")).collect();
            format!("{}/{} done: {}", done, todos.len(), items.join("; "))
        }
        "Task" | "Agent" => {
            let description = str_of(input, "description");
            if description.is_empty() {
                str_of(input, "prompt")
            } else {
                description
            }
            .to_string()
        }
        "AskUserQuestion" => input
            .pointer("/questions/0/question")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        _ => {
            let compact = if input.is_null() {
                String::new()
            } else {
                input.to_string()
            };
            format!("{name} {compact}")
        }
    };
    one_line(&text)
}

/// The diff of an edit tool and whether it was cut (too large, too many rows, or too
/// slow to compute).
fn tool_diff(name: &str, input: &Value) -> Option<(Vec<DiffRow>, bool)> {
    let pairs: Vec<(&str, &str)> = match name {
        "Edit" => vec![(str_of(input, "old_string"), str_of(input, "new_string"))],
        "MultiEdit" => input
            .get("edits")?
            .as_array()?
            .iter()
            .map(|e| (str_of(e, "old_string"), str_of(e, "new_string")))
            .collect(),
        "Write" => vec![("", str_of(input, "content"))],
        _ => return None,
    };
    let mut rows = Vec::new();
    let mut cut = false;
    for (old, new) in pairs {
        if rows.len() >= MAX_DIFF_ROWS {
            cut = true;
            break;
        }
        cut |= diff_rows(old, new, &mut rows);
    }
    Some((rows, cut))
}

/// Appends the line diff of `old` to `new` (three lines of context; line numbers count
/// from the start of each text) to `rows`; whether it was cut.
fn diff_rows(old: &str, new: &str, rows: &mut Vec<DiffRow>) -> bool {
    if old.len() + new.len() > MAX_DIFF_INPUT {
        return true;
    }
    let diff = TextDiff::configure()
        .timeout(DIFF_TIMEOUT)
        .diff_lines(old, new);
    let num = |n: usize| u32::try_from(n + 1).ok();
    for group in diff.grouped_ops(3) {
        let (Some(first), Some(last)) = (group.first(), group.last()) else {
            continue;
        };
        let (o, n) = (
            first.old_range().start..last.old_range().end,
            first.new_range().start..last.new_range().end,
        );
        rows.push(DiffRow {
            kind: DiffKind::Hunk,
            old: None,
            new: None,
            text: format!(
                "@@ -{},{} +{},{} @@",
                o.start + 1,
                o.len(),
                n.start + 1,
                n.len()
            ),
        });
        for op in &group {
            for change in diff.iter_changes(op) {
                if rows.len() >= MAX_DIFF_ROWS {
                    return true;
                }
                let text = change.value().trim_end_matches(['\n', '\r']);
                let text = truncate(text, MAX_TEXT).0;
                let (kind, old, new) = match change.tag() {
                    ChangeTag::Equal => (
                        DiffKind::Ctx,
                        change.old_index().and_then(num),
                        change.new_index().and_then(num),
                    ),
                    ChangeTag::Delete => (DiffKind::Del, change.old_index().and_then(num), None),
                    ChangeTag::Insert => (DiffKind::Add, None, change.new_index().and_then(num)),
                };
                rows.push(DiffRow {
                    kind,
                    old,
                    new,
                    text,
                });
            }
        }
    }
    false
}

/// Tokens a session used, as the agent reports them. `total` is new input, output and
/// cache writes; re-read cached context (`cache_read`) is not counted in it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct TokenUsage {
    pub total: u64,
    pub input: u64,
    pub output: u64,
    pub cache_write: u64,
    pub cache_read: u64,
}

impl TokenUsage {
    pub fn add(&mut self, other: &TokenUsage) {
        self.total = self.total.saturating_add(other.total);
        self.input = self.input.saturating_add(other.input);
        self.output = self.output.saturating_add(other.output);
        self.cache_write = self.cache_write.saturating_add(other.cache_write);
        self.cache_read = self.cache_read.saturating_add(other.cache_read);
    }
}

/// What `usage_delta` read: the usage of the new lines, how many bytes it consumed and
/// the last message id counted (to pass to the next call).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UsageDelta {
    pub usage: TokenUsage,
    pub consumed: usize,
    pub last_id: Option<String>,
}

/// Sums the usage of the assistant lines in `bytes`. Only complete lines are read (the
/// bytes after the last `\n` are not consumed). Lines of one `message.id` (one per
/// content block) count once; `last_id` carries that across calls. Bad JSON is skipped.
pub fn usage_delta(bytes: &[u8], last_id: Option<&str>) -> UsageDelta {
    let consumed = bytes.iter().rposition(|&b| b == b'\n').map_or(0, |i| i + 1);
    let mut usage = TokenUsage::default();
    let mut last = last_id.map(String::from);
    for line in bytes[..consumed].split(|&b| b == b'\n') {
        let Ok(v) = serde_json::from_slice::<Value>(line) else {
            continue;
        };
        if v["type"].as_str() != Some("assistant") {
            continue;
        }
        let msg = &v["message"];
        if !msg["usage"].is_object() {
            continue;
        }
        if let Some(id) = msg["id"].as_str() {
            if last.as_deref() == Some(id) {
                continue;
            }
            last = Some(id.to_string());
        }
        let n = |k: &str| msg["usage"][k].as_u64().unwrap_or(0);
        let (input, output, cache_write) = (
            n("input_tokens"),
            n("output_tokens"),
            n("cache_creation_input_tokens"),
        );
        usage.add(&TokenUsage {
            total: input.saturating_add(cache_write).saturating_add(output),
            input,
            output,
            cache_write,
            cache_read: n("cache_read_input_tokens"),
        });
    }
    UsageDelta {
        usage,
        consumed,
        last_id: last,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn asst(id: &str, u: Value) -> String {
        format!(
            "{}\n",
            serde_json::json!({"type":"assistant","message":{"id":id,"usage":u}})
        )
    }

    #[test]
    fn usage_delta_counts_one_message_once_and_excludes_cache_read() {
        let u = serde_json::json!({"input_tokens":3,"output_tokens":5,"cache_creation_input_tokens":7,"cache_read_input_tokens":100});
        let text = asst("m1", u.clone()) + &asst("m1", u.clone()) + &asst("m2", u);
        let d = usage_delta(text.as_bytes(), None);
        assert_eq!(d.consumed, text.len());
        assert_eq!(d.last_id.as_deref(), Some("m2"));
        assert_eq!(
            d.usage,
            TokenUsage {
                total: 30,
                input: 6,
                output: 10,
                cache_write: 14,
                cache_read: 200
            }
        );
    }

    #[test]
    fn usage_sums_saturate() {
        let big = u64::MAX - 1;
        let u = serde_json::json!({"input_tokens":big,"output_tokens":big,"cache_creation_input_tokens":big});
        let text = asst("m1", u.clone()) + &asst("m2", u);
        let d = usage_delta(text.as_bytes(), None);
        assert_eq!(d.usage.total, u64::MAX);
        assert_eq!(d.usage.input, u64::MAX);
        let mut t = TokenUsage {
            total: u64::MAX,
            ..Default::default()
        };
        t.add(&TokenUsage {
            total: 5,
            ..Default::default()
        });
        assert_eq!(t.total, u64::MAX);
    }

    #[test]
    fn usage_delta_carries_last_id_across_calls() {
        let u = serde_json::json!({"input_tokens":1,"output_tokens":1});
        let text = asst("m1", u);
        let d = usage_delta(text.as_bytes(), Some("m1"));
        assert_eq!(d.usage, TokenUsage::default());
        assert_eq!(d.consumed, text.len());
    }

    #[test]
    fn usage_delta_leaves_partial_line_and_skips_noise() {
        let u = serde_json::json!({"output_tokens":4});
        let full = asst("m1", u.clone());
        let text = format!(
            "not json\n{{\"type\":\"user\"}}\n{full}{}",
            &asst("m2", u)[..20]
        );
        let d = usage_delta(text.as_bytes(), None);
        assert_eq!(d.consumed, text.len() - 20);
        assert_eq!(
            d.usage,
            TokenUsage {
                total: 4,
                output: 4,
                ..Default::default()
            }
        );
        assert_eq!(usage_delta(b"", None).consumed, 0);
        assert_eq!(usage_delta(b"{\"type\":\"assi", None).consumed, 0);
    }

    fn fixture(name: &str) -> Vec<u8> {
        std::fs::read(format!(
            "{}/tests/fixtures/transcripts/{name}.jsonl",
            env!("CARGO_MANIFEST_DIR")
        ))
        .unwrap()
    }

    fn kinds(entries: &[Entry]) -> Vec<String> {
        entries
            .iter()
            .map(|e| match e {
                Entry::User { .. } => "user".into(),
                Entry::Claude { .. } => "claude".into(),
                Entry::Tool { tool, .. } => format!("tool({tool})"),
                Entry::Marker { .. } => "marker".into(),
            })
            .collect()
    }

    #[test]
    fn basic_gives_ordered_entries_with_ids_and_times() {
        let p = parse(&fixture("basic"), 0);
        assert_eq!(
            kinds(&p.entries),
            [
                "user",
                "claude",
                "tool(Bash)",
                "tool(Edit)",
                "tool(Read)",
                "tool(Grep)",
                "tool(TodoWrite)",
                "claude"
            ]
        );
        assert_eq!(p.skipped, 0);
        let Entry::User {
            id,
            at,
            state,
            from,
            ..
        } = &p.entries[0]
        else {
            panic!()
        };
        assert_eq!(id, "t:u1:0");
        assert_eq!(at, "2025-01-01T10:00:01.000Z");
        assert!(DateTime::parse_from_rfc3339(at).is_ok());
        assert_eq!((*state, *from), (UserState::Delivered, UserFrom::Terminal));
        assert_eq!(p.next_offset as usize, fixture("basic").len());
    }

    #[test]
    fn bash_tool_has_summary_result_and_error() {
        let p = parse(&fixture("basic"), 0);
        let Entry::Tool {
            summary,
            result,
            is_error,
            tool_use_id,
            ..
        } = &p.entries[2]
        else {
            panic!()
        };
        assert_eq!(summary, "Run the tests");
        assert_eq!(result.as_deref(), Some("1 failed"));
        assert!(*is_error);
        assert_eq!(tool_use_id, "tb");
    }

    #[test]
    fn edit_tool_has_path_and_diff_counts() {
        let p = parse(&fixture("basic"), 0);
        let Entry::Tool { path, diff, .. } = &p.entries[3] else {
            panic!()
        };
        assert_eq!(path.as_deref(), Some("/tmp/w/src/a.rs"));
        let rows = diff.as_ref().unwrap();
        let count = |k| rows.iter().filter(|r| r.kind == k).count();
        assert_eq!(count(DiffKind::Del), 1);
        assert_eq!(count(DiffKind::Add), 2);
        assert_eq!(count(DiffKind::Hunk), 1);
        let del = rows.iter().find(|r| r.kind == DiffKind::Del).unwrap();
        assert_eq!(
            (del.old, del.new, del.text.as_str()),
            (Some(2), None, "two")
        );
        let add = rows.iter().find(|r| r.kind == DiffKind::Add).unwrap();
        assert_eq!((add.old, add.new), (None, Some(2)));
        let json = serde_json::to_value(&rows[0]).unwrap();
        assert_eq!(json["kind"], "hunk");
    }

    #[test]
    fn other_tools_have_summaries() {
        let p = parse(&fixture("basic"), 0);
        let summary = |i: usize| match &p.entries[i] {
            Entry::Tool { summary, .. } => summary.clone(),
            _ => panic!(),
        };
        assert_eq!(summary(4), "/tmp/w/src/a.rs:10-30");
        assert_eq!(summary(5), "greet (2)");
        assert_eq!(summary(6), "1/2 done: A; B");
    }

    #[test]
    fn unknown_parses_known_entries_only() {
        let p = parse(&fixture("unknown"), 0);
        assert_eq!(kinds(&p.entries), ["user", "claude"]);
        assert_eq!(p.skipped, 2);
    }

    #[test]
    fn partial_stops_before_the_incomplete_line() {
        let bytes = fixture("partial");
        let p = parse(&bytes, 0);
        assert_eq!(kinds(&p.entries), ["user", "claude"]);
        assert_eq!(p.skipped, 0);
        let start = bytes.iter().rposition(|b| *b == b'\n').unwrap() + 1;
        assert_eq!(p.next_offset as usize, start);

        let mut full = bytes.clone();
        full.extend_from_slice(
            br#"{"type":"assistant","uuid":"a2","timestamp":"2025-01-01T10:00:03.000Z","message":{"id":"m2","role":"assistant","content":[{"type":"text","text":"third part"}]}}"#,
        );
        full.push(b'\n');
        // The cut line was a prefix of the real one: rebuild from its start.
        let mut done = bytes[..start].to_vec();
        done.extend_from_slice(&full[bytes.len()..]);
        let again = parse(&done, p.next_offset);
        assert_eq!(kinds(&again.entries), ["claude"]);
        assert_eq!(again.next_offset as usize, done.len());
    }

    #[test]
    fn noise_is_skipped_but_compaction_marks() {
        let p = parse(&fixture("noise"), 0);
        assert_eq!(kinds(&p.entries), ["user", "marker", "claude"]);
        assert!(matches!(
            p.entries[1],
            Entry::Marker {
                marker: MarkerKind::Compacted,
                ..
            }
        ));
        assert_eq!(p.skipped, 0);
    }

    fn user_line(extra: &str, content: &str) -> Vec<u8> {
        format!(
            r#"{{"type":"user","uuid":"n1","timestamp":"2025-01-01T10:00:00.000Z"{extra},"message":{{"role":"user","content":{content}}}}}"#
        )
        .into_bytes()
        .into_iter()
        .chain(*b"\n")
        .collect()
    }

    const NOTE: &str = r#""<task-notification>\n<task-id>b1</task-id>\n<summary>Background command \"sleep 5\" completed\nsecond line</summary>\n</task-notification>""#;

    #[test]
    fn task_notification_is_a_background_done_marker() {
        for extra in [r#","origin":{"kind":"task-notification"}"#, ""] {
            let p = parse(&user_line(extra, NOTE), 0);
            assert_eq!(p.entries.len(), 1);
            let Entry::Marker { marker, detail, .. } = &p.entries[0] else {
                panic!("not a marker: {:?}", p.entries[0]);
            };
            assert_eq!(*marker, MarkerKind::BackgroundDone);
            assert_eq!(
                detail.as_deref(),
                Some("Background command \"sleep 5\" completed")
            );
        }
        let json = serde_json::to_value(&parse(&user_line("", NOTE), 0).entries[0]).unwrap();
        assert_eq!(json["marker"], "background-done");
    }

    #[test]
    fn task_notification_fills_background() {
        for extra in [r#","origin":{"kind":"task-notification"}"#, ""] {
            let p = parse(&user_line(extra, NOTE), 0);
            let Entry::Marker { background, .. } = &p.entries[0] else {
                panic!("not a marker");
            };
            let b = background.as_ref().expect("background");
            assert_eq!(b.task_id.as_deref(), Some("b1"));
            assert_eq!(b.kind, BackgroundKind::Command);
            assert_eq!(b.label.as_deref(), Some("sleep 5"));
        }
    }

    const HANDBACK: &str = r#","isMeta":true,"origin":{"kind":"peer","from":"a97d","name":"code-researcher","handback":true,"body":"[Subagent hand-back] x. The report follows:\n  ## Map\n  line two\n"}"#;

    #[test]
    fn handback_line_is_a_background_done_marker() {
        let p = parse(
            &user_line(HANDBACK, r#""Another Claude session sent a message""#),
            0,
        );
        assert_eq!(p.entries.len(), 1);
        let Entry::Marker {
            id,
            marker,
            detail,
            background,
            ..
        } = &p.entries[0]
        else {
            panic!("not a marker");
        };
        assert_eq!(id, "t:n1:0");
        assert_eq!(*marker, MarkerKind::BackgroundDone);
        assert_eq!(detail.as_deref(), Some("code-researcher"));
        let b = background.as_ref().expect("background");
        assert_eq!(b.source, BackgroundSource::Handback);
        assert_eq!(b.report.as_deref(), Some("## Map\nline two"));
    }

    #[test]
    fn notification_and_handback_get_different_ids() {
        let mut bytes = user_line("", NOTE);
        let other = String::from_utf8(user_line(HANDBACK, r#""m""#))
            .unwrap()
            .replace("\"n1\"", "\"n2\"");
        bytes.extend(other.into_bytes());
        let p = parse(&bytes, 0);
        let ids: Vec<_> = p
            .entries
            .iter()
            .map(|e| match e {
                Entry::Marker { id, .. } => id.clone(),
                _ => panic!("not a marker"),
            })
            .collect();
        assert_eq!(ids, ["t:n1:0", "t:n2:0"]);
    }

    #[test]
    fn other_meta_and_sidechain_lines_still_give_nothing() {
        let skill = r#","isMeta":true"#;
        assert!(
            parse(
                &user_line(skill, r#""Base directory for this skill: /x""#),
                0
            )
            .entries
            .is_empty()
        );
        let side = format!(r#","isSidechain":true{HANDBACK}"#);
        assert!(parse(&user_line(&side, r#""m""#), 0).entries.is_empty());
    }

    #[test]
    fn origin_notification_without_block_still_gives_a_marker() {
        let p = parse(
            &user_line(r#","origin":{"kind":"task-notification"}"#, r#""nothing""#),
            0,
        );
        assert_eq!(p.entries.len(), 1);
        let Entry::Marker {
            detail, background, ..
        } = &p.entries[0]
        else {
            panic!("not a marker");
        };
        assert_eq!(*detail, None);
        let b = background.as_ref().expect("background");
        assert_eq!(b.source, BackgroundSource::Notification);
        assert_eq!(b.kind, BackgroundKind::Task);
    }

    #[test]
    fn task_notification_without_summary_has_no_detail() {
        let p = parse(
            &user_line(
                "",
                r#""<task-notification><task-id>x</task-id></task-notification>""#,
            ),
            0,
        );
        let Entry::Marker { marker, detail, .. } = &p.entries[0] else {
            panic!("not a marker");
        };
        assert_eq!(*marker, MarkerKind::BackgroundDone);
        assert_eq!(*detail, None);
        let json = serde_json::to_value(&p.entries[0]).unwrap();
        assert!(json.get("detail").is_none());
    }

    #[test]
    fn step_summaries() {
        use serde_json::json;
        assert_eq!(
            tool_step_summary("Read", &json!({"file_path": "a.rs"})).as_deref(),
            Some("Leyendo a.rs")
        );
        assert_eq!(
            tool_step_summary("Grep", &json!({"pattern": "foo"})).as_deref(),
            Some("Buscando foo")
        );
        let long = "x".repeat(60);
        assert_eq!(
            tool_step_summary("Bash", &json!({"command": long}))
                .unwrap()
                .len(),
            40
        );
        assert_eq!(
            tool_step_summary("Edit", &json!({})).as_deref(),
            Some("Edit")
        );
    }

    #[test]
    fn sidechain_gives_only_the_task() {
        let p = parse(&fixture("sidechain"), 0);
        assert_eq!(kinds(&p.entries), ["user", "tool(Task)"]);
        let Entry::Tool {
            summary,
            input,
            result,
            ..
        } = &p.entries[1]
        else {
            panic!()
        };
        assert_eq!(summary, "Explore");
        assert_eq!(input["prompt"], "Look around the repo");
        assert_eq!(result.as_deref(), Some("Found three files"));
    }

    #[test]
    fn large_result_is_truncated_and_image_is_a_placeholder() {
        let bytes = fixture("large");
        let p = parse(&bytes, 0);
        assert_eq!(kinds(&p.entries), ["user", "tool(Bash)", "tool(Image)"]);
        let Entry::Tool {
            result, truncated, ..
        } = &p.entries[1]
        else {
            panic!()
        };
        assert_eq!(result.as_ref().unwrap().len(), 4096);
        assert!(*truncated);
        let full = tool_detail(&bytes, "tl").unwrap();
        assert_eq!(full.result.unwrap().len(), 20000);
        assert_eq!(full.input["command"], "cat big");
        assert!(tool_detail(&bytes, "nope").is_none());
    }

    #[test]
    fn pending_tool_has_no_result() {
        let p = parse(&fixture("pending"), 0);
        let Entry::Tool { result, .. } = &p.entries[1] else {
            panic!()
        };
        assert!(result.is_none());
        let json = serde_json::to_value(&p.entries[1]).unwrap();
        assert!(json["result"].is_null());
        assert_eq!(json["kind"], "tool");
    }

    #[test]
    fn ask_keeps_the_tool_use_id() {
        let p = parse(&fixture("ask"), 0);
        let Entry::Tool {
            tool,
            tool_use_id,
            summary,
            result,
            ..
        } = &p.entries[0]
        else {
            panic!()
        };
        assert_eq!(tool, "AskUserQuestion");
        assert_eq!(tool_use_id, "tq");
        assert_eq!(summary, "Which db?");
        assert_eq!(result.as_deref(), Some("User has answered: sqlite"));
    }

    #[test]
    fn parse_message_matches_the_line_mapping() {
        let bytes = fixture("basic");
        let line: Value =
            serde_json::from_slice(bytes.split(|b| *b == b'\n').nth(2).unwrap()).unwrap();
        let entries = parse_message(&line["message"]);
        assert_eq!(kinds(&entries), ["tool(Bash)"]);
        let from_file = &parse(&bytes, 0).entries[2];
        let (
            Entry::Tool {
                summary: a,
                input: ai,
                ..
            },
            Entry::Tool {
                summary: b,
                input: bi,
                ..
            },
        ) = (&entries[0], from_file)
        else {
            panic!()
        };
        assert_eq!((a, ai), (b, bi));
        let user = parse_message(&serde_json::json!({"role": "user", "content": "hello"}));
        assert_eq!(kinds(&user), ["user"]);
    }

    #[test]
    fn assistant_entries_carry_the_message_id_when_there_is_one() {
        let message = serde_json::json!({
            "id": "msg_1", "role": "assistant",
            "content": [
                {"type": "text", "text": "hola"},
                {"type": "tool_use", "id": "tu1", "name": "Bash", "input": {"command": "ls"}}
            ]
        });
        let entries = parse_message(&message);
        assert!(matches!(&entries[0], Entry::Claude { message_id: Some(m), .. } if m == "msg_1"));
        assert!(matches!(&entries[1], Entry::Tool { message_id: Some(m), .. } if m == "msg_1"));
        let json = serde_json::to_value(&entries[0]).unwrap();
        assert_eq!(json["message_id"], "msg_1");
        // From a transcript line, and absent when the message has no id.
        let line = serde_json::json!({
            "type": "assistant", "uuid": "u1", "timestamp": "2026-01-01T00:00:00Z",
            "message": message
        });
        let p = parse(format!("{line}\n").as_bytes(), 0);
        assert!(matches!(&p.entries[0], Entry::Claude { message_id: Some(m), .. } if m == "msg_1"));
        let none = parse_message(&serde_json::json!({"role": "assistant", "content": "x"}));
        assert!(matches!(
            &none[0],
            Entry::Claude {
                message_id: None,
                ..
            }
        ));
        assert!(
            serde_json::to_value(&none[0])
                .unwrap()
                .get("message_id")
                .is_none()
        );
    }

    #[test]
    fn transcript_path_names_the_folder_after_the_workspace() {
        let home = Path::new("/h");
        let path = transcript_path_in(home, Path::new("/nonexistent/my.ws"), "abc").unwrap();
        assert_eq!(
            path,
            Path::new("/h/.claude/projects/-nonexistent-my-ws/abc.jsonl")
        );
    }

    #[test]
    fn transcript_path_refuses_ids_that_escape() {
        let home = Path::new("/h");
        let ws = Path::new("/w");
        for bad in ["", "../x", "a/b", "a.b", &"x".repeat(MAX_ID_LEN + 1)] {
            assert!(transcript_path_in(home, ws, bad).is_none(), "{bad}");
        }
        assert!(transcript_path_in(home, ws, "9f-A_1").is_some());
    }

    #[test]
    fn edit_and_grep_summaries_carry_counts_and_path() {
        let p = parse(&fixture("basic"), 0);
        let Entry::Tool { summary, .. } = &p.entries[3] else {
            panic!()
        };
        assert_eq!(summary, "/tmp/w/src/a.rs +2 \u{2212}1");
        let grep = new_tool(
            "i".into(),
            "t".into(),
            "Grep",
            "x",
            &serde_json::json!({"pattern": "foo", "path": "src"}),
        );
        let Entry::Tool { summary, .. } = &grep else {
            panic!()
        };
        assert_eq!(summary, "foo in src");
    }

    #[test]
    fn read_summary_does_not_overflow() {
        let s = summary(
            "Read",
            &serde_json::json!({"file_path": "f", "offset": u64::MAX, "limit": 5}),
            None,
            None,
        );
        assert!(s.starts_with("f:"));
    }

    #[test]
    fn huge_diffs_are_cut() {
        let big = "x\n".repeat(MAX_DIFF_INPUT);
        let (rows, cut) = tool_diff("Write", &serde_json::json!({"content": big})).unwrap();
        assert!(cut && rows.is_empty());
        let many: String = (0..5000).map(|i| format!("line {i}\n")).collect();
        let (rows, cut) = tool_diff("Write", &serde_json::json!({"content": many})).unwrap();
        assert!(cut);
        assert!(rows.len() <= MAX_DIFF_ROWS);
        let long = "y".repeat(MAX_TEXT * 2);
        let (rows, _) = tool_diff("Write", &serde_json::json!({"content": long})).unwrap();
        assert!(rows.iter().all(|r| r.text.len() <= MAX_TEXT));
        let e = new_tool(
            "i".into(),
            "t".into(),
            "Write",
            "x",
            &serde_json::json!({"file_path": "f", "content": "x\n".repeat(MAX_DIFF_INPUT)}),
        );
        assert!(matches!(
            e,
            Entry::Tool {
                truncated: true,
                ..
            }
        ));
    }

    #[test]
    fn a_result_after_the_cursor_is_an_orphan() {
        let bytes = fixture("basic");
        let first = parse(&bytes, 0);
        // Cut right after the Bash tool_use line (third line).
        let mut off = 0usize;
        for _ in 0..3 {
            off += bytes[off..].iter().position(|b| *b == b'\n').unwrap() + 1;
        }
        let rest = parse(&bytes, off as u64);
        assert!(first.orphan_results.is_empty());
        let orphan = rest
            .orphan_results
            .iter()
            .find(|o| o.0 == "tb")
            .expect("orphan");
        assert_eq!(orphan.1.as_deref(), Some("1 failed"));
        assert!(orphan.2);
    }
}
