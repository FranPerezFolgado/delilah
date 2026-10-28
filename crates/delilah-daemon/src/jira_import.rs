//! Imports the Jira sub-tasks of a parent task as Delilah subtasks (spec 013).
//!
//! `plan_import` is the pure decision table behind the preview and the apply: the
//! caller gathers the facts (store, Jira), this decides what each ticket would become.

use std::collections::{HashMap, HashSet};

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use delilah_core::hierarchy::{self, Refusal, TaskKind};
use delilah_core::{Task, TaskKey, TaskStatus};
use delilah_integrations::jira::{JiraError, JiraIssue};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::{Row, SqliteConnection};

use crate::api::{ApiError, NewTask, new_task, resolve_new_task};
use crate::events::Topic;
use crate::jira::{self, JiraSettings, SETTINGS_KEY};
use crate::pr_types::{PrState, Snapshot};
use crate::state::AppState;
use crate::store;

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
/// - `project`: the project a parent task created now would take (it only matters when
///   there is no Delilah task for the ticket yet).
#[allow(clippy::too_many_arguments)]
pub fn plan_import(
    ticket: &JiraIssue,
    parent: Option<&ParentFacts>,
    jira_subtasks: &[JiraIssue],
    tasks: &HashMap<String, TaskFacts>,
    children: &[Task],
    skips: &HashSet<String>,
    my_keys: &HashSet<String>,
    project: Option<&str>,
) -> ImportPlan {
    // The task the sub-tasks would hang from (what it will be, once imported).
    let mut hypothetical = match parent {
        Some(p) => p.task.clone(),
        None => {
            let mut t = Task::new(TaskKey::new(&ticket.key), &ticket.summary, "project");
            t.project = project.map(String::from);
            t
        }
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
    // Only Jira-looking keys of the ticket's project can have left Jira; subtasks made by
    // hand (free keys) were never there.
    let project_of = |k: &str| k.rsplit_once('-').map(|(p, _)| p.to_ascii_uppercase());
    let ticket_project = project_of(&ticket.key);
    let gone = children
        .iter()
        .filter(|c| !listed.contains(c.key.as_str()))
        .filter(|c| is_issue_key(c.key.as_str()) && project_of(c.key.as_str()) == ticket_project)
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

// ---------------------------------------------------------------------------
// Preview and apply: gather the facts, then run the pure planner / write it all.
// ---------------------------------------------------------------------------

/// `PROJ-123`: a project key (a letter, then letters, digits or `_`) and a number.
fn is_issue_key(key: &str) -> bool {
    let Some((project, number)) = key.split_once('-') else {
        return false;
    };
    project.starts_with(|c: char| c.is_ascii_alphabetic())
        && project
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_')
        && !number.is_empty()
        && number.chars().all(|c| c.is_ascii_digit())
}

/// A 400/409 with the contract's `{ error: <code>, message }` body.
fn coded(status: StatusCode, code: &str, message: impl Into<String>) -> ApiError {
    let message = message.into();
    let mut e = match status {
        StatusCode::CONFLICT => ApiError::conflict(message.clone()),
        _ => ApiError::bad_request(message.clone()),
    };
    e.refusal = Some(json!({ "error": code, "message": message }));
    e
}

fn check_key(key: &str) -> Result<(), ApiError> {
    if is_issue_key(key) {
        Ok(())
    } else {
        Err(coded(
            StatusCode::BAD_REQUEST,
            "bad-key",
            format!("{key} is not a Jira issue key"),
        ))
    }
}

/// Why Jira could not answer for the ticket.
fn unlinked(e: &JiraError) -> &'static str {
    match e {
        JiraError::NotInSample(_) | JiraError::BadKey(_) | JiraError::Http { status: 404, .. } => {
            "not-found"
        }
        _ => "unreachable",
    }
}

/// Whether a stored task is archived: finished and without a workspace.
fn is_archived(task: &Task, has_workspace: bool) -> bool {
    task.status.is_terminal() && !has_workspace
}

async fn parent_facts(app: &AppState, task: Task) -> Result<ParentFacts, ApiError> {
    let archived = is_archived(&task, app.store.workspace(&task.key).await?.is_some());
    // As `api_hierarchy::make_parent_task` reads them.
    let has_open_pr = app
        .store
        .prs_of(&task.key)
        .await?
        .iter()
        .any(|p| p.snapshot.as_ref().is_none_or(|s| s.state == PrState::Open));
    let has_session = !app
        .store
        .open_sessions(Some(task.key.as_str()))
        .await?
        .is_empty();
    Ok(ParentFacts {
        task,
        archived,
        has_open_pr,
        has_session,
    })
}

async fn task_facts(app: &AppState, task: Task) -> Result<TaskFacts, ApiError> {
    let has_workspace = app.store.workspace(&task.key).await?.is_some();
    let started = task.status != TaskStatus::Queued
        || has_workspace
        || !app.store.prs_of(&task.key).await?.is_empty()
        || !app
            .store
            .open_sessions(Some(task.key.as_str()))
            .await?
            .is_empty();
    Ok(TaskFacts {
        archived: is_archived(&task, has_workspace),
        task,
        started,
    })
}

/// The plan for importing `key`, or the reason Jira can't tell (`not-connected`,
/// `not-found`, `unreachable`). `project` is the one a new parent task would take.
pub(crate) async fn preview(
    app: &AppState,
    key: &str,
    project: Option<&str>,
) -> Result<Result<ImportPlan, &'static str>, ApiError> {
    if !is_issue_key(key) {
        return Ok(Err("not-found"));
    }
    if app
        .store
        .setting::<JiraSettings>(SETTINGS_KEY)
        .await?
        .is_none()
    {
        return Ok(Err("not-connected"));
    }
    let Ok((_, client)) = jira::client(app).await else {
        return Ok(Err("unreachable"));
    };
    let ticket = match client.issue(key).await {
        Ok(t) => t,
        Err(e) => return Ok(Err(unlinked(&e))),
    };
    let (subtasks, mine) = tokio::join!(client.subtasks(key), client.my_subtask_keys(key));
    let (jira_subtasks, my_keys) = match (subtasks, mine) {
        (Ok(s), Ok(m)) => (s, m),
        (Err(e), _) | (_, Err(e)) => return Ok(Err(unlinked(&e))),
    };

    let parent_key = TaskKey::new(key);
    let mut keys = vec![parent_key.clone()];
    keys.extend(jira_subtasks.iter().map(|j| TaskKey::new(&j.key)));
    let mut stored: HashMap<String, Task> = app
        .store
        .tasks_by_keys(&keys)
        .await?
        .into_iter()
        .map(|t| (t.key.as_str().to_string(), t))
        .collect();
    let parent = match stored.remove(key) {
        Some(t) => Some(parent_facts(app, t).await?),
        None => None,
    };
    let mut tasks = HashMap::new();
    for j in &jira_subtasks {
        if let Some(t) = stored.remove(&j.key) {
            tasks.insert(j.key.clone(), task_facts(app, t).await?);
        }
    }
    let is_parent_task = parent
        .as_ref()
        .is_some_and(|p| p.task.kind == TaskKind::ParentTask);
    let (children, skips) = if is_parent_task {
        (
            app.store.subtasks_of(&parent_key).await?,
            app.store
                .jira_skips_of(&parent_key)
                .await?
                .into_iter()
                .collect(),
        )
    } else {
        (Vec::new(), HashSet::new())
    };
    Ok(Ok(plan_import(
        &ticket,
        parent.as_ref(),
        &jira_subtasks,
        &tasks,
        &children,
        &skips,
        &my_keys,
        project,
    )))
}

