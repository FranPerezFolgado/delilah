//! Parent tasks and subtasks: pure hierarchy logic.
//!
//! The daemon gathers the facts (PRs, workspaces, sessions) and calls these; the
//! refusals carry no text, the daemon maps each to its reason and localised message.

use serde::{Deserialize, Serialize};

use crate::task::Task;

/// What a task is in the hierarchy. Existing tasks are `Normal`; the kind only
/// moves `Normal -> ParentTask` or `Normal -> Subtask`, never back.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskKind {
    #[default]
    Normal,
    ParentTask,
    Subtask,
}

/// What `progress` needs to know about one subtask.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChildFacts {
    /// Has at least one PR and every PR is merged.
    pub merged: bool,
    /// Terminal status and no workspace row.
    pub archived: bool,
}

/// "N/M subtasks merged" of a parent task.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Progress {
    pub merged: u32,
    /// Subtasks minus the dropped ones.
    pub counted: u32,
    pub dropped: u32,
    /// `counted > 0` and every counted subtask is merged.
    pub ready: bool,
}

/// Progress of a parent task over its existing subtasks. A dropped subtask is
/// archived without having merged; it stops counting.
pub fn progress(children: &[ChildFacts]) -> Progress {
    let merged = children.iter().filter(|c| c.merged).count() as u32;
    let dropped = children.iter().filter(|c| c.archived && !c.merged).count() as u32;
    let counted = children.len() as u32 - dropped;
    Progress {
        merged,
        counted,
        dropped,
        ready: counted > 0 && merged == counted,
    }
}

/// A subtask is live until archived.
pub fn is_live(child: &ChildFacts) -> bool {
    !child.archived
}

/// Why a hierarchy operation is refused; one variant per reason of the API contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Refusal {
    NotAParentTask,
    ParentTaskArchived,
    Nesting,
    ProjectMismatch,
    ParentTaskNotStarted,
    ParentTaskBranchMissing,
    HasOpenPr,
    HasSession,
    InCheckout,
    AlreadyStarted,
    LiveSubtasks,
    NoCommits,
    EmptyCommitNotAllowed,
}

/// Creating a subtask of `parent` (`None` = the key names no task).
/// `explicit_project` is the project the request names, if any.
pub fn check_new_subtask(
    parent: Option<&Task>,
    parent_archived: bool,
    explicit_project: Option<&str>,
) -> Result<(), Refusal> {
    let parent = parent
        .filter(|p| p.kind == TaskKind::ParentTask)
        .ok_or(Refusal::NotAParentTask)?;
    if parent_archived {
        return Err(Refusal::ParentTaskArchived);
    }
    same_project(explicit_project, parent.project.as_deref())
}

/// Turning a normal task into a parent task.
pub fn check_make_parent_task(
    task: &Task,
    has_open_pr: bool,
    has_session: bool,
) -> Result<(), Refusal> {
    match task.kind {
        TaskKind::Subtask => return Err(Refusal::Nesting),
        TaskKind::ParentTask => return Ok(()),
        TaskKind::Normal => {}
    }
    if task.in_checkout == Some(true) {
        return Err(Refusal::InCheckout);
    }
    if has_open_pr {
        return Err(Refusal::HasOpenPr);
    }
    if has_session {
        return Err(Refusal::HasSession);
    }
    Ok(())
}

/// Attaching a normal task to `parent`; `started` = it has run before.
pub fn check_attach(
    task: &Task,
    parent: &Task,
    parent_archived: bool,
    started: bool,
) -> Result<(), Refusal> {
    if task.kind != TaskKind::Normal {
        return Err(Refusal::Nesting);
    }
    if parent.kind != TaskKind::ParentTask {
        return Err(Refusal::NotAParentTask);
    }
    if parent_archived {
        return Err(Refusal::ParentTaskArchived);
    }
    if started {
        return Err(Refusal::AlreadyStarted);
    }
    if task.in_checkout == Some(true) {
        return Err(Refusal::InCheckout);
    }
    same_project(task.project.as_deref(), parent.project.as_deref())
}

/// Starting a subtask needs its parent task's workspace.
pub fn check_start_subtask(parent_has_workspace: bool) -> Result<(), Refusal> {
    if parent_has_workspace {
        Ok(())
    } else {
        Err(Refusal::ParentTaskNotStarted)
    }
}

