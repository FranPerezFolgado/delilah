//! What a background-task notification or hand-back says, in structured form, so a
//! client can render it instead of the raw markup.

use serde::Serialize;
use serde_json::Value;

use super::truncate;

/// Longest report kept, in bytes.
const MAX_REPORT: usize = 16 * 1024;

const PLACEHOLDER: &str = "This agent's report was delivered to you as a message from";
const PREAMBLE_END: &str = "The report follows:\n";

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
        Some(rest[..end].chars().take(200).collect::<String>())
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

/// Decode the five entities the notification escapes; everything else stays as written.
fn decode_entities(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find('&') {
        out.push_str(&rest[..i]);
        rest = &rest[i..];
        let decoded = [
            ("&lt;", '<'),
            ("&gt;", '>'),
            ("&amp;", '&'),
            ("&quot;", '"'),
            ("&#39;", '\''),
        ]
        .iter()
        .find(|(entity, _)| rest.starts_with(entity));
        match decoded {
            Some((entity, c)) => {
                out.push(*c);
                rest = &rest[entity.len()..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

/// The report of a notification: (report, truncated, delivered in the hand-back).
fn read_report(block: &str) -> (Option<String>, bool, bool) {
    let Some(start) = block.find("<result>").map(|i| i + "<result>".len()) else {
        return (None, false, false);
    };
    let rest = &block[start..];
    // Never past the notification's own closing tag.
    let rest = &rest[..rest.rfind("</task-notification>").unwrap_or(rest.len())];
    // The last `</result>`, so a report that quotes one stays whole.
    let end = rest
        .rfind("</result>")
        .or_else(|| rest.find("<usage>"))
        .unwrap_or(rest.len());
    let text = decode_entities(rest[..end].trim());
    if text.is_empty() {
        return (None, false, false);
    }
    if text.starts_with(PLACEHOLDER) {
        return (None, false, true);
    }
    let (report, truncated) = truncate(&text, MAX_REPORT);
    (Some(report), truncated, false)
}

/// The hand-back of a background agent, from the `origin` of its hidden user line.
pub(crate) fn read_handback(origin: &Value) -> Option<BackgroundInfo> {
    if origin.get("kind").and_then(Value::as_str) != Some("peer")
        || origin.get("handback").and_then(Value::as_bool) != Some(true)
    {
        return None;
    }
    let text = |key: &str| {
        origin
            .get(key)
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
    };
    let body = text("body").unwrap_or("");
    let body = match body.find(PREAMBLE_END) {
        Some(i) => &body[i + PREAMBLE_END.len()..],
        None => body,
    };
    let dedented = body
        .lines()
        .map(|l| l.strip_prefix("  ").unwrap_or(l))
        .collect::<Vec<_>>()
        .join("\n");
    let dedented = dedented.trim();
    let (report, report_truncated) = if dedented.is_empty() {
        (None, false)
    } else {
        let (r, t) = truncate(dedented, MAX_REPORT);
        (Some(r), t)
    };
    Some(BackgroundInfo {
        source: BackgroundSource::Handback,
        task_id: text("from").map(|s| s.chars().take(200).collect()),
        kind: BackgroundKind::Agent,
        label: text("name").map(|n| n.chars().take(200).collect()),
        status: Some("completed".into()),
        report,
        report_truncated,
        ..Default::default()
    })
}

/// A piece of a user text: what the user wrote, or a `<task-notification>` block.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Segment<'a> {
    Text(&'a str),
    Notification(&'a str),
}

const OPEN: &str = "<task-notification>";
const CLOSE: &str = "</task-notification>";

/// Split a text into its notifications and the text around them, in order. A
/// notification ends at its closing tag, else at the next opening tag or the end.
pub(crate) fn split_notifications(text: &str) -> Vec<Segment<'_>> {
    let mut out = Vec::new();
    let mut rest = text;
    while let Some(i) = rest.find(OPEN) {
        if i > 0 {
            out.push(Segment::Text(&rest[..i]));
        }
        let block = &rest[i..];
        let after = OPEN.len();
        let next_open = block[after..].find(OPEN).map(|j| j + after);
        // Look for the close tag only before the next opening, so the split stays linear.
        let limit = next_open.unwrap_or(block.len());
        let end = match block[after..limit].find(CLOSE) {
            Some(j) => j + after + CLOSE.len(),
            None => limit,
        };
        out.push(Segment::Notification(&block[..end]));
        rest = &block[end..];
    }
    if !rest.is_empty() || out.is_empty() {
        out.push(Segment::Text(rest));
    }
    out
}

/// Whether a user text comes from another agent or the CLI, not from the user: it has a
/// `<task-notification>` anywhere, or is a message from another Claude session.
pub fn is_agent_message(text: &str) -> bool {
    text.contains(OPEN)
        || text
            .trim_start()
            .starts_with("Another Claude session sent a message")
}

/// Read a `<task-notification>` block. Never panics: whatever cannot be read stays
/// `None`. `tool-use-id`, `output-file` and `note` are not part of it.
pub(crate) fn read_notification(block: &str) -> BackgroundInfo {
    let usage = tag(block, "usage").unwrap_or("");
    let (kind, label, exit_code) =
        tag(block, "summary")
            .map(read_summary)
            .unwrap_or((BackgroundKind::Task, None, None));
    let (report, report_truncated, report_in_handback) = read_report(block);
    BackgroundInfo {
        source: BackgroundSource::Notification,
        task_id: tag(block, "task-id")
            .filter(|s| !s.is_empty())
            .map(|s| s.chars().take(200).collect()),
        kind,
        label,
        status: tag(block, "status")
            .filter(|s| !s.is_empty())
            .map(str::to_lowercase),
        exit_code,
        duration_ms: number(tag(usage, "duration_ms")),
        tool_uses: number(tag(usage, "tool_uses")),
        tokens: number(tag(usage, "subagent_tokens")),
        report,
        report_truncated,
        report_in_handback,
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

    fn with_result(result: &str) -> String {
        format!(
            "<task-notification><task-id>a</task-id><status>completed</status><result>{result}</result>\n<usage><tool_uses>1</tool_uses></usage></task-notification>"
        )
    }

    #[test]
    fn report_keeps_quoted_closing_tag_and_decodes() {
        let b = read_notification(&with_result(
            "a </result> b &lt;x&gt; &amp;amp; &quot;q&quot; &#39;s&#39; <raw>",
        ));
        assert_eq!(
            b.report.as_deref(),
            Some("a </result> b <x> &amp; \"q\" 's' <raw>")
        );
        assert!(!b.report_truncated && !b.report_in_handback);
        assert_eq!(b.tool_uses, Some(1));
    }

    #[test]
    fn report_placeholder_and_missing() {
        let b = read_notification(&with_result(
            "This agent's report was delivered to you as a message from \"a1\" (its SubagentHandback call). Read it there; it is not repeated here.",
        ));
        assert_eq!((b.report, b.report_in_handback), (None, true));
        let b = read_notification(AGENT);
        assert_eq!(
            (b.report, b.report_truncated, b.report_in_handback),
            (None, false, false)
        );
    }

    #[test]
    fn long_report_is_cut_on_a_char_boundary() {
        let b = read_notification(&with_result(&"é".repeat(MAX_REPORT)));
        assert!(b.report_truncated);
        let r = b.report.unwrap();
        assert_eq!(r.len(), MAX_REPORT);
        assert!(r.chars().all(|c| c == 'é'));
        let b = read_notification(&with_result(&"é".repeat(MAX_REPORT / 2 - 1)));
        assert!(!b.report_truncated);
    }

    fn handback(body: &str) -> Value {
        json!({"kind":"peer","from":"a97d","name":"code-researcher","handback":true,"body":body})
    }

    #[test]
    fn reads_handback() {
        let b = read_handback(&handback(
            "[Subagent hand-back] The text below is the final report. The report follows:\n  ## Map\n    nested\n x\n",
        ))
        .unwrap();
        assert_eq!(b.source, BackgroundSource::Handback);
        assert_eq!(b.kind, BackgroundKind::Agent);
        assert_eq!(b.task_id.as_deref(), Some("a97d"));
        assert_eq!(b.label.as_deref(), Some("code-researcher"));
        assert_eq!(b.status.as_deref(), Some("completed"));
        assert_eq!(b.report.as_deref(), Some("## Map\n  nested\n x"));
        let b = read_handback(&handback("plain body")).unwrap();
        assert_eq!(b.report.as_deref(), Some("plain body"));
    }

    #[test]
    fn handback_requires_flag_and_peer_kind() {
        let mut o = handback("x");
        o["handback"] = json!(false);
        assert!(read_handback(&o).is_none());
        let mut o = handback("x");
        o["kind"] = json!("task-notification");
        assert!(read_handback(&o).is_none());
        assert!(read_handback(&Value::Null).is_none());
    }

    #[test]
    fn labels_are_capped_at_200() {
        let long = "é".repeat(300);
        let b = read_notification(&format!("<summary>Agent \"{long}\" finished</summary>"));
        assert_eq!(b.label.unwrap().chars().count(), 200);
        let mut o = handback("x");
        o["name"] = json!(long);
        assert_eq!(
            read_handback(&o).unwrap().label.unwrap().chars().count(),
            200
        );
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

    #[test]
    fn splits_notifications_and_text() {
        use Segment::*;
        let a = "<task-notification>a</task-notification>";
        let b = "<task-notification>b</task-notification>";
        assert_eq!(
            split_notifications(&format!("{a}{b}")),
            [Notification(a), Notification(b)]
        );
        assert_eq!(
            split_notifications(&format!("hi {a} bye")),
            [Text("hi "), Notification(a), Text(" bye")]
        );
        assert_eq!(
            split_notifications("x<task-notification>a<task-notification>b"),
            [
                Text("x"),
                Notification("<task-notification>a"),
                Notification("<task-notification>b")
            ]
        );
        assert_eq!(
            split_notifications("<task-notification>áé"),
            [Notification("<task-notification>áé")]
        );
        assert_eq!(split_notifications("plain"), [Text("plain")]);
    }

    #[test]
    fn many_unclosed_tags_split_in_linear_time() {
        let text = "<task-notification>".repeat(20_000);
        let start = std::time::Instant::now();
        let segments = split_notifications(&text);
        assert_eq!(segments.len(), 20_000);
        assert!(start.elapsed() < std::time::Duration::from_secs(2));
    }

    #[test]
    fn unclosed_result_never_keeps_the_closing_tag() {
        let b = read_notification("<task-notification><result>done</task-notification>");
        assert_eq!(b.report.as_deref(), Some("done"));
        let b =
            read_notification("<task-notification><result>done<usage></usage></task-notification>");
        assert_eq!(b.report.as_deref(), Some("done"));
    }

    #[test]
    fn recognises_agent_messages() {
        assert!(is_agent_message("hola <task-notification>"));
        assert!(is_agent_message(
            "\n  Another Claude session sent a message:\n<agent-message>"
        ));
        assert!(!is_agent_message("what is a task-notification?"));
    }

    #[test]
    fn task_id_is_capped_at_200() {
        let long = "x".repeat(300);
        let b = read_notification(&format!("<task-notification><task-id>{long}</task-id>"));
        assert_eq!(b.task_id.unwrap().chars().count(), 200);
        let h = read_handback(&json!({"kind":"peer","handback":true,"from":long}));
        assert_eq!(h.unwrap().task_id.unwrap().chars().count(), 200);
    }
}
