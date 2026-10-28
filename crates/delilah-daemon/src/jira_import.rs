//! Imports the Jira sub-tasks of a parent task as Delilah subtasks (spec 013).
//!
//! `plan_import` is the pure decision table behind the preview and the apply: the
//! caller gathers the facts (store, Jira), this decides what each ticket would become.

// Wired by the preview and apply routes.
#![allow(dead_code)]

use std::collections::{HashMap, HashSet};

use delilah_core::hierarchy::{self, Refusal, TaskKind};
use delilah_core::{Task, TaskKey, TaskStatus};
use delilah_integrations::jira::JiraIssue;
use serde::Serialize;

/// The ticket being imported, as the dialog shows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TicketInfo {
    pub key: String,
    pub title: String,
    pub status: String,
    pub is_subtask: bool,
}

/// What happens to the ticket itself.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum ParentAction {
    Create,
    Convert,
    Existing,
    Refused { code: &'static str, message: String },
}

/// What happens to one Jira sub-task.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum SubtaskAction {
    Create,
    Attach,
    Linked,
    Conflict { reason: &'static str },
}

/// The Delilah task that already has a sub-task's key.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ExistingTask {
    pub key: String,
    pub kind: TaskKind,
    pub status: TaskStatus,
    pub parent_task: Option<String>,
    pub archived: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SubtaskRow {
    pub key: String,
    pub title: String,
    pub status: String,
    pub status_category: String,
    pub assignee: Option<String>,
    pub mine: bool,
    pub task: Option<ExistingTask>,
    #[serde(flatten)]
    pub action: SubtaskAction,
    pub skipped: bool,
    pub selected: bool,
}

/// A Delilah subtask of the parent task that Jira no longer lists under the ticket.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SubtaskGone {
    pub key: String,
    pub title: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ImportPlan {
    pub ticket: TicketInfo,
    pub parent: ParentAction,
    pub subtasks: Vec<SubtaskRow>,
    pub gone: Vec<SubtaskGone>,
}

/// The Delilah task with the ticket's key and what the daemon knows about it.
#[derive(Debug, Clone)]
pub struct ParentFacts {
    pub task: Task,
    pub archived: bool,
    pub has_open_pr: bool,
    pub has_session: bool,
}

/// A Delilah task with a sub-task's key.
#[derive(Debug, Clone)]
pub struct TaskFacts {
    pub task: Task,
    pub archived: bool,
    /// It has run before (a workspace, a session or a PR).
    pub started: bool,
}

/// Code and message for a refusal of the ticket (spec 012's codes).
fn refusal_text(r: Refusal) -> (&'static str, &'static str) {
    match r {
        Refusal::Nesting => ("nesting", "A subtask can't become a parent task"),
        Refusal::HasOpenPr => ("open-pr", "The task has an open pull request"),
        Refusal::HasSession => ("running-session", "The task has a running session"),
        Refusal::InCheckout => ("in-checkout", "The task works in the project's checkout"),
        Refusal::ParentTaskArchived => ("archived", "The task is archived"),
        Refusal::NotAParentTask => ("not-a-parent-task", "The task is not a parent task"),
        Refusal::ProjectMismatch => ("project-mismatch", "The project does not match"),
        _ => ("refused", "The task can't become a parent task"),
    }
}

fn refused(r: Refusal) -> ParentAction {
    let (code, message) = refusal_text(r);
    ParentAction::Refused {
        code,
        message: message.to_string(),
    }
}

fn conflict_reason(r: Refusal) -> &'static str {
    match r {
        Refusal::ProjectMismatch => "other-project",
        Refusal::InCheckout => "in-checkout",
        _ => "started",
    }
}