/// `GET` answer: the plan, or why there is none.
#[derive(Serialize)]
#[serde(untagged)]
pub enum PreviewBody {
    Linked {
        linked: bool,
        #[serde(flatten)]
        plan: Box<ImportPlan>,
    },
    Unlinked {
        linked: bool,
        reason: &'static str,
    },
}

impl From<Result<ImportPlan, &'static str>> for PreviewBody {
    fn from(r: Result<ImportPlan, &'static str>) -> Self {
        match r {
            Ok(plan) => Self::Linked {
                linked: true,
                plan: Box::new(plan),
            },
            Err(reason) => Self::Unlinked {
                linked: false,
                reason,
            },
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct PreviewQuery {
    pub project: Option<String>,
}

/// `GET /api/v1/jira/issues/{key}/import`.
pub async fn get_preview(
    State(app): State<AppState>,
    Path(key): Path<String>,
    Query(q): Query<PreviewQuery>,
) -> Result<Json<PreviewBody>, ApiError> {
    check_key(&key)?;
    let project = q
        .project
        .as_deref()
        .map(str::trim)
        .filter(|p| !p.is_empty());
    Ok(Json(preview(&app, &key, project).await?.into()))
}

/// What the user chose in the dialog.
#[derive(Debug, Default, Deserialize)]
pub struct ImportRequest {
    pub project: Option<String>,
    #[serde(default)]
    pub add: Vec<String>,
    #[serde(default)]
    pub skip: Vec<String>,
    #[serde(default)]
    pub unskip: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LeftOut {
    pub key: String,
    pub reason: String,
}

#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize)]
pub struct ImportResult {
    pub parent_task: String,
    pub created: Vec<String>,
    pub attached: Vec<String>,
    pub left_out: Vec<LeftOut>,
    pub skipped: Vec<String>,
    pub unskipped: Vec<String>,
}

fn new_input(key: &str, title: &str, project: Option<&str>) -> NewTask {
    NewTask {
        key: key.to_string(),
        title: title.to_string(),
        project: project.map(String::from),
        notes: None,
        free: false,
        in_checkout: None,
        saving_mode: None,
        autopilot: None,
        workflow: None,
        view: None,
        ci_autofix: None,
        model: None,
        kind: None,
        parent_task: None,
    }
}

/// What the store holds for a task right now, read inside the transaction.
struct Live {
    task: Task,
    has_open_pr: bool,
    has_session: bool,
    started: bool,
    archived: bool,
}

/// Reads a task's facts on the transaction's connection (the pool may have no other).
async fn live(conn: &mut SqliteConnection, key: &str) -> Result<Option<Live>, ApiError> {
    let db = |e: sqlx::Error| ApiError::internal(e.to_string());
    let Some(row) =
        sqlx::query("SELECT kind, project, status, in_checkout FROM tasks WHERE key = ?")
            .bind(key)
            .fetch_optional(&mut *conn)
            .await
            .map_err(db)?
    else {
        return Ok(None);
    };
    let kind = match row.try_get::<String, _>("kind").map_err(db)?.as_str() {
        "parent_task" => TaskKind::ParentTask,
        "subtask" => TaskKind::Subtask,
        _ => TaskKind::Normal,
    };
    let status: TaskStatus = serde_json::from_str(&row.try_get::<String, _>("status").map_err(db)?)
        .map_err(|e| ApiError::internal(e.to_string()))?;
    let mut task = Task::new(TaskKey::new(key), "", "project");
    task.kind = kind;
    task.project = row.try_get("project").map_err(db)?;
    task.in_checkout = row.try_get("in_checkout").map_err(db)?;
    let has_workspace: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM workspaces WHERE task_key = ?")
            .bind(key)
            .fetch_one(&mut *conn)
            .await
            .map_err(db)?;
    let snapshots: Vec<Option<String>> =
        sqlx::query_scalar("SELECT snapshot FROM pull_requests WHERE task_key = ?")
            .bind(key)
            .fetch_all(&mut *conn)
            .await
            .map_err(db)?;
    let has_open_pr = snapshots.iter().any(|s| match s {
        None => true,
        Some(s) => serde_json::from_str::<Snapshot>(s).is_ok_and(|s| s.state == PrState::Open),
    });
    let sessions: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM sessions WHERE task_key = ?")
        .bind(key)
        .fetch_one(&mut *conn)
        .await
        .map_err(db)?;
    let open_sessions: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM sessions WHERE task_key = ? AND closed = 0")
            .bind(key)
            .fetch_one(&mut *conn)
            .await
            .map_err(db)?;
    Ok(Some(Live {
        started: status != TaskStatus::Queued
            || has_workspace > 0
            || !snapshots.is_empty()
            || sessions > 0,
        archived: status.is_terminal() && has_workspace == 0,
        has_open_pr,
        has_session: open_sessions > 0,
        task,
    }))
}

/// Why an attach is no longer possible, as a `left_out` reason.
fn left_out_reason(r: Refusal) -> &'static str {
    match r {
        Refusal::ProjectMismatch => "other-project",
        Refusal::InCheckout => "in-checkout",
        Refusal::Nesting | Refusal::NotAParentTask => "taken",
        _ => "started",
    }
}

fn gone(key: &str, reason: &str) -> LeftOut {
    LeftOut {
        key: key.to_string(),
        reason: reason.to_string(),
    }
}

/// Writes what the user chose: the parent task (created or converted), the new
/// subtasks, the attached tasks and the skips, all in one transaction that re-checks
/// what may have changed since the preview. `plan` is a fresh preview of the ticket.
pub async fn apply_jira_import(
    app: &AppState,
    plan: &ImportPlan,
    req: &ImportRequest,
) -> Result<ImportResult, ApiError> {
    let parent_key = TaskKey::new(&plan.ticket.key);
    let skip: HashSet<String> = req.skip.iter().cloned().collect();
    let unskip: HashSet<String> = req.unskip.iter().cloned().collect();
    // A key the user skips is not added in the same call.
    let wanted: HashSet<&str> = req
        .add
        .iter()
        .map(String::as_str)
        .filter(|k| !skip.contains(*k))
        .collect();
    let rows: Vec<&SubtaskRow> = plan
        .subtasks
        .iter()
        .filter(|r| wanted.contains(r.key.as_str()))
        .collect();

    // Everything that needs lookups happens before the transaction.
    let project = req
        .project
        .as_deref()
        .map(str::trim)
        .filter(|p| !p.is_empty());
    let parent_task = match plan.parent {
        ParentAction::Create => {
            let mut input = new_input(&plan.ticket.key, &plan.ticket.title, project);
            input.kind = Some(TaskKind::ParentTask);
            let (spec, defaults) = resolve_new_task(app, input, None).await?;
            Some(new_task(spec, &defaults))
        }
        _ => None,
    };
    let existing = app.store.get_task(&parent_key).await?;
    let effective_project: Option<String> = match (&parent_task, &existing) {
        (Some(t), _) => t.project.clone(),
        (None, Some(t)) => t.project.clone(),
        (None, None) => None,
    };
    let mut new_subtasks: HashMap<String, Task> = HashMap::new();
    for row in &rows {
        if row.action != SubtaskAction::Create {
            continue;
        }
        let task = if plan.parent == ParentAction::Existing {
            // The parent task is there: the usual rules for a new subtask apply.
            let mut input = new_input(&row.key, &row.title, None);
            input.parent_task = Some(parent_key.as_str().to_string());
            let (spec, defaults) = resolve_new_task(app, input, None).await?;
            new_task(spec, &defaults)
        } else {
            // The parent task is made in this call: nothing to check against yet.
            let input = new_input(&row.key, &row.title, effective_project.as_deref());
            let (mut spec, defaults) = resolve_new_task(app, input, None).await?;
            spec.kind = TaskKind::Subtask;
            spec.parent_task = Some(parent_key.clone());
            new_task(spec, &defaults)
        };
        new_subtasks.insert(row.key.clone(), task);
    }

    let mut result = ImportResult {
        parent_task: parent_key.as_str().to_string(),
        ..Default::default()
    };
    let mut changed = false;
    let mut tx = app.store.begin_immediate().await?;

    // The ticket itself.
    let stale = |what: &str| {
        coded(
            StatusCode::CONFLICT,
            "stale",
            format!("{} changed while importing: {what}", plan.ticket.key),
        )
    };
    let current = live(&mut tx, parent_key.as_str()).await?;
    let hypothetical_project = match plan.parent {
        ParentAction::Create => {
            if current.is_some() {
                return Err(stale("a task with its key appeared"));
            }
            let task = parent_task.as_ref().expect("built for Create");
            store::insert_task(&mut *tx, task).await?;
            changed = true;
            task.project.clone()
        }
        ParentAction::Convert => {
            let Some(cur) = current else {
                return Err(stale("the task is gone"));
            };
            if cur.task.kind != TaskKind::ParentTask {
                hierarchy::check_make_parent_task(&cur.task, cur.has_open_pr, cur.has_session)
                    .map_err(|r| {
                        let (code, message) = refusal_text(r);
                        coded(StatusCode::CONFLICT, code, message)
                    })?;
                store::become_parent_task(&mut *tx, &parent_key).await?;
                changed = true;
            }
            cur.task.project
        }
        ParentAction::Existing => {
            let Some(cur) = current.filter(|c| c.task.kind == TaskKind::ParentTask) else {
                return Err(stale("it is no longer a parent task"));
            };
            if cur.archived {
                let (code, message) = refusal_text(Refusal::ParentTaskArchived);
                return Err(coded(StatusCode::CONFLICT, code, message));
            }
            cur.task.project
        }
        ParentAction::Refused { code, ref message } => {
            return Err(coded(StatusCode::CONFLICT, code, message.clone()));
        }
    };
    let mut hypothetical = Task::new(parent_key.clone(), "", "project");
    hypothetical.kind = TaskKind::ParentTask;
    hypothetical.project = hypothetical_project.clone();

    // The sub-tasks, in the order Jira lists them.
    for row in rows {
        match &row.action {
            SubtaskAction::Create => {
                if live(&mut tx, &row.key).await?.is_some() {
                    result.left_out.push(gone(&row.key, "taken"));
                    continue;
                }
                let task = new_subtasks.get(&row.key).expect("built above");
                store::insert_task(&mut *tx, task).await?;
                result.created.push(row.key.clone());
                changed = true;
            }
            SubtaskAction::Attach => {
                let Some(cur) = live(&mut tx, &row.key).await? else {
                    result.left_out.push(gone(&row.key, "taken"));
                    continue;
                };
                if let Err(r) =
                    hierarchy::check_attach(&cur.task, &hypothetical, false, cur.started)
                {
                    result.left_out.push(gone(&row.key, left_out_reason(r)));
                    continue;
                }
                // A subtask takes its parent task's project.
                sqlx::query("UPDATE tasks SET project = ? WHERE key = ?")
                    .bind(hypothetical_project.as_deref())
                    .bind(&row.key)
                    .execute(&mut *tx)
                    .await
                    .map_err(|e| ApiError::internal(e.to_string()))?;
                store::set_kind_tx(
                    &mut *tx,
                    &TaskKey::new(&row.key),
                    TaskKind::Subtask,
                    Some(&parent_key),
                )
                .await?;
                result.attached.push(row.key.clone());
                changed = true;
            }
            SubtaskAction::Linked => result.left_out.push(gone(&row.key, "linked")),
            SubtaskAction::Conflict { reason } => result.left_out.push(gone(&row.key, reason)),
        }
    }

    store::remove_jira_skips(&mut tx, &parent_key, &unskip).await?;
    store::add_jira_skips(&mut tx, &parent_key, &skip).await?;
    tx.commit()
        .await
        .map_err(|e| ApiError::internal(e.to_string()))?;
    if changed {
        app.emit(Topic::Tasks);
    }
    result.skipped = sorted(skip);
    result.unskipped = sorted(unskip);
    Ok(result)
}

fn sorted(set: HashSet<String>) -> Vec<String> {
    let mut v: Vec<String> = set.into_iter().collect();
    v.sort();
    v
}

/// `POST /api/v1/jira/issues/{key}/import`.
pub async fn post_import(
    State(app): State<AppState>,
    Path(key): Path<String>,
    Json(req): Json<ImportRequest>,
) -> Result<Json<ImportResult>, ApiError> {
    check_key(&key)?;
    let project = req
        .project
        .as_deref()
        .map(str::trim)
        .filter(|p| !p.is_empty());
    let plan = match preview(&app, &key, project).await? {
        Ok(plan) => plan,
        Err(reason) => {
            return Err(ApiError::conflict_reason(
                format!("Jira can't tell about {key}: {reason}"),
                reason,
            ));
        }
    };
    if let ParentAction::Refused { code, message } = &plan.parent {
        return Err(coded(StatusCode::CONFLICT, code, message.clone()));
    }
    if plan.parent == ParentAction::Create && project.is_none() {
        return Err(coded(
            StatusCode::BAD_REQUEST,
            "project-required",
            "say which project the new parent task belongs to",
        ));
    }
    let known: HashSet<&str> = plan.subtasks.iter().map(|r| r.key.as_str()).collect();
    if let Some(bad) = req
        .add
        .iter()
        .chain(&req.skip)
        .chain(&req.unskip)
        .find(|k| !known.contains(k.as_str()))
    {
        return Err(coded(
            StatusCode::BAD_REQUEST,
            "not-a-subtask",
            format!("{bad} is not a sub-task of {key} in Jira"),
        ));
    }
    Ok(Json(apply_jira_import(&app, &plan, &req).await?))
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
            None,
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
            None,
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
            // Made by hand: never in Jira, so never "gone".
            task("by-hand", TaskKind::Subtask, None, Some("P-1")),
            task("OTHER-5", TaskKind::Subtask, None, Some("P-1")),
        ];
        let plan = plan_import(
            &ticket,
            Some(&parent),
            &[issue("P-2", "new", None, true)],
            &HashMap::new(),
            &kids,
            &HashSet::new(),
            &HashSet::new(),
            None,
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
            None,
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

    // --- handlers, through the router with the sample client ---

    use axum::body::Body;
    use axum::http::Request;
    use http_body_util::BodyExt;
    use serde_json::{Value, json};
    use tower::ServiceExt;

    async fn app() -> AppState {
        let config = crate::config::Config {
            home: std::env::temp_dir().join(format!("delilah-import-{}", uuid::Uuid::new_v4())),
            addr: crate::config::DEFAULT_ADDR.parse().unwrap(),
            allow_file_protocol: false,
            notify: false,
            ui_origin: None,
            claude_bin: "claude".into(),
            agent_program: None,
            ..crate::config::Config::test_defaults()
        };
        let state = AppState::new(crate::store::Store::in_memory().await.unwrap(), config);
        for name in ["web", "api"] {
            state
                .store
                .create_project(&delilah_core::Project {
                    name: name.into(),
                    path: format!("/w/{name}").into(),
                    kind: delilah_core::ProjectKind::Repo,
                    workflow: "prompt".into(),
                    settings: crate::autopilot::feature_settings(),
                    preview_command: None,
                })
                .await
                .unwrap();
        }
        state
    }

    async fn connect(state: &AppState) {
        let (s, _) = call(
            state,
            "PUT",
            "/api/v1/jira",
            Some(json!({ "mode": "sample" })),
        )
        .await;
        assert_eq!(s, StatusCode::OK);
    }

    async fn call(
        state: &AppState,
        method: &str,
        uri: &str,
        body: Option<Value>,
    ) -> (StatusCode, Value) {
        let mut req = Request::builder().method(method).uri(uri);
        let body = match body {
            Some(b) => {
                req = req.header("content-type", "application/json");
                Body::from(b.to_string())
            }
            None => Body::empty(),
        };
        let res = crate::api::router(state.clone())
            .oneshot(req.body(body).unwrap())
            .await
            .unwrap();
        let status = res.status();
        let bytes = res.into_body().collect().await.unwrap().to_bytes();
        (
            status,
            serde_json::from_slice(&bytes).unwrap_or(Value::Null),
        )
    }

    const PREVIEW: &str = "/api/v1/jira/issues/PROJ-1242/import";

    async fn stored(state: &AppState, key: &str) -> Option<Task> {
        state.store.get_task(&TaskKey::new(key)).await.unwrap()
    }

    /// How many `Tasks` events are waiting.
    fn tasks_events(rx: &mut tokio::sync::broadcast::Receiver<Topic>) -> usize {
        let mut n = 0;
        while let Ok(t) = rx.try_recv() {
            n += usize::from(t == Topic::Tasks);
        }
        n
    }

    #[tokio::test]
    async fn the_preview_lists_the_fixture_subtasks() {
        let state = app().await;
        connect(&state).await;
        let (s, body) = call(&state, "GET", PREVIEW, None).await;
        assert_eq!(s, StatusCode::OK, "{body}");
        assert_eq!(body["linked"], true);
        assert_eq!(body["ticket"]["key"], "PROJ-1242");
        assert_eq!(body["ticket"]["is_subtask"], false);
        assert_eq!(body["parent"], json!({ "action": "create" }));
        let rows = body["subtasks"].as_array().unwrap();
        assert_eq!(rows.len(), 2);
        let by = |k: &str| rows.iter().find(|r| r["key"] == k).unwrap();
        // Mine: preselected. Ana's: not.
        assert_eq!(by("PROJ-1250")["selected"], true);
        assert_eq!(by("PROJ-1250")["mine"], true);
        assert_eq!(by("PROJ-1250")["action"], "create");
        assert_eq!(by("PROJ-1251")["selected"], false);
        assert_eq!(by("PROJ-1251")["assignee"], "Ana");
    }

    #[tokio::test]
    async fn the_preview_answers_for_bad_keys_and_missing_jira() {
        let state = app().await;
        let (s, body) = call(&state, "GET", "/api/v1/jira/issues/nope/import", None).await;
        assert_eq!(
            (s, body["error"].as_str()),
            (StatusCode::BAD_REQUEST, Some("bad-key"))
        );

        // Not connected: quiet 200, no Jira call needed.
        let (s, body) = call(&state, "GET", PREVIEW, None).await;
        assert_eq!(s, StatusCode::OK);
        assert_eq!(body, json!({ "linked": false, "reason": "not-connected" }));

        connect(&state).await;
        let (s, body) = call(&state, "GET", "/api/v1/jira/issues/NOPE-1/import", None).await;
        assert_eq!(s, StatusCode::OK);
        assert_eq!(body, json!({ "linked": false, "reason": "not-found" }));
    }

    #[tokio::test]
    async fn the_import_creates_the_parent_task_and_the_chosen_subtasks() {
        let state = app().await;
        connect(&state).await;
        let mut rx = state.events.subscribe();
        let (s, body) = call(
            &state,
            "POST",
            PREVIEW,
            Some(json!({ "project": "web", "add": ["PROJ-1250"], "skip": ["PROJ-1251"] })),
        )
        .await;
        assert_eq!(s, StatusCode::OK, "{body}");
        assert_eq!(body["parent_task"], "PROJ-1242");
        assert_eq!(body["created"], json!(["PROJ-1250"]));
        assert_eq!(body["skipped"], json!(["PROJ-1251"]));
        assert_eq!(tasks_events(&mut rx), 1);

        let parent = stored(&state, "PROJ-1242").await.unwrap();
        assert_eq!(parent.kind, TaskKind::ParentTask);
        assert_eq!(parent.workflow, "none");
        assert_eq!(parent.project.as_deref(), Some("web"));
        let sub = stored(&state, "PROJ-1250").await.unwrap();
        assert_eq!(sub.kind, TaskKind::Subtask);
        assert_eq!(sub.parent_task, Some(TaskKey::new("PROJ-1242")));
        assert_eq!(sub.project.as_deref(), Some("web"));
        assert_eq!(sub.status, TaskStatus::Queued);
        assert!(state.store.workspace(&sub.key).await.unwrap().is_none());
        assert!(stored(&state, "PROJ-1251").await.is_none());

        // The preview now knows the parent task, the link and the skip.
        let (_, body) = call(&state, "GET", PREVIEW, None).await;
        assert_eq!(body["parent"], json!({ "action": "existing" }));
        let rows = body["subtasks"].as_array().unwrap();
        let by = |k: &str| rows.iter().find(|r| r["key"] == k).unwrap();
        assert_eq!(by("PROJ-1250")["action"], "linked");
        assert_eq!(by("PROJ-1251")["skipped"], true);
        // Importing again with nothing to add writes and announces nothing.
        let (s, body) = call(
            &state,
            "POST",
            PREVIEW,
            Some(json!({ "unskip": ["PROJ-1251"] })),
        )
        .await;
        assert_eq!(s, StatusCode::OK, "{body}");
        assert_eq!(body["unskipped"], json!(["PROJ-1251"]));
        assert_eq!(tasks_events(&mut rx), 0);
        assert!(
            state
                .store
                .jira_skips_of(&parent.key)
                .await
                .unwrap()
                .is_empty()
        );
    }

    #[tokio::test]
    async fn bad_requests_write_nothing() {
        let state = app().await;
        connect(&state).await;
        let (s, body) = call(
            &state,
            "POST",
            PREVIEW,
            Some(json!({ "add": ["PROJ-1250"] })),
        )
        .await;
        assert_eq!(
            (s, body["error"].as_str()),
            (StatusCode::BAD_REQUEST, Some("project-required"))
        );
        // PROJ-1243 exists in Jira but is not a sub-task of PROJ-1242.
        let (s, body) = call(
            &state,
            "POST",
            PREVIEW,
            Some(json!({ "project": "web", "add": ["PROJ-1250", "PROJ-1243"] })),
        )
        .await;
        assert_eq!(
            (s, body["error"].as_str()),
            (StatusCode::BAD_REQUEST, Some("not-a-subtask"))
        );
        let (s, _) = call(
            &state,
            "POST",
            PREVIEW,
            Some(json!({ "project": "web", "skip": ["PROJ-9"] })),
        )
        .await;
        assert_eq!(s, StatusCode::BAD_REQUEST);
        assert!(stored(&state, "PROJ-1242").await.is_none());
        assert!(stored(&state, "PROJ-1250").await.is_none());
    }

    #[tokio::test]
    async fn skips_and_unskips_need_a_parent_and_are_silent_writes() {
        let state = app().await;
        connect(&state).await;
        // No parent task and no project to create one in: 400, nothing written.
        let (s, body) = call(
            &state,
            "POST",
            PREVIEW,
            Some(json!({ "skip": ["PROJ-1251"] })),
        )
        .await;
        assert_eq!(
            (s, body["error"].as_str()),
            (StatusCode::BAD_REQUEST, Some("project-required"))
        );
        let (s, _) = call(
            &state,
            "POST",
            PREVIEW,
            Some(json!({ "unskip": ["PROJ-1251"] })),
        )
        .await;
        assert_eq!(s, StatusCode::BAD_REQUEST);
        assert!(stored(&state, "PROJ-1242").await.is_none());

        // A hand-made parent task whose key is a sample ticket gets the offer.
        put_task(&state, "PROJ-1242", "web", TaskKind::ParentTask, None).await;
        let (_, body) = call(&state, "GET", PREVIEW, None).await;
        assert_eq!(body["parent"], json!({ "action": "existing" }));
        let rows = body["subtasks"].as_array().unwrap();
        let by = |k: &str| rows.iter().find(|r| r["key"] == k).unwrap();
        assert_eq!(by("PROJ-1250")["action"], "create");
        assert_eq!(by("PROJ-1250")["selected"], true);

        // A skip-only write (empty `add`) is valid and announces nothing.
        let mut rx = state.events.subscribe();
        let (s, body) = call(
            &state,
            "POST",
            PREVIEW,
            Some(json!({ "add": [], "skip": ["PROJ-1250"] })),
        )
        .await;
        assert_eq!(s, StatusCode::OK, "{body}");
        assert_eq!(body["skipped"], json!(["PROJ-1250"]));
        assert_eq!(tasks_events(&mut rx), 0);
        let (_, body) = call(&state, "GET", PREVIEW, None).await;
        let rows = body["subtasks"].as_array().unwrap();
        let row = rows.iter().find(|r| r["key"] == "PROJ-1250").unwrap();
        assert_eq!(
            (row["skipped"].clone(), row["selected"].clone()),
            (json!(true), json!(false))
        );

        // Unskipping reverses it.
        let (s, _) = call(
            &state,
            "POST",
            PREVIEW,
            Some(json!({ "unskip": ["PROJ-1250"] })),
        )
        .await;
        assert_eq!(s, StatusCode::OK);
        let (_, body) = call(&state, "GET", PREVIEW, None).await;
        let rows = body["subtasks"].as_array().unwrap();
        let row = rows.iter().find(|r| r["key"] == "PROJ-1250").unwrap();
        assert_eq!(
            (row["skipped"].clone(), row["selected"].clone()),
            (json!(false), json!(true))
        );
    }

    #[tokio::test]
    async fn a_libre_parent_is_not_found_and_deleted_or_archived_subtasks_follow_the_store() {
        let state = app().await;
        connect(&state).await;
        let (s, body) = call(&state, "GET", "/api/v1/jira/issues/LIBRE-5/import", None).await;
        assert_eq!(s, StatusCode::OK);
        assert_eq!(body, json!({ "linked": false, "reason": "not-found" }));

        put_task(&state, "PROJ-1242", "web", TaskKind::ParentTask, None).await;
        put_task(
            &state,
            "PROJ-1250",
            "web",
            TaskKind::Subtask,
            Some("PROJ-1242"),
        )
        .await;
        let (_, body) = call(&state, "GET", PREVIEW, None).await;
        let rows = body["subtasks"].as_array().unwrap();
        assert_eq!(
            rows.iter().find(|r| r["key"] == "PROJ-1250").unwrap()["action"],
            "linked"
        );

        // Archived (finished, no workspace): still linked.
        sqlx::query("UPDATE tasks SET status = '{\"type\":\"done\"}' WHERE key = 'PROJ-1250'")
            .execute(&state.store.pool)
            .await
            .unwrap();
        let (_, body) = call(&state, "GET", PREVIEW, None).await;
        let rows = body["subtasks"].as_array().unwrap();
        let row = rows.iter().find(|r| r["key"] == "PROJ-1250").unwrap();
        assert_eq!(row["action"], "linked");
        assert_eq!(row["task"]["archived"], true);

        // Deleted: offered again.
        sqlx::query("DELETE FROM tasks WHERE key = 'PROJ-1250'")
            .execute(&state.store.pool)
            .await
            .unwrap();
        let (_, body) = call(&state, "GET", PREVIEW, None).await;
        let rows = body["subtasks"].as_array().unwrap();
        let row = rows.iter().find(|r| r["key"] == "PROJ-1250").unwrap();
        assert_eq!(
            (row["action"].clone(), row["selected"].clone()),
            (json!("create"), json!(true))
        );
    }

    #[tokio::test]
    async fn a_normal_ticket_task_converts_and_a_backlog_task_attaches() {
        let state = app().await;
        connect(&state).await;
        for key in ["PROJ-1242", "PROJ-1250"] {
            let mut t = Task::new(TaskKey::new(key), key, "project");
            t.project = Some("web".into());
            state.store.create_task(&t).await.unwrap();
        }
        // Preview with the planner's project: the attach is fine for the parent's own.
        let (_, body) = call(&state, "GET", PREVIEW, None).await;
        assert_eq!(body["parent"], json!({ "action": "convert" }));
        let by = |k: &str| {
            body["subtasks"]
                .as_array()
                .unwrap()
                .iter()
                .find(|r| r["key"] == k)
                .unwrap()
                .clone()
        };
        assert_eq!(by("PROJ-1250")["action"], "attach");

        let (s, res) = call(
            &state,
            "POST",
            PREVIEW,
            Some(json!({ "add": ["PROJ-1250", "PROJ-1251"] })),
        )
        .await;
        assert_eq!(s, StatusCode::OK, "{res}");
        assert_eq!(res["attached"], json!(["PROJ-1250"]));
        assert_eq!(res["created"], json!(["PROJ-1251"]));
        assert_eq!(
            stored(&state, "PROJ-1242").await.unwrap().kind,
            TaskKind::ParentTask
        );
        let attached = stored(&state, "PROJ-1250").await.unwrap();
        assert_eq!(attached.kind, TaskKind::Subtask);
        assert_eq!(attached.parent_task, Some(TaskKey::new("PROJ-1242")));
        // A sub-task made for a converted parent takes its project.
        let created = stored(&state, "PROJ-1251").await.unwrap();
        assert_eq!(created.project.as_deref(), Some("web"));
    }

    #[tokio::test]
    async fn a_refused_parent_is_a_409_and_a_stale_plan_writes_nothing() {
        let state = app().await;
        connect(&state).await;
        let mut t = Task::new(TaskKey::new("PROJ-1242"), "p", "project");
        t.project = Some("web".into());
        state.store.create_task(&t).await.unwrap();
        sqlx::query(
            "INSERT INTO sessions (id, task_key, kind, label, is_primary, created_at)
             VALUES ('s1', 'PROJ-1242', 'claude', 'Main', 1, 'x')",
        )
        .execute(&state.store.pool)
        .await
        .unwrap();
        let (s, body) = call(
            &state,
            "POST",
            PREVIEW,
            Some(json!({ "add": ["PROJ-1250"] })),
        )
        .await;
        assert_eq!(
            (s, body["error"].as_str()),
            (StatusCode::CONFLICT, Some("running-session"))
        );

        // A plan made before the session opened: the transaction re-checks and rolls back.
        sqlx::query("DELETE FROM sessions")
            .execute(&state.store.pool)
            .await
            .unwrap();
        let plan = preview(&state, "PROJ-1242", None)
            .await
            .ok()
            .unwrap()
            .unwrap();
        assert_eq!(plan.parent, ParentAction::Convert);
        sqlx::query(
            "INSERT INTO sessions (id, task_key, kind, label, is_primary, created_at)
             VALUES ('s2', 'PROJ-1242', 'claude', 'Main', 1, 'x')",
        )
        .execute(&state.store.pool)
        .await
        .unwrap();
        let req = ImportRequest {
            add: vec!["PROJ-1250".into()],
            skip: vec!["PROJ-1251".into()],
            ..Default::default()
        };
        let err = apply_jira_import(&state, &plan, &req).await.err().unwrap();
        assert_eq!(err.status, StatusCode::CONFLICT);
        assert_eq!(
            stored(&state, "PROJ-1242").await.unwrap().kind,
            TaskKind::Normal
        );
        assert!(stored(&state, "PROJ-1250").await.is_none());
        assert!(
            state
                .store
                .jira_skips_of(&TaskKey::new("PROJ-1242"))
                .await
                .unwrap()
                .is_empty()
        );
    }

    #[tokio::test]
    async fn a_started_or_taken_task_is_left_out_not_failed() {
        let state = app().await;
        connect(&state).await;
        let plan = preview(&state, "PROJ-1242", Some("web"))
            .await
            .ok()
            .unwrap()
            .unwrap();
        assert_eq!(plan.subtasks[0].action, SubtaskAction::Create);
        // Someone made PROJ-1250 after the preview.
        let t = Task::new(TaskKey::new("PROJ-1250"), "mine", "project");
        state.store.create_task(&t).await.unwrap();
        let req = ImportRequest {
            project: Some("web".into()),
            add: vec!["PROJ-1250".into()],
            ..Default::default()
        };
        let res = apply_jira_import(&state, &plan, &req).await.ok().unwrap();
        assert!(res.created.is_empty());
        assert_eq!(
            res.left_out,
            vec![LeftOut {
                key: "PROJ-1250".into(),
                reason: "taken".into()
            }]
        );
        assert_eq!(
            stored(&state, "PROJ-1242").await.unwrap().kind,
            TaskKind::ParentTask
        );
    }

    #[tokio::test]
    async fn a_subtask_moved_to_another_ticket_in_jira_is_gone_and_left_alone() {
        let state = app().await;
        connect(&state).await;
        put_task(&state, "PROJ-1242", "web", TaskKind::ParentTask, None).await;
        // Not one of the sample ticket's sub-tasks: as if Jira moved it elsewhere.
        put_task(
            &state,
            "PROJ-7777",
            "web",
            TaskKind::Subtask,
            Some("PROJ-1242"),
        )
        .await;
        let (_, body) = call(&state, "GET", PREVIEW, None).await;
        assert_eq!(
            body["gone"],
            json!([{ "key": "PROJ-7777", "title": "PROJ-7777" }])
        );
        assert!(
            body["subtasks"]
                .as_array()
                .unwrap()
                .iter()
                .all(|r| r["key"] != "PROJ-7777")
        );

        // Applying an import neither archives nor changes it.
        let before = stored(&state, "PROJ-7777").await.unwrap();
        let (s, res) = call(
            &state,
            "POST",
            PREVIEW,
            Some(json!({ "add": ["PROJ-1250"] })),
        )
        .await;
        assert_eq!(s, StatusCode::OK, "{res}");
        let after = stored(&state, "PROJ-7777").await.unwrap();
        assert_eq!(after.status, before.status);
        assert_eq!(after.kind, TaskKind::Subtask);
        assert_eq!(after.parent_task, before.parent_task);
        let (_, body) = call(&state, "GET", PREVIEW, None).await;
        assert_eq!(body["gone"].as_array().unwrap().len(), 1);
    }

    async fn put_task(
        state: &AppState,
        key: &str,
        project: &str,
        kind: TaskKind,
        parent: Option<&str>,
    ) {
        let mut t = Task::new(TaskKey::new(key), key, "project");
        t.project = Some(project.into());
        t.kind = kind;
        t.parent_task = parent.map(TaskKey::new);
        state.store.create_task(&t).await.unwrap();
    }

    async fn open_session(state: &AppState, key: &str) {
        sqlx::query(
            "INSERT INTO sessions (id, task_key, kind, label, is_primary, created_at)
             VALUES (?, ?, 'claude', 'Main', 1, 'x')",
        )
        .bind(format!("s-{key}"))
        .bind(key)
        .execute(&state.store.pool)
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn converting_keeps_the_status_and_an_existing_parent_only_adds() {
        let state = app().await;
        connect(&state).await;
        put_task(&state, "PROJ-1242", "web", TaskKind::Normal, None).await;
        sqlx::query("UPDATE tasks SET status = '{\"type\":\"failed\",\"reason\":\"x\"}' WHERE key = 'PROJ-1242'")
            .execute(&state.store.pool)
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO workspaces (task_key, parent, path, branch, submodules)
             VALUES ('PROJ-1242', '/w', '/w/t', 'b', '[]')",
        )
        .execute(&state.store.pool)
        .await
        .unwrap();
        let (s, res) = call(
            &state,
            "POST",
            PREVIEW,
            Some(json!({ "add": ["PROJ-1250"] })),
        )
        .await;
        assert_eq!(s, StatusCode::OK, "{res}");
        let t = stored(&state, "PROJ-1242").await.unwrap();
        assert_eq!(t.kind, TaskKind::ParentTask);
        assert!(matches!(t.status, TaskStatus::Failed { .. }));

        // An existing parent task: the call only adds.
        let state = app().await;
        connect(&state).await;
        put_task(&state, "PROJ-1242", "web", TaskKind::ParentTask, None).await;
        let (s, res) = call(
            &state,
            "POST",
            PREVIEW,
            Some(json!({ "add": ["PROJ-1250"] })),
        )
        .await;
        assert_eq!(s, StatusCode::OK, "{res}");
        assert_eq!(res["created"], json!(["PROJ-1250"]));
        assert_eq!(res["attached"], json!([]));
        assert_eq!(
            stored(&state, "PROJ-1250").await.unwrap().kind,
            TaskKind::Subtask
        );

        // Second run: nothing more is created (SC-003).
        let (s, res) = call(
            &state,
            "POST",
            PREVIEW,
            Some(json!({ "add": ["PROJ-1250", "PROJ-1251"] })),
        )
        .await;
        assert_eq!(s, StatusCode::OK, "{res}");
        assert_eq!(res["created"], json!(["PROJ-1251"]));
        let (_, res) = call(
            &state,
            "POST",
            PREVIEW,
            Some(json!({ "add": ["PROJ-1250", "PROJ-1251"] })),
        )
        .await;
        assert_eq!(res["created"], json!([]));
        assert_eq!(res["attached"], json!([]));
    }

    #[tokio::test]
    async fn a_converted_task_loses_nothing_but_its_agent_settings() {
        let state = app().await;
        connect(&state).await;
        put_task(&state, "PROJ-1242", "web", TaskKind::Normal, None).await;
        sqlx::query("UPDATE tasks SET notes = 'keep me', saving_mode = 1, status = '{\"type\":\"running\"}' WHERE key = 'PROJ-1242'")
            .execute(&state.store.pool)
            .await
            .unwrap();
        let (s, res) = call(&state, "POST", PREVIEW, Some(json!({}))).await;
        assert_eq!(s, StatusCode::OK, "{res}");
        let t = stored(&state, "PROJ-1242").await.unwrap();
        assert_eq!(t.kind, TaskKind::ParentTask);
        assert_eq!(t.status, TaskStatus::Running);
        assert_eq!(t.notes.as_deref(), Some("keep me"));
        assert_eq!(t.workflow, "none");
        assert!(!t.saving_mode);
    }

    #[tokio::test]
    async fn a_ticket_task_that_cannot_be_a_parent_is_a_409_and_writes_nothing() {
        // A sub-task of another parent task.
        let state = app().await;
        connect(&state).await;
        put_task(&state, "OTHER-1", "web", TaskKind::ParentTask, None).await;
        put_task(
            &state,
            "PROJ-1242",
            "web",
            TaskKind::Subtask,
            Some("OTHER-1"),
        )
        .await;
        let body = json!({ "add": ["PROJ-1250"] });
        let (s, res) = call(&state, "POST", PREVIEW, Some(body.clone())).await;
        assert_eq!(
            (s, res["error"].as_str()),
            (StatusCode::CONFLICT, Some("nesting"))
        );
        assert!(stored(&state, "PROJ-1250").await.is_none());

        // An open PR.
        let state = app().await;
        connect(&state).await;
        put_task(&state, "PROJ-1242", "web", TaskKind::Normal, None).await;
        sqlx::query(
            "INSERT INTO pull_requests (task_key, repo, host, owner_name, number, url, linked_by, created_at)
             VALUES ('PROJ-1242', '.', 'github.com', 'o/n', 1, 'u', 'pasted', '2026-01-01T00:00:00Z')",
        )
        .execute(&state.store.pool)
        .await
        .unwrap();
        let (s, res) = call(&state, "POST", PREVIEW, Some(body.clone())).await;
        assert_eq!(
            (s, res["error"].as_str()),
            (StatusCode::CONFLICT, Some("open-pr"))
        );
        assert!(stored(&state, "PROJ-1250").await.is_none());
        assert_eq!(
            stored(&state, "PROJ-1242").await.unwrap().kind,
            TaskKind::Normal
        );

        // Archived: finished with no workspace.
        let state = app().await;
        connect(&state).await;
        put_task(&state, "PROJ-1242", "web", TaskKind::Normal, None).await;
        sqlx::query("UPDATE tasks SET status = '{\"type\":\"done\"}' WHERE key = 'PROJ-1242'")
            .execute(&state.store.pool)
            .await
            .unwrap();
        let (s, res) = call(&state, "POST", PREVIEW, Some(body)).await;
        assert_eq!(
            (s, res["error"].as_str()),
            (StatusCode::CONFLICT, Some("archived"))
        );
        assert!(stored(&state, "PROJ-1250").await.is_none());
        assert_eq!(
            stored(&state, "PROJ-1242").await.unwrap().kind,
            TaskKind::Normal
        );
    }

    #[tokio::test]
    async fn attach_leaves_out_started_other_project_and_taken_tasks() {
        // A started task and one that already hangs from another parent task.
        let state = app().await;
        connect(&state).await;
        put_task(&state, "OTHER-1", "web", TaskKind::ParentTask, None).await;
        put_task(&state, "PROJ-1250", "web", TaskKind::Normal, None).await;
        open_session(&state, "PROJ-1250").await;
        put_task(
            &state,
            "PROJ-1251",
            "web",
            TaskKind::Subtask,
            Some("OTHER-1"),
        )
        .await;
        let (s, res) = call(
            &state,
            "POST",
            PREVIEW,
            Some(json!({ "project": "web", "add": ["PROJ-1250", "PROJ-1251"] })),
        )
        .await;
        assert_eq!(s, StatusCode::OK, "{res}");
        assert_eq!(res["attached"], json!([]));
        assert_eq!(res["created"], json!([]));
        let left = res["left_out"].as_array().unwrap();
        assert_eq!(left.len(), 2, "{res}");
        let reason = |k: &str| left.iter().find(|l| l["key"] == k).unwrap()["reason"].clone();
        assert_eq!(reason("PROJ-1250"), "started");
        assert!(reason("PROJ-1251").is_string());
        assert_eq!(
            stored(&state, "PROJ-1250").await.unwrap().kind,
            TaskKind::Normal
        );
        assert_eq!(
            stored(&state, "PROJ-1251").await.unwrap().parent_task,
            Some(TaskKey::new("OTHER-1"))
        );

        // A task of another project than the parent task's.
        let state = app().await;
        connect(&state).await;
        put_task(&state, "PROJ-1250", "api", TaskKind::Normal, None).await;
        let (s, res) = call(
            &state,
            "POST",
            PREVIEW,
            Some(json!({ "project": "web", "add": ["PROJ-1250", "PROJ-1251"] })),
        )
        .await;
        assert_eq!(s, StatusCode::OK, "{res}");
        assert_eq!(res["created"], json!(["PROJ-1251"]));
        assert_eq!(
            res["left_out"],
            json!([{ "key": "PROJ-1250", "reason": "other-project" }])
        );
    }

    #[tokio::test]
    async fn a_failing_insert_after_a_convert_rolls_the_convert_back() {
        let state = app().await;
        connect(&state).await;
        put_task(&state, "PROJ-1242", "web", TaskKind::Normal, None).await;
        let plan = preview(&state, "PROJ-1242", None)
            .await
            .ok()
            .unwrap()
            .unwrap();
        assert_eq!(plan.parent, ParentAction::Convert);
        sqlx::query(
            "CREATE TRIGGER boom BEFORE INSERT ON tasks WHEN NEW.key = 'PROJ-1250'
             BEGIN SELECT RAISE(ABORT, 'boom'); END",
        )
        .execute(&state.store.pool)
        .await
        .unwrap();
        let req = ImportRequest {
            add: vec!["PROJ-1250".into()],
            skip: vec!["PROJ-1251".into()],
            ..Default::default()
        };
        let mut rx = state.events.subscribe();
        assert!(apply_jira_import(&state, &plan, &req).await.is_err());
        assert_eq!(
            stored(&state, "PROJ-1242").await.unwrap().kind,
            TaskKind::Normal
        );
        assert!(stored(&state, "PROJ-1250").await.is_none());
        assert!(
            state
                .store
                .jira_skips_of(&TaskKey::new("PROJ-1242"))
                .await
                .unwrap()
                .is_empty()
        );
        assert_eq!(tasks_events(&mut rx), 0);
    }

    #[tokio::test]
    async fn a_session_started_after_the_preview_is_a_409_and_an_attach_target_is_left_out() {
        let state = app().await;
        connect(&state).await;
        put_task(&state, "PROJ-1242", "web", TaskKind::Normal, None).await;
        put_task(&state, "PROJ-1250", "web", TaskKind::Normal, None).await;
        let plan = preview(&state, "PROJ-1242", None)
            .await
            .ok()
            .unwrap()
            .unwrap();
        let by = |k: &str| {
            plan.subtasks
                .iter()
                .find(|r| r.key == k)
                .unwrap()
                .action
                .clone()
        };
        assert_eq!(by("PROJ-1250"), SubtaskAction::Attach);

        // The attach target starts first: left out, the rest goes through.
        open_session(&state, "PROJ-1250").await;
        let req = ImportRequest {
            add: vec!["PROJ-1250".into(), "PROJ-1251".into()],
            ..Default::default()
        };
        let res = apply_jira_import(&state, &plan, &req).await.ok().unwrap();
        assert_eq!(res.left_out, vec![gone("PROJ-1250", "started")]);
        assert_eq!(res.created, vec!["PROJ-1251".to_string()]);
        assert!(res.attached.is_empty());
    }

    #[tokio::test]
    async fn a_session_on_the_ticket_task_after_the_preview_writes_nothing() {
        let state = app().await;
        connect(&state).await;
        put_task(&state, "PROJ-1242", "web", TaskKind::Normal, None).await;
        let plan = preview(&state, "PROJ-1242", None)
            .await
            .ok()
            .unwrap()
            .unwrap();
        open_session(&state, "PROJ-1242").await;
        let req = ImportRequest {
            add: vec!["PROJ-1250".into()],
            ..Default::default()
        };
        let err = apply_jira_import(&state, &plan, &req).await.err().unwrap();
        assert_eq!(err.status, StatusCode::CONFLICT);
        assert_eq!(
            stored(&state, "PROJ-1242").await.unwrap().kind,
            TaskKind::Normal
        );
        assert!(stored(&state, "PROJ-1250").await.is_none());
    }

    #[test]
    fn the_chosen_project_decides_whether_a_task_attaches() {
        let ticket = issue("P-1", "new", None, false);
        let mut tasks = HashMap::new();
        tasks.insert(
            "P-2".to_string(),
            tf(
                task("P-2", TaskKind::Normal, Some("api"), None),
                false,
                false,
            ),
        );
        let subtasks = [issue("P-2", "new", None, true)];
        let action = |project: Option<&str>| {
            plan_import(
                &ticket,
                None,
                &subtasks,
                &tasks,
                &[],
                &HashSet::new(),
                &HashSet::new(),
                project,
            )
            .subtasks[0]
                .action
                .clone()
        };
        assert_eq!(action(Some("api")), SubtaskAction::Attach);
        assert_eq!(action(None), SubtaskAction::Attach);
        assert_eq!(
            action(Some("web")),
            SubtaskAction::Conflict {
                reason: "other-project"
            }
        );
    }
}
