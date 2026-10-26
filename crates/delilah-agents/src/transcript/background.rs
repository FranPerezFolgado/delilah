//! What a background-task notification or hand-back says, in structured form, so a
//! client can render it instead of the raw markup.

use serde::Serialize;

/// Where the information came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum BackgroundSource {
    /// A `<task-notification>` user line.
    #[default]
    Notification,
    /// The sub-agent's report handed back to the main agent.
    Handback,
}

/// What kind of background work it was.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum BackgroundKind {
    Agent,
    Command,
    #[default]
    Task,
}

/// The parsed content of a background-task notification (absent fields are unknown).
#[derive(Debug, Clone, PartialEq, Default, Serialize)]
pub struct BackgroundInfo {
    pub source: BackgroundSource,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub task_id: Option<String>,
    pub kind: BackgroundKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_uses: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tokens: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub report: Option<String>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub report_truncated: bool,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub report_in_handback: bool,
}

/// Contents of the first `<name>` tag: up to its `</name>`, else up to the next tag
/// start or the end of the block (a truncated block still gives what was written).
fn tag<'a>(block: &'a str, name: &str) -> Option<&'a str> {
    let open = format!("<{name}>");
    let start = block.find(&open)? + open.len();
    let rest = &block[start..];
    let end = rest
        .find(&format!("</{name}>"))
        .or_else(|| rest.find('<'))
        .unwrap_or(rest.len());
    Some(rest[..end].trim())
}

fn number(value: Option<&str>) -> Option<u64> {
    value?.trim().parse().ok()
}

/// Kind, label and exit code from the notification summary.
fn read_summary(summary: &str) -> (BackgroundKind, Option<String>, Option<i64>) {
    let first = summary.trim().lines().next().unwrap_or("").trim();
    let quoted = |prefix: &str| {
        let rest = first.strip_prefix(prefix)?;
        let end = rest.rfind('"')?;
        Some(rest[..end].to_string())
    };
    if let Some(label) = quoted("Agent \"") {
        return (BackgroundKind::Agent, Some(label), None);
    }
    if let Some(label) = quoted("Background command \"") {
        let exit_code = first.rfind("(exit code ").and_then(|i| {
            let digits = &first[i + "(exit code ".len()..];
            digits[..digits.find(')')?].trim().parse().ok()
        });
        return (BackgroundKind::Command, Some(label), exit_code);
    }
    let label: String = first.chars().take(200).collect();
    (
        BackgroundKind::Task,
        (!label.is_empty()).then_some(label),
        None,
    )
}