fn same_project(own: Option<&str>, parent: Option<&str>) -> Result<(), Refusal> {
    match (own, parent) {
        (Some(a), Some(b)) if a != b => Err(Refusal::ProjectMismatch),
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::task::TaskKey;

    fn task(key: &str, kind: TaskKind, project: Option<&str>) -> Task {
        let mut t = Task::new(TaskKey::new(key), "t", "feature");
        t.kind = kind;
        t.project = project.map(String::from);
        t
    }

    fn c(merged: bool, archived: bool) -> ChildFacts {
        ChildFacts { merged, archived }
    }

    #[test]
    fn progress_counts_merged_and_ready() {
        let p = progress(&[c(true, false), c(false, false)]);
        assert_eq!((p.merged, p.counted, p.dropped, p.ready), (1, 2, 0, false));
        assert!(progress(&[c(true, false), c(true, true)]).ready);
    }

    #[test]
    fn progress_zero_subtasks_not_ready() {
        let p = progress(&[]);
        assert_eq!((p.merged, p.counted, p.dropped, p.ready), (0, 0, 0, false));
    }

    #[test]
    fn progress_dropped_do_not_count() {
        let p = progress(&[c(false, true), c(true, false), c(false, false)]);
        assert_eq!((p.merged, p.counted, p.dropped, p.ready), (1, 2, 1, false));
        let all = progress(&[c(false, true), c(false, true)]);
        assert_eq!(
            (all.merged, all.counted, all.dropped, all.ready),
            (0, 0, 2, false)
        );
    }

    #[test]
    fn is_live_until_archived() {
        assert!(is_live(&c(false, false)));
        assert!(is_live(&c(true, false)));
        assert!(!is_live(&c(true, true)));
        assert!(!is_live(&c(false, true)));
    }

    #[test]
    fn kind_serde_and_default() {
        assert_eq!(
            serde_json::to_string(&TaskKind::ParentTask).unwrap(),
            "\"parent_task\""
        );
        assert_eq!(
            serde_json::from_str::<TaskKind>("\"subtask\"").unwrap(),
            TaskKind::Subtask
        );
        assert_eq!(TaskKind::default(), TaskKind::Normal);
    }

    #[test]
    fn new_subtask_refusals() {
        let parent = task("P-1", TaskKind::ParentTask, Some("app"));
        assert_eq!(check_new_subtask(Some(&parent), false, None), Ok(()));
        assert_eq!(check_new_subtask(Some(&parent), false, Some("app")), Ok(()));
        assert_eq!(
            check_new_subtask(None, false, None),
            Err(Refusal::NotAParentTask)
        );
        let normal = task("P-2", TaskKind::Normal, Some("app"));
        assert_eq!(
            check_new_subtask(Some(&normal), false, None),
            Err(Refusal::NotAParentTask)
        );
        let sub = task("P-3", TaskKind::Subtask, Some("app"));
        assert_eq!(
            check_new_subtask(Some(&sub), false, None),
            Err(Refusal::NotAParentTask)
        );
        assert_eq!(
            check_new_subtask(Some(&parent), true, None),
            Err(Refusal::ParentTaskArchived)
        );
        assert_eq!(
            check_new_subtask(Some(&parent), false, Some("other")),
            Err(Refusal::ProjectMismatch)
        );
    }

    #[test]
    fn make_parent_task_refusals() {
        let mut t = task("P-1", TaskKind::Normal, None);
        assert_eq!(check_make_parent_task(&t, false, false), Ok(()));
        assert_eq!(
            check_make_parent_task(&t, true, false),
            Err(Refusal::HasOpenPr)
        );
        assert_eq!(
            check_make_parent_task(&t, false, true),
            Err(Refusal::HasSession)
        );
        t.in_checkout = Some(true);
        assert_eq!(
            check_make_parent_task(&t, false, false),
            Err(Refusal::InCheckout)
        );
        let sub = task("P-2", TaskKind::Subtask, None);
        assert_eq!(
            check_make_parent_task(&sub, false, false),
            Err(Refusal::Nesting)
        );
    }

    #[test]
    fn attach_refusals() {
        let parent = task("P-1", TaskKind::ParentTask, Some("app"));
        let t = task("P-2", TaskKind::Normal, Some("app"));
        assert_eq!(check_attach(&t, &parent, false, false), Ok(()));
        let unset = task("P-3", TaskKind::Normal, None);
        assert_eq!(check_attach(&unset, &parent, false, false), Ok(()));
        assert_eq!(
            check_attach(&t, &parent, true, false),
            Err(Refusal::ParentTaskArchived)
        );
        assert_eq!(
            check_attach(&t, &parent, false, true),
            Err(Refusal::AlreadyStarted)
        );
        let normal = task("P-4", TaskKind::Normal, Some("app"));
        assert_eq!(
            check_attach(&t, &normal, false, false),
            Err(Refusal::NotAParentTask)
        );
        let other = task("P-5", TaskKind::Normal, Some("other"));
        assert_eq!(
            check_attach(&other, &parent, false, false),
            Err(Refusal::ProjectMismatch)
        );
        let pt = task("P-6", TaskKind::ParentTask, Some("app"));
        assert_eq!(
            check_attach(&pt, &parent, false, false),
            Err(Refusal::Nesting)
        );
        let sub = task("P-7", TaskKind::Subtask, Some("app"));
        assert_eq!(
            check_attach(&sub, &parent, false, false),
            Err(Refusal::Nesting)
        );
        let mut co = task("P-8", TaskKind::Normal, Some("app"));
        co.in_checkout = Some(true);
        assert_eq!(
            check_attach(&co, &parent, false, false),
            Err(Refusal::InCheckout)
        );
    }

    #[test]
    fn start_subtask_needs_parent_workspace() {
        assert_eq!(check_start_subtask(true), Ok(()));
        assert_eq!(
            check_start_subtask(false),
            Err(Refusal::ParentTaskNotStarted)
        );
    }
}
