//! In-memory history of recent activity events per agent session (not persisted).

use std::collections::{HashMap, HashSet, VecDeque};

use chrono::{DateTime, Utc};
use delilah_agents::claude::{Action, ActivityEvent, AgentRef, shorten};
use serde::Serialize;

const MAX_ENTRIES: usize = 200;
const MAX_LOGS: usize = 64;

/// Caps every free-text field of an event at 40 chars (FR-023), as `activity.rs` does.
fn capped(event: ActivityEvent) -> ActivityEvent {
    let agent = |a: AgentRef| AgentRef {
        id: shorten(&a.id),
        role: shorten(&a.role),
    };
    match event {
        ActivityEvent::ToolStarted { agent: a, mut action } => {
            // The id pairs with the capped `ToolEnded` id, so it is cut the same way.
            action.id = shorten(&action.id);
            action.tool = shorten(&action.tool);
            ActivityEvent::ToolStarted { agent: a.map(agent), action }
        }
        ActivityEvent::ToolEnded { agent: a, id, failed } => ActivityEvent::ToolEnded {
            agent: a.map(agent),
            id: shorten(&id),
            failed,
        },
        ActivityEvent::HelperStarted { agent: a } => {
            ActivityEvent::HelperStarted { agent: agent(a) }
        }
        ActivityEvent::HelperEnded { id } => ActivityEvent::HelperEnded { id: shorten(&id) },
        ActivityEvent::TurnOver => ActivityEvent::TurnOver,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub seq: u64,
    pub at: DateTime<Utc>,
    pub event: ActivityEvent,
}

#[derive(Debug, Clone)]
pub struct SessionLog {
    pub session: String,
    pub owner: String,
    pub entries: VecDeque<Entry>,
    pub dropped: bool,
    pub last_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct ActivityLog {
    sessions: HashMap<String, SessionLog>,
    next_seq: u64,
    pub started_at: DateTime<Utc>,
}

/// JSON shape of one entry (contracts/api.md). Built explicitly: the adapter's
/// `ActivityEvent` is tagged kebab-case, the API uses snake_case.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum EntryKind {
    ToolStarted {
        agent: Option<AgentRef>,
        action: Action,
    },
    ToolEnded {
        agent: Option<AgentRef>,
        id: String,
        failed: bool,
    },
    HelperStarted {
        agent: AgentRef,
    },
    HelperEnded {
        id: String,
    },
    TurnOver,
}

#[derive(Debug, Clone, Serialize)]
pub struct EntryView {
    pub seq: u64,
    pub at: DateTime<Utc>,
    #[serde(flatten)]
    pub kind: EntryKind,
}

#[derive(Debug, Clone, Serialize)]
pub struct SessionLogView {
    pub session: String,
    pub live: bool,
    pub dropped: bool,
    pub entries: Vec<EntryView>,
}

impl From<&Entry> for EntryView {
    fn from(e: &Entry) -> Self {
        let kind = match &e.event {
            ActivityEvent::ToolStarted { agent, action } => EntryKind::ToolStarted {
                agent: agent.clone(),
                action: action.clone(),
            },
            ActivityEvent::ToolEnded { agent, id, failed } => EntryKind::ToolEnded {
                agent: agent.clone(),
                id: id.clone(),
                failed: *failed,
            },
            ActivityEvent::HelperStarted { agent } => EntryKind::HelperStarted {
                agent: agent.clone(),
            },
            ActivityEvent::HelperEnded { id } => EntryKind::HelperEnded { id: id.clone() },
            ActivityEvent::TurnOver => EntryKind::TurnOver,
        };
        EntryView {
            seq: e.seq,
            at: e.at,
            kind,
        }
    }
}

impl ActivityLog {
    pub fn new(now: DateTime<Utc>) -> Self {
        Self {
            sessions: HashMap::new(),
            next_seq: 1,
            started_at: now,
        }
    }

    /// Appends an event to the session's log. `live_ids` is a snapshot of live sessions,
    /// used so eviction never drops the log of a running session.
    pub fn record(
        &mut self,
        session: &str,
        owner: &str,
        at: DateTime<Utc>,
        event: ActivityEvent,
        live_ids: &HashSet<String>,
    ) {
        if !self.sessions.contains_key(session) && self.sessions.len() >= MAX_LOGS {
            let victim = self
                .sessions
                .values()
                .filter(|l| !live_ids.contains(&l.session))
                .min_by_key(|l| l.last_at)
                .map(|l| l.session.clone());
            if let Some(v) = victim {
                self.sessions.remove(&v);
            }
        }
        let seq = self.next_seq;
        self.next_seq += 1;
        let log = self
            .sessions
            .entry(session.to_string())
            .or_insert_with(|| SessionLog {
                session: session.to_string(),
                owner: owner.to_string(),
                entries: VecDeque::new(),
                dropped: false,
                last_at: at,
            });
        log.owner = owner.to_string();
        log.last_at = at;
        if log.entries.len() >= MAX_ENTRIES {
            log.entries.pop_front();
            log.dropped = true;
        }
        log.entries.push_back(Entry {
            seq,
            at,
            event: capped(event),
        });
    }

    /// Logs of `owner` with entries `seq > after`, ordered by their oldest kept entry.
    /// Every log of the owner is returned (possibly with no entries), so a client
    /// polling with `after` still sees `live` flip when a session ends.
    pub fn for_owner(
        &self,
        owner: &str,
        after: Option<u64>,
        live_ids: &HashSet<String>,
    ) -> Vec<SessionLogView> {
        let mut logs: Vec<&SessionLog> = self
            .sessions
            .values()
            .filter(|l| l.owner == owner)
            .collect();
        logs.sort_by_key(|l| l.entries.front().map_or(u64::MAX, |e| e.seq));
        logs.into_iter()
            .map(|l| {
                let entries: Vec<EntryView> = l
                    .entries
                    .iter()
                    .filter(|e| after.is_none_or(|a| e.seq > a))
                    .map(EntryView::from)
                    .collect();
                SessionLogView {
                    session: l.session.clone(),
                    live: live_ids.contains(&l.session),
                    dropped: l.dropped,
                    entries,
                }
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn t(s: i64) -> DateTime<Utc> {
        Utc.timestamp_opt(1_800_000_000 + s, 0).unwrap()
    }

    fn live(ids: &[&str]) -> HashSet<String> {
        ids.iter().map(|s| s.to_string()).collect()
    }

    fn ended(id: &str) -> ActivityEvent {
        ActivityEvent::ToolEnded {
            agent: None,
            id: id.into(),
            failed: false,
        }
    }

    #[test]
    fn caps_at_200_and_flags_dropped() {
        let mut log = ActivityLog::new(t(0));
        let none = live(&[]);
        for i in 0..201 {
            log.record("s", "T-1", t(i), ended(&format!("t{i}")), &none);
        }
        let v = log.for_owner("T-1", None, &none);
        assert_eq!(v[0].entries.len(), 200);
        assert!(v[0].dropped);
        assert_eq!(v[0].entries[0].seq, 2);
    }

    #[test]
    fn seq_increases_across_sessions() {
        let mut log = ActivityLog::new(t(0));
        let none = live(&[]);
        log.record("a", "T-1", t(1), ActivityEvent::TurnOver, &none);
        log.record("b", "T-1", t(2), ActivityEvent::TurnOver, &none);
        log.record("a", "T-1", t(3), ActivityEvent::TurnOver, &none);
        let v = log.for_owner("T-1", None, &none);
        let a: Vec<u64> = v[0].entries.iter().map(|e| e.seq).collect();
        assert_eq!(v[0].session, "a");
        assert_eq!(a, vec![1, 3]);
        assert_eq!(v[1].entries[0].seq, 2);
    }

    #[test]
    fn eviction_drops_oldest_non_live() {
        let mut log = ActivityLog::new(t(0));
        let l = live(&["s0"]);
        for i in 0..64 {
            log.record(&format!("s{i}"), "T-1", t(i), ActivityEvent::TurnOver, &l);
        }
        log.record("new", "T-1", t(100), ActivityEvent::TurnOver, &l);
        assert_eq!(log.sessions.len(), 64);
        assert!(log.sessions.contains_key("s0"));
        assert!(!log.sessions.contains_key("s1"));
        assert!(log.sessions.contains_key("new"));
    }

    #[test]
    fn grows_past_64_when_all_live() {
        let mut log = ActivityLog::new(t(0));
        let all: HashSet<String> = (0..65).map(|i| format!("s{i}")).collect();
        for i in 0..65 {
            log.record(&format!("s{i}"), "T-1", t(i), ActivityEvent::TurnOver, &all);
        }
        assert_eq!(log.sessions.len(), 65);
    }

    #[test]
    fn filters_by_owner_and_after() {
        let mut log = ActivityLog::new(t(0));
        let l = live(&["a"]);
        log.record("a", "T-1", t(1), ActivityEvent::TurnOver, &l);
        log.record("a", "T-1", t(2), ActivityEvent::TurnOver, &l);
        log.record("b", "T-2", t(3), ActivityEvent::TurnOver, &l);
        assert!(log.for_owner("nope", None, &l).is_empty());
        let v = log.for_owner("T-1", Some(1), &l);
        assert_eq!(v.len(), 1);
        assert!(v[0].live);
        assert_eq!(v[0].entries.len(), 1);
        assert_eq!(v[0].entries[0].seq, 2);
        let v = log.for_owner("T-1", Some(2), &l);
        assert_eq!(v.len(), 1);
        assert!(v[0].entries.is_empty());
    }

    #[test]
    fn entry_view_json_matches_contract() {
        let mut log = ActivityLog::new(t(0));
        let none = live(&[]);
        log.record("s", "T-1", t(1), ended("toolu_1"), &none);
        log.record(
            "s",
            "T-1",
            t(2),
            ActivityEvent::HelperStarted {
                agent: AgentRef {
                    id: "a1b2".into(),
                    role: "investigar".into(),
                },
            },
            &none,
        );
        log.record("s", "T-1", t(3), ActivityEvent::TurnOver, &none);
        let v = serde_json::to_value(log.for_owner("T-1", None, &none)).unwrap();
        let e = &v[0]["entries"];
        assert_eq!(e[0]["type"], "tool_ended");
        assert!(e[0]["agent"].is_null() && e[0].as_object().unwrap().contains_key("agent"));
        assert_eq!(e[0]["id"], "toolu_1");
        assert_eq!(e[0]["failed"], false);
        assert_eq!(e[1]["type"], "helper_started");
        assert_eq!(e[1]["agent"]["role"], "investigar");
        assert_eq!(e[2]["type"], "turn_over");
        assert_eq!(v[0]["live"], false);
        assert_eq!(v[0]["dropped"], false);
    }

    #[test]
    fn tool_started_json_matches_contract() {
        use delilah_agents::claude::ActionKind;
        let mut log = ActivityLog::new(t(0));
        let none = live(&[]);
        let event = ActivityEvent::ToolStarted {
            agent: Some(AgentRef { id: "a1".into(), role: "r".into() }),
            action: Action {
                id: "toolu_1".into(),
                kind: ActionKind::Edit,
                tool: "Edit".into(),
                target: Some("src/a.rs".into()),
                background: false,
                since: t(1),
            },
        };
        log.record("s", "T-1", t(1), event, &none);
        let v = serde_json::to_value(log.for_owner("T-1", None, &none)).unwrap();
        let e = &v[0]["entries"][0];
        assert_eq!(e["type"], "tool_started");
        assert_eq!(e["seq"], 1);
        assert_eq!(e["agent"], serde_json::json!({"id": "a1", "role": "r"}));
        let a = &e["action"];
        assert_eq!(a["id"], "toolu_1");
        assert_eq!(a["kind"], "edit");
        assert_eq!(a["tool"], "Edit");
        assert_eq!(a["target"], "src/a.rs");
        assert_eq!(a["background"], false);
        assert_eq!(a["since"], serde_json::to_value(t(1)).unwrap());
        assert_eq!(a.as_object().unwrap().len(), 6);
    }

    #[test]
    fn stored_fields_are_capped_at_40_chars() {
        use delilah_agents::claude::ActionKind;
        let long = "x".repeat(100);
        let mut log = ActivityLog::new(t(0));
        let none = live(&[]);
        let agent = || AgentRef { id: long.clone(), role: long.clone() };
        log.record(
            "s",
            "T-1",
            t(1),
            ActivityEvent::ToolStarted {
                agent: Some(agent()),
                action: Action {
                    id: long.clone(),
                    kind: ActionKind::Other,
                    tool: long.clone(),
                    target: None,
                    background: false,
                    since: t(1),
                },
            },
            &none,
        );
        log.record("s", "T-1", t(2), ActivityEvent::ToolEnded { agent: Some(agent()), id: long.clone(), failed: false }, &none);
        log.record("s", "T-1", t(3), ActivityEvent::HelperStarted { agent: agent() }, &none);
        log.record("s", "T-1", t(4), ActivityEvent::HelperEnded { id: long.clone() }, &none);
        let v = serde_json::to_value(log.for_owner("T-1", None, &none)).unwrap();
        let mut strings = Vec::new();
        for e in v[0]["entries"].as_array().unwrap() {
            for key in ["id", "agent", "action"] {
                match &e[key] {
                    serde_json::Value::String(s) => strings.push(s.clone()),
                    serde_json::Value::Object(o) => strings.extend(
                        o.values().filter_map(|x| x.as_str()).map(String::from),
                    ),
                    _ => {}
                }
            }
        }
        assert!(strings.len() >= 9);
        assert!(strings.iter().all(|s| s.chars().count() <= 40), "{strings:?}");
    }
}