/// Read a `<task-notification>` block. Never panics: whatever cannot be read stays
/// `None`. `tool-use-id`, `output-file`, `note` and `result` are not part of it.
pub(crate) fn read_notification(block: &str) -> BackgroundInfo {
    let usage = tag(block, "usage").unwrap_or("");
    let (kind, label, exit_code) =
        tag(block, "summary")
            .map(read_summary)
            .unwrap_or((BackgroundKind::Task, None, None));
    BackgroundInfo {
        source: BackgroundSource::Notification,
        task_id: tag(block, "task-id")
            .filter(|s| !s.is_empty())
            .map(String::from),
        kind,
        label,
        status: tag(block, "status")
            .filter(|s| !s.is_empty())
            .map(str::to_lowercase),
        exit_code,
        duration_ms: number(tag(usage, "duration_ms")),
        tool_uses: number(tag(usage, "tool_uses")),
        tokens: number(tag(usage, "subagent_tokens")),
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transcript::{Entry, MarkerKind};
    use serde_json::json;

    fn marker(detail: &str, background: Option<BackgroundInfo>) -> serde_json::Value {
        serde_json::to_value(Entry::Marker {
            id: "t:u:0".into(),
            at: "2026-01-01T00:00:00Z".into(),
            marker: MarkerKind::BackgroundDone,
            detail: Some(detail.into()),
            background,
        })
        .unwrap()
    }

    const AGENT: &str = "<task-notification>\n<task-id>a696</task-id>\n<tool-use-id>toolu_1</tool-use-id>\n<output-file>/tmp/x.output</output-file>\n<status>Completed</status>\n<summary>Agent \"WP-07 update status.md\" finished</summary>\n<usage><subagent_tokens>19864</subagent_tokens><tool_uses>4</tool_uses><duration_ms>16270</duration_ms></usage>\n<note>secret note</note>\n</task-notification>";

    #[test]
    fn reads_agent_notification() {
        let b = read_notification(AGENT);
        assert_eq!(b.source, BackgroundSource::Notification);
        assert_eq!(b.task_id.as_deref(), Some("a696"));
        assert_eq!(b.kind, BackgroundKind::Agent);
        assert_eq!(b.label.as_deref(), Some("WP-07 update status.md"));
        assert_eq!(b.status.as_deref(), Some("completed"));
        assert_eq!(b.tokens, Some(19864));
        assert_eq!(b.tool_uses, Some(4));
        assert_eq!(b.duration_ms, Some(16270));
        assert_eq!(b.exit_code, None);
        let dbg = format!("{b:?}");
        for hidden in ["toolu_1", "x.output", "secret note"] {
            assert!(!dbg.contains(hidden));
        }
    }

    #[test]
    fn reads_command_exit_codes() {
        let s = |code: &str| {
            read_notification(&format!(
                "<task-notification><summary>Background command \"Sleep 25 seconds in background\" completed (exit code {code})</summary></task-notification>"
            ))
        };
        let b = s("0");
        assert_eq!(b.kind, BackgroundKind::Command);
        assert_eq!(b.label.as_deref(), Some("Sleep 25 seconds in background"));
        assert_eq!(b.exit_code, Some(0));
        assert_eq!(s("1").exit_code, Some(1));
        assert_eq!(s("x").exit_code, None);
    }

    #[test]
    fn other_summary_is_a_task_with_first_line() {
        let b = read_notification("<summary>Monitor fired\nsecond</summary>");
        assert_eq!(b.kind, BackgroundKind::Task);
        assert_eq!(b.label.as_deref(), Some("Monitor fired"));
        let long = format!("<summary>{}</summary>", "é".repeat(300));
        assert_eq!(read_notification(&long).label.unwrap().chars().count(), 200);
    }

    #[test]
    fn truncated_and_garbage_blocks() {
        let b = read_notification("<task-notification>\n<task-id>z</task-id>\n<status>completed");
        assert_eq!(b.task_id.as_deref(), Some("z"));
        assert_eq!(b.status.as_deref(), Some("completed"));
        let b = read_notification(
            "<usage><subagent_tokens>many</subagent_tokens><tool_uses>-1</tool_uses></usage>",
        );
        assert_eq!((b.tokens, b.tool_uses, b.duration_ms), (None, None, None));
        assert_eq!(read_notification(""), BackgroundInfo::default());
        let _ = read_notification("<summary>Agent \"é");
        let _ = read_notification("<summary>Background command \"é\" (exit code ");
    }

    #[test]
    fn agent_with_usage_and_report() {
        let v = marker(
            "Agent \"WP-07\" finished",
            Some(BackgroundInfo {
                task_id: Some("ad1c".into()),
                kind: BackgroundKind::Agent,
                label: Some("WP-07".into()),
                status: Some("completed".into()),
                duration_ms: Some(89665),
                tool_uses: Some(2),
                tokens: Some(12317),
                report: Some("## done".into()),
                ..Default::default()
            }),
        );
        assert_eq!(
            v["background"],
            json!({"source": "notification", "task_id": "ad1c", "kind": "agent",
                "label": "WP-07", "status": "completed", "duration_ms": 89665,
                "tool_uses": 2, "tokens": 12317, "report": "## done"})
        );
    }

    #[test]
    fn command_with_exit_code() {
        let v = marker(
            "x",
            Some(BackgroundInfo {
                task_id: Some("bbe".into()),
                kind: BackgroundKind::Command,
                label: Some("Sleep".into()),
                status: Some("completed".into()),
                exit_code: Some(0),
                ..Default::default()
            }),
        );
        assert_eq!(
            v["background"],
            json!({"source": "notification", "task_id": "bbe", "kind": "command",
                "label": "Sleep", "status": "completed", "exit_code": 0})
        );
    }

    #[test]
    fn handback_and_flags() {
        let v = marker(
            "code-researcher",
            Some(BackgroundInfo {
                source: BackgroundSource::Handback,
                kind: BackgroundKind::Agent,
                report_in_handback: true,
                report_truncated: true,
                ..Default::default()
            }),
        );
        assert_eq!(
            v["background"],
            json!({"source": "handback", "kind": "agent",
                "report_truncated": true, "report_in_handback": true})
        );
        let v = marker("x", Some(BackgroundInfo::default()));
        assert_eq!(
            v["background"],
            json!({"source": "notification", "kind": "task"})
        );
    }

    #[test]
    fn marker_without_background_is_unchanged() {
        let v = serde_json::to_value(Entry::Marker {
            id: "t:u:0".into(),
            at: "a".into(),
            marker: MarkerKind::Compacted,
            detail: None,
            background: None,
        })
        .unwrap();
        assert!(v.get("background").is_none());
        assert_eq!(
            v,
            json!({"id": "t:u:0", "at": "a", "kind": "marker", "marker": "compacted"})
        );
    }
}
