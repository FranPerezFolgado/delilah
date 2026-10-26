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
        assert_eq!(v["background"], json!({"source": "notification", "kind": "task"}));
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
