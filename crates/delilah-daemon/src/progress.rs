//! The live progress of a chat-mode session (turn, background work, sub-agents), a pure
//! reducer over the stream events. Live state only: never persisted (FR-016).

use std::collections::HashMap;

use chrono::{DateTime, Duration, Utc};
use delilah_agents::stream::{StreamEvent, TaskEvent, TaskKind, TaskStatus};
use delilah_agents::transcript::tool_step_summary;
use serde::Serialize;
use serde_json::Value;

/// Sub-agent steps kept (FR-006); `steps - recent.len()` were hidden.
const RECENT_MAX: usize = 50;
/// How long a finished background row stays in the strip (FR-010).
const FINAL_KEEP: Duration = Duration::seconds(60);
const SELF_STARTED_REASON: &str = "tarea en segundo plano terminada";

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct Progress {
    pub turn: Option<Turn>,
    pub background: Vec<BackgroundItem>,
    pub subagents: Vec<Subagent>,
    /// task_id -> sub-agent tool_use_id, to route `Ended`/`Progress` events.
    #[serde(skip)]
    task_agents: HashMap<String, String>,
    /// Turns started so far (counted at `Init`), to tell when a sub-agent's end is a turn old.
    #[serde(skip)]
    turns: u64,
    /// A task ended since the last `Init`: a self-started turn after it has a reason.
    #[serde(skip)]
    task_ended: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Turn {
    pub started_at: DateTime<Utc>,
    /// `user` | `claude` (self-started).
    pub by: &'static str,
    pub reason: Option<String>,
    pub step: Option<String>,
    pub last_signal_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct BackgroundItem {
    pub task_id: String,
    pub tool_use_id: Option<String>,
    /// `command` | `agent` | `other`.
    pub kind: &'static str,
    pub description: String,
    /// `running` | `completed` | `failed` | `stopped` | `unknown`.
    pub state: &'static str,
    pub started_at: Option<DateTime<Utc>>,
    pub ended_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Subagent {
    pub tool_use_id: String,
    pub description: String,
    pub state: &'static str,
    pub started_at: DateTime<Utc>,
    pub ended_at: Option<DateTime<Utc>>,
    pub steps: u32,
    pub recent: Vec<Step>,
    /// `steps` comes from the stream's `tool_uses`, not from counting.
    #[serde(skip)]
    counted_by_stream: bool,
    /// `Progress::turns` when it finished: dropped when the turn after that ends.
    #[serde(skip)]
    ended_turn: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Step {
    pub tool: String,
    pub summary: String,
    pub at: DateTime<Utc>,
}

fn kind_str(k: &TaskKind) -> &'static str {
    match k {
        TaskKind::Command => "command",
        TaskKind::Agent => "agent",
        TaskKind::Other(_) => "other",
    }
}

fn status_str(s: TaskStatus) -> &'static str {
    match s {
        TaskStatus::Running => "running",
        TaskStatus::Completed => "completed",
        TaskStatus::Failed => "failed",
        TaskStatus::Stopped => "stopped",
        TaskStatus::Unknown => "unknown",
    }
}

fn step_text(tool: &str, input: &Value) -> String {
    tool_step_summary(tool, input).unwrap_or_else(|| tool.to_string())
}

impl Progress {
    /// Any background item still running.
    pub fn has_background(&self) -> bool {
        self.background.iter().any(|b| b.state == "running")
    }

    fn subagent(&mut self, id: &str, description: &str, now: DateTime<Utc>) -> &mut Subagent {
        if let Some(i) = self.subagents.iter().position(|s| s.tool_use_id == id) {
            return &mut self.subagents[i];
        }
        self.subagents.push(Subagent {
            tool_use_id: id.to_string(),
            description: description.to_string(),
            state: "running",
            started_at: now,
            ended_at: None,
            steps: 0,
            recent: vec![],
            counted_by_stream: false,
            ended_turn: None,
        });
        self.subagents.last_mut().unwrap()
    }

    fn set_step(&mut self, step: String) {
        if let Some(t) = &mut self.turn {
            t.step = Some(step);
        }
    }

    fn row(&mut self, task_id: &str) -> Option<&mut BackgroundItem> {
        self.background.iter_mut().find(|b| b.task_id == task_id)
    }
}

/// Applies one stream event; returns whether anything visible changed.
pub fn apply(p: &mut Progress, event: &StreamEvent, now: DateTime<Utc>) -> bool {
    let before = p.clone();
    if let Some(t) = &mut p.turn {
        t.last_signal_at = now;
    }
    match event {
        StreamEvent::Init { self_started, .. } => {
            p.background.retain(|b| b.state == "running");
            p.turns += 1;
            let by = if *self_started { "claude" } else { "user" };
            let reason = (*self_started && p.task_ended).then(|| SELF_STARTED_REASON.to_string());
            p.task_ended = false;
            p.turn = Some(Turn {
                started_at: now,
                by,
                reason,
                step: None,
                last_signal_at: now,
            });
        }
        StreamEvent::ToolStarted { tool, input, .. } if tool == "Agent" || tool == "Task" => {
            match input.get("description").and_then(Value::as_str) {
                Some(d) if !d.is_empty() => p.set_step(format!("Subagente: {d}")),
                _ => p.set_step(step_text(tool, input)),
            }
        }
        StreamEvent::ToolStarted { tool, input, .. } => p.set_step(step_text(tool, input)),
        StreamEvent::SubagentStep {
            parent,
            tool,
            input,
        } => {
            let summary = step_text(tool, input);
            let agent = p.subagent(parent, "", now);
            if !agent.counted_by_stream {
                agent.steps += 1;
            }
            agent.recent.push(Step {
                tool: tool.clone(),
                summary: summary.clone(),
                at: now,
            });
            if agent.recent.len() > RECENT_MAX {
                agent.recent.remove(0);
            }
            let label = if agent.description.is_empty() {
                "Subagente".to_string()
            } else {
                format!("Subagente: {}", agent.description)
            };
            p.set_step(format!("{label} · {summary}"));
        }
        StreamEvent::Task(task) => apply_task(p, task, now),
        StreamEvent::TurnResult { .. } => {
            p.turn = None;
            // Finished sub-agents go when the turn after their end ends (the card has the result).
            let turns = p.turns;
            p.subagents
                .retain(|s| s.ended_turn.is_none_or(|e| turns <= e));
        }
        StreamEvent::Exited { .. } => {
            p.turn = None;
            for b in p.background.iter_mut().filter(|b| b.state == "running") {
                b.state = "stopped";
                b.ended_at = Some(now);
            }
            for s in p.subagents.iter_mut().filter(|s| s.state == "running") {
                s.state = "stopped";
                s.ended_at = Some(now);
                s.ended_turn = Some(p.turns);
            }
        }
        _ => {}
    }
    *p != before
}

fn apply_task(p: &mut Progress, task: &TaskEvent, now: DateTime<Utc>) {
    match task {
        TaskEvent::Started {
            task_id,
            tool_use_id,
            kind,
            description,
            background,
        } => {
            if *kind == TaskKind::Agent
                && let Some(id) = tool_use_id
            {
                p.task_agents.insert(task_id.clone(), id.clone());
                let agent = p.subagent(id, description, now);
                if agent.description.is_empty() {
                    agent.description = description.clone();
                }
                let has_step = p.turn.as_ref().is_some_and(|t| t.step.is_some());
                if !has_step && !description.is_empty() {
                    p.set_step(format!("Subagente: {description}"));
                }
            }
            if *background {
                if let Some(b) = p.row(task_id) {
                    b.tool_use_id = tool_use_id.clone();
                    b.description = description.clone();
                    b.started_at.get_or_insert(now);
                } else {
                    p.background.push(BackgroundItem {
                        task_id: task_id.clone(),
                        tool_use_id: tool_use_id.clone(),
                        kind: kind_str(kind),
                        description: description.clone(),
                        state: "running",
                        started_at: Some(now),
                        ended_at: None,
                    });
                }
            }
        }
        TaskEvent::Progress {
            task_id,
            tool_use_id,
            description,
            tool_uses,
            ..
        } => {
            let id = tool_use_id
                .clone()
                .or_else(|| p.task_agents.get(task_id).cloned());
            let Some(id) = id.filter(|id| p.subagents.iter().any(|s| &s.tool_use_id == id)) else {
                return;
            };
            let agent = p.subagent(&id, "", now);
            if let Some(n) = tool_uses {
                agent.steps = *n;
                agent.counted_by_stream = true;
            }
            if let Some(d) = description.as_ref().filter(|d| !d.is_empty()) {
                let label = agent.description.clone();
                let text = if label.is_empty() {
                    format!("Subagente · {d}")
                } else {
                    format!("Subagente: {label} · {d}")
                };
                p.set_step(text);
            }
        }
        TaskEvent::Ended {
            task_id, status, ..
        } => {
            let turns = p.turns;
            let state = status_str(*status);
            p.task_ended = true;
            if let Some(b) = p.row(task_id).filter(|b| b.state == "running") {
                b.state = state;
                b.ended_at = Some(now);
            }
            if let Some(id) = p.task_agents.get(task_id).cloned() {
                let agent = p.subagent(&id, "", now);
                if agent.state == "running" {
                    agent.state = match status {
                        TaskStatus::Failed => "failed",
                        TaskStatus::Stopped => "stopped",
                        _ => "completed",
                    };
                    agent.ended_at = Some(now);
                    agent.ended_turn = Some(turns);
                }
            }
        }
        TaskEvent::Running {
            task_ids,
            described,
        } => {
            for (id, kind, description) in described {
                if p.row(id).is_none() {
                    p.background.push(BackgroundItem {
                        task_id: id.clone(),
                        tool_use_id: None,
                        kind: kind_str(kind),
                        description: description.clone(),
                        state: "running",
                        started_at: None,
                        ended_at: None,
                    });
                }
            }
            for id in task_ids {
                if p.row(id).is_none() {
                    p.background.push(BackgroundItem {
                        task_id: id.clone(),
                        tool_use_id: None,
                        kind: "other",
                        description: String::new(),
                        state: "running",
                        started_at: None,
                        ended_at: None,
                    });
                }
            }
            for b in p.background.iter_mut() {
                if b.state == "running" && !task_ids.contains(&b.task_id) {
                    b.state = "completed";
                    b.ended_at = Some(now);
                    p.task_ended = true;
                }
            }
        }
    }
}

/// Drops final rows 60 s after they ended, and sub-agents too when no turn is running (during
/// a turn they stay until the turn after their end ends); returns whether any went.
pub fn tick(p: &mut Progress, now: DateTime<Utc>) -> bool {
    let expired = |ended: Option<DateTime<Utc>>| ended.is_some_and(|e| now - e >= FINAL_KEEP);
    let (nb, ns) = (p.background.len(), p.subagents.len());
    p.background.retain(|b| !expired(b.ended_at));
    if p.turn.is_none() {
        p.subagents.retain(|s| !expired(s.ended_at));
    }
    (nb, ns) != (p.background.len(), p.subagents.len())
}

#[cfg(test)]
mod tests {
    use super::*;
    use delilah_agents::stream::StreamSession;
    use serde_json::json;
    use tokio::process::Command;

    fn t(secs: i64) -> DateTime<Utc> {
        DateTime::from_timestamp(1_800_000_000 + secs, 0).unwrap()
    }

    fn init(self_started: bool) -> StreamEvent {
        StreamEvent::Init {
            session_id: "s".into(),
            self_started,
        }
    }

    fn started(id: &str, tu: Option<&str>, kind: TaskKind, bg: bool) -> StreamEvent {
        StreamEvent::Task(TaskEvent::Started {
            task_id: id.into(),
            tool_use_id: tu.map(Into::into),
            kind,
            description: format!("desc {id}"),
            background: bg,
        })
    }

    fn ended(id: &str, status: TaskStatus) -> StreamEvent {
        StreamEvent::Task(TaskEvent::Ended {
            task_id: id.into(),
            status,
            summary: None,
        })
    }

    fn sub_step(parent: &str, tool: &str) -> StreamEvent {
        StreamEvent::SubagentStep {
            parent: parent.into(),
            tool: tool.into(),
            input: json!({"file_path": "b.rs"}),
        }
    }

    #[test]
    fn turn_is_set_on_init_and_cleared_on_result_or_exit() {
        let mut p = Progress::default();
        assert!(apply(&mut p, &init(false), t(0)));
        let turn = p.turn.clone().unwrap();
        assert_eq!(
            (turn.by, turn.started_at, turn.reason),
            ("user", t(0), None)
        );
        let result = StreamEvent::TurnResult {
            subtype: "success".into(),
            is_error: false,
            result: None,
        };
        assert!(apply(&mut p, &result, t(1)));
        assert!(p.turn.is_none());
        apply(&mut p, &init(true), t(2));
        let turn = p.turn.clone().unwrap();
        assert_eq!(turn.by, "claude");
        assert_eq!(turn.reason, None, "no task ended: no reason");
        assert!(apply(&mut p, &StreamEvent::Exited { code: None }, t(3)));
        assert!(p.turn.is_none());
    }

    #[test]
    fn a_self_started_turn_after_a_task_end_has_the_reason() {
        let mut p = Progress::default();
        let result = StreamEvent::TurnResult {
            subtype: "success".into(),
            is_error: false,
            result: None,
        };
        apply(&mut p, &init(false), t(0));
        apply(&mut p, &started("b1", None, TaskKind::Command, true), t(1));
        apply(&mut p, &result, t(2));
        apply(&mut p, &ended("b1", TaskStatus::Completed), t(3));
        apply(&mut p, &init(true), t(4));
        let turn = p.turn.clone().unwrap();
        assert_eq!(turn.reason.as_deref(), Some(SELF_STARTED_REASON));
        // The end was consumed: the next self-started turn has none.
        apply(&mut p, &result, t(5));
        apply(&mut p, &init(true), t(6));
        assert_eq!(p.turn.as_ref().unwrap().reason, None);
        // A user turn never has one.
        apply(&mut p, &ended("zz", TaskStatus::Completed), t(7));
        apply(&mut p, &result, t(8));
        apply(&mut p, &init(false), t(9));
        assert_eq!(p.turn.as_ref().unwrap().reason, None);
    }

    #[test]
    fn a_finished_subagent_stays_until_the_turn_after_its_end_ends() {
        let mut p = Progress::default();
        let result = StreamEvent::TurnResult {
            subtype: "success".into(),
            is_error: false,
            result: None,
        };
        apply(&mut p, &init(false), t(0));
        apply(
            &mut p,
            &started("a1", Some("tuA"), TaskKind::Agent, true),
            t(1),
        );
        apply(&mut p, &result, t(2));
        apply(&mut p, &ended("a1", TaskStatus::Completed), t(3));
        // The self-started follow-up turn still shows it, however long it runs.
        apply(&mut p, &init(true), t(4));
        assert!(!tick(&mut p, t(400)));
        assert_eq!(p.subagents[0].state, "completed");
        apply(&mut p, &result, t(401));
        assert!(p.subagents.is_empty());
        // A foreground one ending mid-turn goes at the end of the next turn.
        apply(&mut p, &init(false), t(402));
        apply(
            &mut p,
            &started("a2", Some("tuB"), TaskKind::Agent, false),
            t(403),
        );
        apply(&mut p, &ended("a2", TaskStatus::Completed), t(404));
        apply(&mut p, &result, t(405));
        assert_eq!(p.subagents.len(), 1);
        apply(&mut p, &init(false), t(406));
        apply(&mut p, &result, t(407));
        assert!(p.subagents.is_empty());
    }

    #[test]
    fn subagent_activity_is_a_sign_of_life_only() {
        let mut p = Progress::default();
        apply(&mut p, &init(false), t(0));
        let act = StreamEvent::SubagentActivity {
            parent: "tuA".into(),
        };
        assert!(apply(&mut p, &act, t(30)));
        let turn = p.turn.as_ref().unwrap();
        assert_eq!((turn.last_signal_at, turn.step.clone()), (t(30), None));
        assert!(p.subagents.is_empty());
    }

    #[test]
    fn an_agent_tool_names_the_step_from_its_start() {
        let mut p = Progress::default();
        apply(&mut p, &init(false), t(0));
        let tool = StreamEvent::ToolStarted {
            tool_use_id: "u".into(),
            tool: "Agent".into(),
            input: json!({"description": "Review the diff", "prompt": "x"}),
        };
        apply(&mut p, &tool, t(1));
        assert_eq!(
            p.turn.as_ref().unwrap().step.as_deref(),
            Some("Subagente: Review the diff")
        );
        // A task start sets it when there is no step yet.
        let mut q = Progress::default();
        apply(&mut q, &init(false), t(0));
        apply(
            &mut q,
            &started("a1", Some("tuA"), TaskKind::Agent, false),
            t(1),
        );
        assert_eq!(
            q.turn.as_ref().unwrap().step.as_deref(),
            Some("Subagente: desc a1")
        );
    }

    #[test]
    fn step_follows_tools_and_signal_is_bumped_by_any_event() {
        let mut p = Progress::default();
        apply(&mut p, &init(false), t(0));
        let tool = StreamEvent::ToolStarted {
            tool_use_id: "u".into(),
            tool: "Read".into(),
            input: json!({"file_path": "a.rs"}),
        };
        assert!(apply(&mut p, &tool, t(1)));
        assert_eq!(
            p.turn.as_ref().unwrap().step.as_deref(),
            Some("Leyendo a.rs")
        );
        let text = StreamEvent::TextDelta {
            message: "m".into(),
            text: "hi".into(),
        };
        assert!(apply(&mut p, &text, t(5)));
        let turn = p.turn.as_ref().unwrap();
        assert_eq!(turn.step.as_deref(), Some("Leyendo a.rs"));
        assert_eq!(turn.last_signal_at, t(5));
        // Nothing changes without a turn.
        let mut idle = Progress::default();
        assert!(!apply(&mut idle, &text, t(6)));
    }

    #[test]
    fn subagents_keep_steps_apart_and_prefer_progress_description() {
        let mut p = Progress::default();
        apply(&mut p, &init(false), t(0));
        apply(
            &mut p,
            &started("a1", Some("tuA"), TaskKind::Agent, false),
            t(1),
        );
        apply(
            &mut p,
            &started("a2", Some("tuB"), TaskKind::Agent, false),
            t(1),
        );
        assert!(p.background.is_empty());
        apply(&mut p, &sub_step("tuA", "Read"), t(2));
        apply(&mut p, &sub_step("tuA", "Read"), t(3));
        apply(&mut p, &sub_step("tuB", "Read"), t(3));
        assert_eq!(p.subagents[0].steps, 2);
        assert_eq!(p.subagents[1].steps, 1);
        assert_eq!(
            p.turn.as_ref().unwrap().step.as_deref(),
            Some("Subagente: desc a2 · Leyendo b.rs")
        );
        let progress = StreamEvent::Task(TaskEvent::Progress {
            task_id: "a1".into(),
            tool_use_id: Some("tuA".into()),
            description: Some("Reading files".into()),
            last_tool: None,
            tool_uses: Some(7),
        });
        apply(&mut p, &progress, t(4));
        assert_eq!(p.subagents[0].steps, 7);
        assert_eq!(
            p.turn.as_ref().unwrap().step.as_deref(),
            Some("Subagente: desc a1 · Reading files")
        );
        apply(&mut p, &sub_step("tuA", "Read"), t(5));
        assert_eq!(p.subagents[0].steps, 7, "stream count wins over counting");
        apply(&mut p, &ended("a1", TaskStatus::Failed), t(6));
        assert_eq!(p.subagents[0].state, "failed");
        assert_eq!(p.subagents[0].ended_at, Some(t(6)));
    }

    #[test]
    fn unknown_parent_creates_a_subagent_and_recent_is_capped() {
        let mut p = Progress::default();
        for i in 0..60 {
            apply(&mut p, &sub_step("tuX", "Read"), t(i));
        }
        let s = &p.subagents[0];
        assert_eq!((s.steps, s.recent.len()), (60, 50));
        assert_eq!(s.recent[0].at, t(10));
    }

    #[test]
    fn running_snapshot_is_authoritative_for_background_rows() {
        let mut p = Progress::default();
        apply(&mut p, &init(false), t(0));
        apply(
            &mut p,
            &started("b1", Some("tu1"), TaskKind::Command, true),
            t(1),
        );
        apply(&mut p, &started("b2", None, TaskKind::Command, true), t(1));
        assert!(p.has_background());
        let snap = |ids: &[&str]| {
            StreamEvent::Task(TaskEvent::Running {
                task_ids: ids.iter().map(|s| s.to_string()).collect(),
                described: ids
                    .iter()
                    .map(|s| (s.to_string(), TaskKind::Command, "d".to_string()))
                    .collect(),
            })
        };
        assert!(apply(&mut p, &snap(&["b1", "b3"]), t(2)));
        let by_id = |p: &Progress, id: &str| {
            p.background
                .iter()
                .find(|b| b.task_id == id)
                .cloned()
                .unwrap()
        };
        assert_eq!(by_id(&p, "b2").state, "completed");
        assert_eq!(by_id(&p, "b2").ended_at, Some(t(2)));
        assert_eq!(by_id(&p, "b3").started_at, None);
        assert_eq!(by_id(&p, "b1").started_at, Some(t(1)));
        // An Ended state is not overwritten by the snapshot.
        apply(&mut p, &ended("b1", TaskStatus::Failed), t(3));
        apply(&mut p, &snap(&["b3"]), t(4));
        assert_eq!(by_id(&p, "b1").state, "failed");
        // Same snapshot again, between turns: nothing changes.
        p.turn = None;
        let mut idle = p.clone();
        assert!(!apply(&mut idle, &snap(&["b3"]), t(5)));
    }

    #[test]
    fn final_rows_go_on_next_init_or_after_sixty_seconds() {
        let mut p = Progress::default();
        apply(&mut p, &started("b1", None, TaskKind::Command, true), t(0));
        apply(&mut p, &started("b2", None, TaskKind::Command, true), t(0));
        apply(&mut p, &ended("b1", TaskStatus::Completed), t(10));
        assert!(!tick(&mut p, t(69)));
        assert!(tick(&mut p, t(70)));
        assert_eq!(p.background.len(), 1);
        apply(&mut p, &ended("b2", TaskStatus::Completed), t(80));
        apply(&mut p, &init(true), t(81));
        assert!(p.background.is_empty());
    }

    #[test]
    fn exit_stops_running_work() {
        let mut p = Progress::default();
        apply(&mut p, &started("b1", None, TaskKind::Command, true), t(0));
        apply(
            &mut p,
            &started("a1", Some("tuA"), TaskKind::Agent, true),
            t(0),
        );
        apply(&mut p, &ended("b1", TaskStatus::Completed), t(1));
        apply(&mut p, &StreamEvent::Exited { code: Some(0) }, t(2));
        assert_eq!(p.background[0].state, "completed");
        assert_eq!(p.background[1].state, "stopped");
        assert_eq!(p.subagents[0].state, "stopped");
        assert!(!p.has_background());
    }

    #[test]
    fn serialises_as_the_contract() {
        let mut p = Progress::default();
        apply(&mut p, &init(false), t(0));
        apply(
            &mut p,
            &started("b1", Some("tu1"), TaskKind::Command, true),
            t(1),
        );
        apply(&mut p, &sub_step("tuA", "Read"), t(2));
        let v = serde_json::to_value(&p).unwrap();
        assert_eq!(v["turn"]["by"], "user");
        assert!(v["turn"]["started_at"].as_str().unwrap().ends_with('Z'));
        assert_eq!(v["background"][0]["kind"], "command");
        assert_eq!(v["background"][0]["ended_at"], Value::Null);
        assert_eq!(v["subagents"][0]["recent"][0]["summary"], "Leyendo b.rs");
        assert!(v.get("task_agents").is_none());
    }

    #[tokio::test]
    async fn replays_the_background_fixture() {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../delilah-agents/tests/fixtures/stream-background.jsonl"
        );
        let mut cmd = Command::new("cat");
        cmd.arg(path);
        let (session, _rx) = StreamSession::spawn(cmd).unwrap();
        let mut events = session.take_events().unwrap();
        let mut p = Progress::default();
        let (mut bys, mut n, mut max_sub, mut saw_bg) = (vec![], 0, 0, false);
        while let Some(mut e) = events.recv().await {
            // `cat` writes no user line, so the first turn reads as self-started: it was the user's.
            if let StreamEvent::Init { self_started, .. } = &mut e
                && n == 0
            {
                *self_started = false;
            }
            n += 1;
            apply(&mut p, &e, t(n));
            if let Some(turn) = &p.turn
                && matches!(e, StreamEvent::Init { .. })
            {
                bys.push(turn.by);
            }
            max_sub = max_sub.max(p.subagents.len());
            saw_bg |= p.has_background();
            if matches!(e, StreamEvent::Exited { .. }) {
                break;
            }
        }
        assert_eq!(bys, vec!["user", "claude", "claude"]);
        assert!(max_sub >= 2, "two parallel sub-agents");
        assert!(saw_bg);
        assert!(p.turn.is_none());
        assert!(!p.has_background());
        assert!(p.subagents.iter().all(|s| s.state != "running"));
    }
}
