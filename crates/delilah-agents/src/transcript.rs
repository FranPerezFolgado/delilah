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

/// Tool results and long input strings are cut to this many bytes in entries.
pub const MAX_TEXT: usize = 4096;
/// Longest one-line summary, in characters.
const MAX_SUMMARY: usize = 120;

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
    },
    Marker {
        id: String,
        at: String,
        marker: MarkerKind,
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
}

/// The untruncated input and result of one tool call.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ToolDetail {
    pub input: Value,
    pub result: Option<String>,
    pub is_error: bool,
}

/// Where Claude saves the transcript of `session_id` started in `workspace`
/// (`None` without `$HOME`). The folder is named after the workspace path.
pub fn transcript_path(workspace: &Path, session_id: &str) -> Option<PathBuf> {
    let home = std::env::var_os("HOME")?;
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
        Path::new(&home)
            .join(".claude/projects")
            .join(folder)
            .join(format!("{session_id}.jsonl")),
    )
}

/// Something one line yields: an entry, or the result of an earlier tool call.
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
/// fills the entry of its `tool_use` when both are in `bytes`.
pub fn parse(bytes: &[u8], from_offset: u64) -> Parsed {
    let mut pos = usize::try_from(from_offset)
        .unwrap_or(usize::MAX)
        .min(bytes.len());
    let mut entries: Vec<Entry> = Vec::new();
    let mut by_tool: HashMap<String, usize> = HashMap::new();
    let mut skipped = 0;
    while let Some(nl) = bytes[pos..].iter().position(|b| *b == b'\n') {
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
                    }
                }
            }
        }
    }
    Parsed {
        entries,
        next_offset: pos as u64,
        skipped,
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
    if flag("isSidechain") || flag("isMeta") {
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
    if flag("isCompactSummary") {
        return vec![Item::Entry(Entry::Marker {
            id: format!("t:{uuid}:0"),
            at,
            marker: MarkerKind::Compacted,
        })];
    }
    match line.get("message") {
        Some(message) => message_items(message, uuid, &at, skipped),
        None => {
            *skipped += 1;
            vec![]
        }
    }
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
            })),
            _ => *skipped += 1,
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
    let diff = tool_diff(name, input);
    let mut stored = input.clone();
    let truncated = truncate_strings(&mut stored);
    Entry::Tool {
        id,
        at,
        tool: name.to_string(),
        tool_use_id: tool_use_id.to_string(),
        summary: summary(name, &stored, None),
        input: stored,
        result: None,
        is_error: false,
        truncated,
        diff,
        path,
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
        ..
    } = entry
    {
        let (cut, was_cut) = truncate(text, MAX_TEXT);
        *s = summary(tool, input, Some(text));
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
fn summary(name: &str, input: &Value, result: Option<&str>) -> String {
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
        "Edit" | "MultiEdit" | "Write" | "NotebookEdit" => str_of(input, "file_path").to_string(),
        "Read" => {
            let path = str_of(input, "file_path");
            match (
                input.get("offset").and_then(Value::as_u64),
                input.get("limit").and_then(Value::as_u64),
            ) {
                (Some(o), Some(l)) => format!("{path}:{o}-{}", o + l),
                (Some(o), None) => format!("{path}:{o}-"),
                (None, Some(l)) => format!("{path}:1-{l}"),
                _ => path.to_string(),
            }
        }
        "Grep" | "Glob" => {
            let pattern = str_of(input, "pattern");
            match result {
                Some(r) => {
                    let n = r.lines().filter(|l| !l.trim().is_empty()).count();
                    format!("{pattern} ({n})")
                }
                None => pattern.to_string(),
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

fn tool_diff(name: &str, input: &Value) -> Option<Vec<DiffRow>> {
    match name {
        "Edit" => Some(diff_rows(
            str_of(input, "old_string"),
            str_of(input, "new_string"),
        )),
        "MultiEdit" => {
            let edits = input.get("edits")?.as_array()?;
            Some(
                edits
                    .iter()
                    .flat_map(|e| diff_rows(str_of(e, "old_string"), str_of(e, "new_string")))
                    .collect(),
            )
        }
        "Write" => Some(diff_rows("", str_of(input, "content"))),
        _ => None,
    }
}

/// Line diff of `old` to `new` with three lines of context; line numbers count from the
/// start of each text.
fn diff_rows(old: &str, new: &str) -> Vec<DiffRow> {
    let diff = TextDiff::from_lines(old, new);
    let num = |n: usize| u32::try_from(n + 1).ok();
    let mut rows = Vec::new();
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
                let text = change.value().trim_end_matches(['\n', '\r']).to_string();
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
    rows
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn transcript_path_names_the_folder_after_the_workspace() {
        let path = transcript_path(Path::new("/nonexistent/my.ws"), "abc").unwrap();
        assert!(path.ends_with(".claude/projects/-nonexistent-my-ws/abc.jsonl"));
    }
}
