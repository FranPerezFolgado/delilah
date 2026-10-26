//! What a session's agents are doing right now: the main agent's current tool
//! call and the helpers (sub-agents) with theirs. Pure state, no locking or I/O.

use chrono::{DateTime, Utc};
use delilah_agents::claude::{Action, ActivityEvent, shorten};
use serde::Serialize;

/// Most helpers kept per session; further new ones are ignored.
const MAX_HELPERS: usize = 64;

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct Activity {
    pub action: Option<Action>,
    pub helpers: Vec<Helper>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Helper {
    pub id: String,
    pub role: String,
    pub action: Option<Action>,
    pub since: DateTime<Utc>,
}

impl Activity {
    /// Applies one event; true when the state changed.
    pub fn apply(&mut self, e: ActivityEvent) -> bool {
        let before = self.clone();
        let e = match e {
            ActivityEvent::ToolStarted { agent, mut action } => {
                action.tool = shorten(&action.tool);
                ActivityEvent::ToolStarted { agent, action }
            }
            e => e,
        };
        match e {
            ActivityEvent::ToolStarted {
                agent: None,
                action,
            } => self.action = Some(action),
            ActivityEvent::ToolStarted {
                agent: Some(a),
                action,
            } => {
                if let Some(h) = self.upsert(&a.id, &a.role) {
                    h.action = Some(action);
                }
            }
            ActivityEvent::ToolEnded {
                agent: None, id, ..
            } => {
                if self.action.as_ref().is_some_and(|a| a.id == id) {
                    self.action = None;
                }
            }
            ActivityEvent::ToolEnded {
                agent: Some(a), id, ..
            } => {
                // Only an existing helper: a late end after SubagentStop must not revive it.
                let key = shorten(&a.id);
                if let Some(h) = self.helpers.iter_mut().find(|h| h.id == key)
                    && h.action.as_ref().is_some_and(|x| x.id == id)
                {
                    h.action = None;
                }
            }
            ActivityEvent::HelperStarted { agent } => {
                self.upsert(&agent.id, &agent.role);
            }
            ActivityEvent::HelperEnded { id } => {
                let id = shorten(&id);
                self.helpers.retain(|h| h.id != id)
            }
            ActivityEvent::TurnOver => {
                self.action = None;
                self.helpers.clear();
            }
        }
        *self != before
    }

    /// Empties the state; true when there was something to empty.
    pub fn clear(&mut self) -> bool {
        let had = !self.is_empty();
        self.action = None;
        self.helpers.clear();
        had
    }

    /// The helper with this id, created at the end if new; an existing role is
    /// kept when the new one is empty.
    /// None when it is new and the cap is reached.
    fn upsert(&mut self, id: &str, role: &str) -> Option<&mut Helper> {
        let id = shorten(id);
        let role = shorten(role);
        let i = match self.helpers.iter().position(|h| h.id == id) {
            Some(i) => {
                if !role.is_empty() {
                    self.helpers[i].role = role;
                }
                i
            }
            None => {
                if self.helpers.len() >= MAX_HELPERS {
                    return None;
                }
                self.helpers.push(Helper {
                    id,
                    role,
                    action: None,
                    since: Utc::now(),
                });
                self.helpers.len() - 1
            }
        };
        Some(&mut self.helpers[i])
    }

    pub fn is_empty(&self) -> bool {
        self.action.is_none() && self.helpers.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use delilah_agents::claude::{ActionKind, AgentRef};

    fn action(id: &str) -> Action {
        Action {
            id: id.into(),
            kind: ActionKind::Edit,
            tool: "Edit".into(),
            target: Some("a.rs".into()),
            background: false,
            since: Utc::now(),
        }
    }
    fn agent(id: &str, role: &str) -> AgentRef {
        AgentRef {
            id: id.into(),
            role: role.into(),
        }
    }
    fn start(agent: Option<AgentRef>, id: &str) -> ActivityEvent {
        ActivityEvent::ToolStarted {
            agent,
            action: action(id),
        }
    }
    fn end(agent: Option<AgentRef>, id: &str) -> ActivityEvent {
        ActivityEvent::ToolEnded {
            agent,
            id: id.into(),
            failed: false,
        }
    }

    #[test]
    fn main_tool_start_and_end() {
        let mut a = Activity::default();
        assert!(a.apply(start(None, "t1")));
        assert_eq!(a.action.as_ref().unwrap().id, "t1");
        assert!(!a.apply(end(None, "other")));
        assert!(a.action.is_some());
        assert!(a.apply(end(None, "t1")));
        assert!(a.action.is_none());
    }

    #[test]
    fn identical_start_is_no_change_and_second_replaces() {
        let mut a = Activity::default();
        let act = action("t1");
        let ev = ActivityEvent::ToolStarted {
            agent: None,
            action: act.clone(),
        };
        assert!(a.apply(ev.clone()));
        assert!(!a.apply(ev));
        assert!(a.apply(start(None, "t2")));
        assert_eq!(a.action.as_ref().unwrap().id, "t2");
    }

    #[test]
    fn turn_over_clears_everything() {
        let mut a = Activity::default();
        a.apply(start(None, "t1"));
        a.apply(start(Some(agent("h1", "Explore")), "t2"));
        assert!(a.apply(ActivityEvent::TurnOver));
        assert!(a.is_empty());
        assert!(!a.apply(ActivityEvent::TurnOver));
    }

    #[test]
    fn tool_for_unknown_helper_upserts_it() {
        let mut a = Activity::default();
        assert!(a.apply(start(Some(agent("h1", "Explore")), "t1")));
        assert_eq!(a.helpers.len(), 1);
        assert_eq!(a.helpers[0].role, "Explore");
        assert_eq!(a.helpers[0].action.as_ref().unwrap().id, "t1");
        assert!(a.action.is_none());
        assert!(!a.apply(end(Some(agent("h1", "Explore")), "zz")));
        assert!(a.apply(end(Some(agent("h1", "")), "t1")));
        assert!(a.helpers[0].action.is_none());
        assert_eq!(a.helpers[0].role, "Explore");
    }

    #[test]
    fn helpers_are_capped_and_fields_shortened() {
        let mut a = Activity::default();
        for i in 0..64 {
            assert!(a.apply(ActivityEvent::HelperStarted {
                agent: agent(&format!("h{i}"), "r")
            }));
        }
        assert!(!a.apply(ActivityEvent::HelperStarted {
            agent: agent("extra", "r")
        }));
        assert!(!a.apply(start(Some(agent("extra", "r")), "t")));
        assert_eq!(a.helpers.len(), 64);

        let mut b = Activity::default();
        let long = "é".repeat(100);
        b.apply(ActivityEvent::HelperStarted {
            agent: agent(&long, &long),
        });
        assert_eq!(b.helpers[0].id.chars().count(), 40);
        assert_eq!(b.helpers[0].role.chars().count(), 40);
        let mut act = action("t1");
        act.tool = long.clone();
        b.apply(ActivityEvent::ToolStarted {
            agent: Some(agent(&long, "")),
            action: act,
        });
        assert_eq!(b.helpers.len(), 1);
        assert_eq!(
            b.helpers[0].action.as_ref().unwrap().tool.chars().count(),
            40
        );
        assert!(b.apply(end(Some(agent(&long, "")), "t1")));
    }

    #[test]
    fn tool_end_for_unknown_helper_does_not_create_it() {
        let mut a = Activity::default();
        assert!(!a.apply(end(Some(agent("ghost", "r")), "t1")));
        assert!(a.helpers.is_empty());
    }

    #[test]
    fn helper_started_twice_is_one_and_ended_removes() {
        let mut a = Activity::default();
        assert!(a.apply(ActivityEvent::HelperStarted {
            agent: agent("h1", "Plan")
        }));
        assert!(!a.apply(ActivityEvent::HelperStarted {
            agent: agent("h1", "Plan")
        }));
        assert_eq!(a.helpers.len(), 1);
        assert!(a.apply(ActivityEvent::HelperEnded { id: "h1".into() }));
        assert!(!a.apply(ActivityEvent::HelperEnded { id: "h1".into() }));
        assert!(a.helpers.is_empty());
    }

    #[test]
    fn helpers_keep_start_order() {
        let mut a = Activity::default();
        for id in ["h1", "h2", "h3"] {
            a.apply(ActivityEvent::HelperStarted {
                agent: agent(id, "r"),
            });
        }
        a.apply(start(Some(agent("h1", "r")), "t"));
        let ids: Vec<_> = a.helpers.iter().map(|h| h.id.as_str()).collect();
        assert_eq!(ids, ["h1", "h2", "h3"]);
    }

    #[test]
    fn clear_reports_whether_anything_was_there() {
        let mut a = Activity::default();
        assert!(!a.clear());
        a.apply(start(None, "t1"));
        assert!(a.clear());
        assert!(!a.clear());
    }

    #[test]
    fn serializes_as_contract_shape() {
        let mut a = Activity::default();
        a.apply(start(None, "t1"));
        a.apply(ActivityEvent::HelperStarted {
            agent: agent("h1", "Plan"),
        });
        let v = serde_json::to_value(&a).unwrap();
        assert_eq!(v["action"]["kind"], "edit");
        assert!(v["action"]["since"].is_string());
        assert_eq!(v["helpers"][0]["id"], "h1");
        assert_eq!(v["helpers"][0]["role"], "Plan");
        assert!(v["helpers"][0]["action"].is_null());
        assert!(v["helpers"][0]["since"].is_string());
    }
}