/// Decides what importing `ticket` would do. Pure: every fact comes in as an argument.
///
/// - `parent`: the Delilah task keyed like the ticket, if any.
/// - `jira_subtasks`: the ticket's sub-tasks in Jira.
/// - `tasks`: the Delilah tasks keyed like those sub-tasks.
/// - `children`: the Delilah subtasks of the parent task (for `gone`).
/// - `skips`: Jira keys the user skipped for this parent task.
/// - `my_keys`: sub-task keys assigned to the current user.
pub fn plan_import(
    ticket: &JiraIssue,
    parent: Option<&ParentFacts>,
    jira_subtasks: &[JiraIssue],
    tasks: &HashMap<String, TaskFacts>,
    children: &[Task],
    skips: &HashSet<String>,
    my_keys: &HashSet<String>,
) -> ImportPlan {
    // The task the sub-tasks would hang from (what it will be, once imported).
    let mut hypothetical = match parent {
        Some(p) => p.task.clone(),
        None => Task::new(TaskKey::new(&ticket.key), &ticket.summary, "project"),
    };
    let parent_archived = parent.is_some_and(|p| p.archived);

    let parent_action = if ticket.subtask {
        refused(Refusal::Nesting)
    } else {
        match parent {
            None => ParentAction::Create,
            Some(p) if p.archived => refused(Refusal::ParentTaskArchived),
            Some(p) if p.task.kind == TaskKind::ParentTask => ParentAction::Existing,
            Some(p) => {
                match hierarchy::check_make_parent_task(&p.task, p.has_open_pr, p.has_session) {
                    Ok(()) => ParentAction::Convert,
                    Err(r) => refused(r),
                }
            }
        }
    };
    hypothetical.kind = TaskKind::ParentTask;

    let subtasks = jira_subtasks
        .iter()
        .map(|j| {
            let facts = tasks.get(&j.key);
            let action = match facts {
                None => SubtaskAction::Create,
                Some(f) => match f.task.kind {
                    TaskKind::ParentTask => SubtaskAction::Conflict {
                        reason: "parent-task",
                    },
                    TaskKind::Subtask => {
                        if f.task.parent_task.as_ref().map(|k| k.as_str())
                            == Some(hypothetical.key.as_str())
                        {
                            SubtaskAction::Linked
                        } else {
                            SubtaskAction::Conflict {
                                reason: "other-parent",
                            }
                        }
                    }
                    TaskKind::Normal => {
                        match hierarchy::check_attach(
                            &f.task,
                            &hypothetical,
                            parent_archived,
                            f.started,
                        ) {
                            Ok(()) => SubtaskAction::Attach,
                            Err(r) => SubtaskAction::Conflict {
                                reason: if f.started {
                                    "started"
                                } else {
                                    conflict_reason(r)
                                },
                            },
                        }
                    }
                },
            };
            let skipped = skips.contains(&j.key);
            let mine = my_keys.contains(&j.key);
            let selected = matches!(action, SubtaskAction::Create | SubtaskAction::Attach)
                && !skipped
                && j.status_category != "done"
                && (j.assignee.is_none() || mine);
            SubtaskRow {
                key: j.key.clone(),
                title: j.summary.clone(),
                status: j.status.clone(),
                status_category: j.status_category.clone(),
                assignee: j.assignee.clone(),
                mine,
                task: facts.map(|f| ExistingTask {
                    key: f.task.key.as_str().to_string(),
                    kind: f.task.kind,
                    status: f.task.status.clone(),
                    parent_task: f.task.parent_task.as_ref().map(|k| k.as_str().to_string()),
                    archived: f.archived,
                }),
                action,
                skipped,
                selected,
            }
        })
        .collect();

    let listed: HashSet<&str> = jira_subtasks.iter().map(|j| j.key.as_str()).collect();
    let gone = children
        .iter()
        .filter(|c| !listed.contains(c.key.as_str()))
        .map(|c| SubtaskGone {
            key: c.key.as_str().to_string(),
            title: c.title.clone(),
        })
        .collect();

    ImportPlan {
        ticket: TicketInfo {
            key: ticket.key.clone(),
            title: ticket.summary.clone(),
            status: ticket.status.clone(),
            is_subtask: ticket.subtask,
        },
        parent: parent_action,
        subtasks,
        gone,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn issue(key: &str, cat: &str, assignee: Option<&str>, subtask: bool) -> JiraIssue {
        serde_json::from_value(serde_json::json!({
            "key": key, "summary": format!("title {key}"), "issue_type": "Task",
            "status": "Some status", "status_category": cat,
            "assignee": assignee, "subtask": subtask,
            "components": [], "description": "", "url": null,
        }))
        .unwrap()
    }

    fn task(key: &str, kind: TaskKind, project: Option<&str>, parent: Option<&str>) -> Task {
        let mut t = Task::new(TaskKey::new(key), format!("t {key}"), "project");
        t.kind = kind;
        t.project = project.map(String::from);
        t.parent_task = parent.map(TaskKey::new);
        t
    }

    fn pf(t: Task) -> ParentFacts {
        ParentFacts {
            task: t,
            archived: false,
            has_open_pr: false,
            has_session: false,
        }
    }

    fn tf(t: Task, started: bool, archived: bool) -> TaskFacts {
        TaskFacts {
            task: t,
            archived,
            started,
        }
    }

    fn parent_action(ticket: &JiraIssue, p: Option<&ParentFacts>) -> ParentAction {
        plan_import(
            ticket,
            p,
            &[],
            &HashMap::new(),
            &[],
            &HashSet::new(),
            &HashSet::new(),
        )
        .parent
    }

    fn code(a: ParentAction) -> &'static str {
        match a {
            ParentAction::Refused { code, .. } => code,
            other => panic!("not refused: {other:?}"),
        }
    }

    #[test]
    fn parent_actions() {
        let t = issue("P-1", "new", None, false);
        assert_eq!(parent_action(&t, None), ParentAction::Create);
        let normal = pf(task("P-1", TaskKind::Normal, None, None));
        assert_eq!(parent_action(&t, Some(&normal)), ParentAction::Convert);
        let existing = pf(task("P-1", TaskKind::ParentTask, None, None));
        assert_eq!(parent_action(&t, Some(&existing)), ParentAction::Existing);
    }

    #[test]
    fn parent_refusals() {
        let t = issue("P-1", "new", None, false);
        let sub = pf(task("P-1", TaskKind::Subtask, None, Some("P-0")));
        assert_eq!(code(parent_action(&t, Some(&sub))), "nesting");
        let mut pr = pf(task("P-1", TaskKind::Normal, None, None));
        pr.has_open_pr = true;
        assert_eq!(code(parent_action(&t, Some(&pr))), "open-pr");
        let mut ses = pf(task("P-1", TaskKind::Normal, None, None));
        ses.has_session = true;
        assert_eq!(code(parent_action(&t, Some(&ses))), "running-session");
        let mut arch = pf(task("P-1", TaskKind::Normal, None, None));
        arch.archived = true;
        assert_eq!(code(parent_action(&t, Some(&arch))), "archived");
        // A ticket that is itself a Jira sub-task never becomes a parent task.
        let jira_sub = issue("P-1", "new", None, true);
        assert_eq!(code(parent_action(&jira_sub, None)), "nesting");
    }

    fn plan_one(
        j: JiraIssue,
        existing: Option<TaskFacts>,
        skipped: bool,
        mine: bool,
    ) -> SubtaskRow {
        let parent = pf(task("P-1", TaskKind::ParentTask, Some("web"), None));
        let ticket = issue("P-1", "new", None, false);
        let mut tasks = HashMap::new();
        if let Some(f) = existing {
            tasks.insert(j.key.clone(), f);
        }
        let skips: HashSet<String> = skipped.then(|| j.key.clone()).into_iter().collect();
        let my: HashSet<String> = mine.then(|| j.key.clone()).into_iter().collect();
        let mut plan = plan_import(
            &ticket,
            Some(&parent),
            std::slice::from_ref(&j),
            &tasks,
            &[],
            &skips,
            &my,
        );
        plan.subtasks.remove(0)
    }

    #[test]
    fn subtask_actions() {
        let j = || issue("P-2", "new", None, true);
        let cases: Vec<(Option<TaskFacts>, SubtaskAction)> = vec![
            (None, SubtaskAction::Create),
            (
                Some(tf(task("P-2", TaskKind::Normal, None, None), false, false)),
                SubtaskAction::Attach,
            ),
            (
                Some(tf(
                    task("P-2", TaskKind::Normal, Some("web"), None),
                    false,
                    false,
                )),
                SubtaskAction::Attach,
            ),
            (
                Some(tf(
                    task("P-2", TaskKind::Subtask, None, Some("P-1")),
                    true,
                    false,
                )),
                SubtaskAction::Linked,
            ),
            (
                Some(tf(
                    task("P-2", TaskKind::Subtask, None, Some("P-1")),
                    true,
                    true,
                )),
                SubtaskAction::Linked,
            ),
            (
                Some(tf(task("P-2", TaskKind::Normal, None, None), true, false)),
                SubtaskAction::Conflict { reason: "started" },
            ),
            (
                Some(tf(
                    task("P-2", TaskKind::Normal, Some("api"), None),
                    false,
                    false,
                )),
                SubtaskAction::Conflict {
                    reason: "other-project",
                },
            ),
            (
                Some(tf(
                    task("P-2", TaskKind::Subtask, None, Some("P-9")),
                    true,
                    false,
                )),
                SubtaskAction::Conflict {
                    reason: "other-parent",
                },
            ),
            (
                Some(tf(
                    task("P-2", TaskKind::ParentTask, None, None),
                    false,
                    false,
                )),
                SubtaskAction::Conflict {
                    reason: "parent-task",
                },
            ),
        ];
        for (existing, want) in cases {
            let row = plan_one(j(), existing.clone(), false, false);
            assert_eq!(row.action, want, "{existing:?}");
            assert_eq!(row.task.is_some(), existing.is_some());
        }
    }

    #[test]
    fn selected_defaults() {
        // (category, assignee, mine, skipped, existing, selected)
        let started = Some(tf(task("P-2", TaskKind::Normal, None, None), true, false));
        let linked = Some(tf(
            task("P-2", TaskKind::Subtask, None, Some("P-1")),
            true,
            false,
        ));
        #[allow(clippy::type_complexity)]
        let cases: Vec<(&str, Option<&str>, bool, bool, Option<TaskFacts>, bool)> = vec![
            ("new", None, false, false, None, true),
            ("indeterminate", Some("Fran"), true, false, None, true),
            ("done", None, false, false, None, false),
            ("new", Some("Ana"), false, false, None, false),
            ("new", None, false, true, None, false),
            ("new", None, false, false, started, false),
            ("new", None, false, false, linked, false),
        ];
        for (cat, who, mine, skipped, existing, want) in cases {
            let row = plan_one(issue("P-2", cat, who, true), existing, skipped, mine);
            assert_eq!(row.selected, want, "{cat} {who:?} {mine} {skipped}");
            assert_eq!(row.skipped, skipped);
            assert_eq!(row.mine, mine);
        }
    }

    #[test]
    fn gone_lists_children_missing_from_jira() {
        let parent = pf(task("P-1", TaskKind::ParentTask, None, None));
        let ticket = issue("P-1", "new", None, false);
        let kids = vec![
            task("P-2", TaskKind::Subtask, None, Some("P-1")),
            task("P-9", TaskKind::Subtask, None, Some("P-1")),
        ];
        let plan = plan_import(
            &ticket,
            Some(&parent),
            &[issue("P-2", "new", None, true)],
            &HashMap::new(),
            &kids,
            &HashSet::new(),
            &HashSet::new(),
        );
        assert_eq!(
            plan.gone,
            vec![SubtaskGone {
                key: "P-9".into(),
                title: "t P-9".into()
            }]
        );
        assert_eq!(plan.ticket.key, "P-1");
    }

    #[test]
    fn attach_when_parent_is_created_or_converted() {
        // The parent task doesn't exist yet: a never-started normal task attaches.
        let ticket = issue("P-1", "new", None, false);
        let mut tasks = HashMap::new();
        tasks.insert(
            "P-2".to_string(),
            tf(task("P-2", TaskKind::Normal, None, None), false, false),
        );
        let plan = plan_import(
            &ticket,
            None,
            &[issue("P-2", "new", None, true)],
            &tasks,
            &[],
            &HashSet::new(),
            &HashSet::new(),
        );
        assert_eq!(plan.parent, ParentAction::Create);
        assert_eq!(plan.subtasks[0].action, SubtaskAction::Attach);
    }

    #[test]
    fn serializes_like_the_contract() {
        let row = plan_one(issue("P-2", "new", None, true), None, false, false);
        let v = serde_json::to_value(&row).unwrap();
        assert_eq!(v["action"], "create");
        let p = serde_json::to_value(ParentAction::Create).unwrap();
        assert_eq!(p, serde_json::json!({ "action": "create" }));
    }
}
