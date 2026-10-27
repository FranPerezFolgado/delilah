//! Integration of subtask work into the parent branch.
//!
//! Which base a task's branch starts from, how far a parent task is, which subtasks are
//! live and how its PR is merged. Only store queries here: no `gh`, no git.

use delilah_core::hierarchy::{self, ChildFacts, Progress, Refusal, TaskKind};
use delilah_core::{ProjectSettings, Task, TaskKey};
use sqlx::Row;

use crate::pr_types::{MergeMethod, PrState, Snapshot};
use crate::store::{Store, StoreError};

/// What a task's branch starts from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Base {
    /// The project's default branch.
    Default,
    /// The branch of the parent task's workspace.
    Branch(String),
}

#[derive(Debug, thiserror::Error)]
pub enum IntegrationError {
    #[error("refused: {0:?}")]
    Refused(Refusal),
    #[error(transparent)]
    Store(#[from] StoreError),
}

impl From<sqlx::Error> for IntegrationError {
    fn from(e: sqlx::Error) -> Self {
        Self::Store(e.into())
    }
}

/// A subtask's PR as the summaries show it: the latest one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubtaskPr {
    pub url: String,
    /// `None` until a check has read the PR.
    pub state: Option<PrState>,
}

/// One subtask with what progress needs to know and what its summary shows.
#[derive(Debug, Clone)]
pub struct SubtaskFact {
    pub task: Task,
    pub facts: ChildFacts,
    pub pr: Option<SubtaskPr>,
}

/// The base of `task`'s branch: a subtask starts from its parent task's branch.
pub async fn base_for(store: &Store, task: &Task) -> Result<Base, IntegrationError> {
    if task.kind != TaskKind::Subtask {
        return Ok(Base::Default);
    }
    let parent = task.parent_task.as_ref().ok_or(Refusal::NotAParentTask);
    let parent = parent.map_err(IntegrationError::Refused)?;
    let workspace = store.workspace(parent).await?;
    hierarchy::check_start_subtask(workspace.is_some()).map_err(IntegrationError::Refused)?;
    Ok(Base::Branch(workspace.expect("checked above").1.branch))
}

/// How a task of a hierarchy kind starts (R4-R7): always in its own worktree, named at
/// start, and for a subtask from the parent task's branch and submodules. A parent task
/// gets a workspace and a pushed branch but no agent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StartPlan {
    pub in_checkout: bool,
    pub branchless: bool,
    /// The parent task's branch (a subtask only): the start is `origin/<base>`.
    pub base: Option<String>,
    /// The parent task's submodules (a subtask only).
    pub submodules: Option<Vec<String>>,
    pub launch_agent: bool,
}

/// The start rules of `task`; `None` for a normal task, which starts as it always did.
/// A subtask whose parent task has no workspace is refused (`ParentTaskNotStarted`).
pub async fn start_rules(
    store: &Store,
    task: &Task,
) -> Result<Option<StartPlan>, IntegrationError> {
    let plan = |base, submodules, launch_agent| StartPlan {
        in_checkout: false,
        branchless: false,
        base,
        submodules,
        launch_agent,
    };
    match task.kind {
        TaskKind::Normal => Ok(None),
        TaskKind::ParentTask => Ok(Some(plan(None, None, false))),
        TaskKind::Subtask => {
            let parent = task.parent_task.as_ref().ok_or(Refusal::NotAParentTask);
            let parent = parent.map_err(IntegrationError::Refused)?;
            let workspace = store.workspace(parent).await?;
            hierarchy::check_start_subtask(workspace.is_some())
                .map_err(IntegrationError::Refused)?;
            let ws = workspace.expect("checked above").1;
            Ok(Some(plan(Some(ws.branch), Some(ws.submodules), true)))
        }
    }
}

/// The subtasks of `parent` with their facts: one task query, one PR query and one
/// workspace query, whatever the number of children.
pub async fn subtask_facts(
    store: &Store,
    parent: &TaskKey,
) -> Result<Vec<SubtaskFact>, IntegrationError> {
    let tasks = store.subtasks_of(parent).await?;
    if tasks.is_empty() {
        return Ok(Vec::new());
    }
    let prs = sqlx::query(
        "SELECT p.task_key, p.url, p.snapshot FROM pull_requests p
         JOIN tasks t ON t.key = p.task_key WHERE t.parent_task = ? ORDER BY p.id",
    )
    .bind(parent.as_str())
    .fetch_all(&store.pool)
    .await?;
    let mut by_task: std::collections::HashMap<String, Vec<SubtaskPr>> = Default::default();
    for row in &prs {
        let snapshot: Option<String> = row.try_get("snapshot")?;
        let state = match snapshot {
            Some(s) => Some(
                serde_json::from_str::<Snapshot>(&s)
                    .map_err(StoreError::from)?
                    .state,
            ),
            None => None,
        };
        by_task
            .entry(row.try_get("task_key")?)
            .or_default()
            .push(SubtaskPr {
                url: row.try_get("url")?,
                state,
            });
    }
    let with_workspace: std::collections::HashSet<String> = sqlx::query_scalar(
        "SELECT w.task_key FROM workspaces w
         JOIN tasks t ON t.key = w.task_key WHERE t.parent_task = ?",
    )
    .bind(parent.as_str())
    .fetch_all(&store.pool)
    .await?
    .into_iter()
    .collect();
    Ok(tasks
        .into_iter()
        .map(|task| {
            let prs = by_task.remove(task.key.as_str()).unwrap_or_default();
            let facts = ChildFacts {
                merged: !prs.is_empty() && prs.iter().all(|p| p.state == Some(PrState::Merged)),
                archived: task.status.is_terminal() && !with_workspace.contains(task.key.as_str()),
            };
            SubtaskFact {
                task,
                facts,
                pr: prs.into_iter().last(),
            }
        })
        .collect())
}

/// "N/M subtasks merged" of a parent task.
pub async fn progress(store: &Store, parent: &TaskKey) -> Result<Progress, IntegrationError> {
    let facts: Vec<_> = subtask_facts(store, parent)
        .await?
        .into_iter()
        .map(|f| f.facts)
        .collect();
    Ok(hierarchy::progress(&facts))
}

/// The keys of the subtasks not archived yet.
pub async fn live_subtasks(
    store: &Store,
    parent: &TaskKey,
) -> Result<Vec<TaskKey>, IntegrationError> {
    Ok(subtask_facts(store, parent)
        .await?
        .into_iter()
        .filter(|f| hierarchy::is_live(&f.facts))
        .map(|f| f.task.key)
        .collect())
}

/// The method Merge uses for a PR of a task of `kind`: subtasks squash into the parent
/// branch, a parent task follows the project's setting, the rest take the first the
/// repository allows. A preference the repository does not allow falls back to that
/// first one; `None` when it allows none.
pub fn preferred_method(
    kind: TaskKind,
    settings: &ProjectSettings,
    allowed: &[MergeMethod],
) -> Option<MergeMethod> {
    let wanted = match kind {
        TaskKind::Subtask => Some(MergeMethod::Squash),
        // `MergePreference` is not exported by delilah-core's root; its serde name
        // (lowercase) is the same as `MergeMethod`'s (kebab-case) for these three.
        TaskKind::ParentTask => serde_json::to_value(settings.parent_task_merge)
            .ok()
            .and_then(|v| serde_json::from_value(v).ok()),
        TaskKind::Normal => None,
    };
    wanted
        .filter(|m| allowed.contains(m))
        .or_else(|| allowed.first().copied())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prs::tests::snapshot;
    use crate::store_prs::NewPr;
    use delilah_core::{ProjectKind, TaskStatus};
    use delilah_git::Workspace;

    async fn store() -> Store {
        let store = Store::in_memory().await.unwrap();
        let mut p = Task::new(TaskKey::new("P-1"), "parent", "app");
        p.kind = TaskKind::ParentTask;
        store.create_task(&p).await.unwrap();
        store
    }

    async fn child(store: &Store, key: &str) -> Task {
        let mut t = Task::new(TaskKey::new(key), "child", "app");
        t.kind = TaskKind::Subtask;
        t.parent_task = Some(TaskKey::new("P-1"));
        store.create_task(&t).await.unwrap();
        t
    }

    async fn workspace(store: &Store, key: &str, branch: &str) {
        store
            .save_workspace(
                &TaskKey::new(key),
                "/tmp/parent".as_ref(),
                &Workspace {
                    path: "/tmp/ws".into(),
                    branch: branch.into(),
                    kind: ProjectKind::Repo,
                    submodules: vec![],
                },
            )
            .await
            .unwrap();
    }

    async fn pr(store: &Store, key: &str, n: i64, state: Option<PrState>) {
        let row = store
            .link_pr(
                &TaskKey::new(key),
                &NewPr {
                    repo: format!("repo{n}"),
                    host: "github.com".into(),
                    owner_name: "me/repo".into(),
                    number: n,
                    url: format!("https://github.com/me/repo/pull/{n}"),
                    linked_by: "detected".into(),
                },
            )
            .await
            .unwrap();
        if let Some(state) = state {
            store
                .save_check(
                    row.id,
                    &snapshot(state, vec![]),
                    &Default::default(),
                    chrono::Utc::now(),
                    &[],
                    &[],
                )
                .await
                .unwrap();
        }
    }

    async fn finish(store: &Store, key: &str) {
        sqlx::query("UPDATE tasks SET status = ? WHERE key = ?")
            .bind(serde_json::to_string(&TaskStatus::Done).unwrap())
            .bind(key)
            .execute(&store.pool)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn base_is_default_for_normal_and_parent_tasks() {
        let store = store().await;
        let parent = store.get_task(&TaskKey::new("P-1")).await.unwrap().unwrap();
        assert_eq!(base_for(&store, &parent).await.unwrap(), Base::Default);
        let normal = Task::new(TaskKey::new("N-1"), "n", "app");
        assert_eq!(base_for(&store, &normal).await.unwrap(), Base::Default);
    }

    #[tokio::test]
    async fn base_of_a_subtask_is_the_parent_branch_or_an_error() {
        let store = store().await;
        let sub = child(&store, "C-1").await;
        assert!(matches!(
            base_for(&store, &sub).await,
            Err(IntegrationError::Refused(Refusal::ParentTaskNotStarted))
        ));
        workspace(&store, "P-1", "feature/P-1").await;
        assert_eq!(
            base_for(&store, &sub).await.unwrap(),
            Base::Branch("feature/P-1".into())
        );
    }

    #[tokio::test]
    async fn start_rules_by_kind() {
        let store = store().await;
        let normal = Task::new(TaskKey::new("N-1"), "n", "app");
        assert_eq!(start_rules(&store, &normal).await.unwrap(), None);
        let parent = store.get_task(&TaskKey::new("P-1")).await.unwrap().unwrap();
        let plan = start_rules(&store, &parent).await.unwrap().unwrap();
        assert!(!plan.launch_agent && !plan.in_checkout && !plan.branchless);
        assert_eq!((plan.base, plan.submodules), (None, None));
        let sub = child(&store, "C-1").await;
        assert!(matches!(
            start_rules(&store, &sub).await,
            Err(IntegrationError::Refused(Refusal::ParentTaskNotStarted))
        ));
        workspace(&store, "P-1", "feature/P-1").await;
        let plan = start_rules(&store, &sub).await.unwrap().unwrap();
        assert!(plan.launch_agent && !plan.in_checkout && !plan.branchless);
        assert_eq!(plan.base.as_deref(), Some("feature/P-1"));
        assert_eq!(plan.submodules, Some(vec![]));
    }

    #[tokio::test]
    async fn progress_counts_merged_archived_and_missing_children() {
        let store = store().await;
        let parent = TaskKey::new("P-1");
        assert!(!progress(&store, &parent).await.unwrap().ready);
        for k in ["C-1", "C-2", "C-3", "C-4"] {
            child(&store, k).await;
        }
        // C-1: all PRs merged. C-2: one merged, one open. C-3: dropped. C-4: no PR.
        pr(&store, "C-1", 1, Some(PrState::Merged)).await;
        pr(&store, "C-2", 2, Some(PrState::Merged)).await;
        pr(&store, "C-2", 3, Some(PrState::Open)).await;
        finish(&store, "C-3").await;
        let p = progress(&store, &parent).await.unwrap();
        assert_eq!((p.merged, p.counted, p.dropped, p.ready), (1, 3, 1, false));
        // A finished child with a workspace is not archived yet.
        finish(&store, "C-4").await;
        workspace(&store, "C-4", "feature/C-4").await;
        let facts = subtask_facts(&store, &parent).await.unwrap();
        assert!(
            !facts
                .iter()
                .find(|f| f.task.key.as_str() == "C-4")
                .unwrap()
                .facts
                .archived
        );
        assert!(
            facts
                .iter()
                .find(|f| f.task.key.as_str() == "C-3")
                .unwrap()
                .facts
                .archived
        );
        let c2 = facts.iter().find(|f| f.task.key.as_str() == "C-2").unwrap();
        assert!(!c2.facts.merged);
        assert_eq!(c2.pr.as_ref().unwrap().state, Some(PrState::Open));
    }

    #[tokio::test]
    async fn deleted_children_are_absent_and_all_merged_is_ready() {
        let store = store().await;
        let parent = TaskKey::new("P-1");
        child(&store, "C-1").await;
        child(&store, "C-2").await;
        pr(&store, "C-1", 1, Some(PrState::Merged)).await;
        store.delete_task(&TaskKey::new("C-2")).await.unwrap();
        let p = progress(&store, &parent).await.unwrap();
        assert_eq!((p.merged, p.counted, p.ready), (1, 1, true));
    }

    #[tokio::test]
    async fn live_subtasks_are_the_not_archived_ones() {
        let store = store().await;
        let parent = TaskKey::new("P-1");
        child(&store, "C-1").await;
        child(&store, "C-2").await;
        child(&store, "C-3").await;
        finish(&store, "C-2").await;
        workspace(&store, "C-3", "feature/C-3").await;
        finish(&store, "C-3").await;
        let live: Vec<_> = live_subtasks(&store, &parent)
            .await
            .unwrap()
            .iter()
            .map(|k| k.to_string())
            .collect();
        assert_eq!(live, ["C-1", "C-3"]);
    }

    #[test]
    fn preferred_method_by_kind() {
        use MergeMethod::*;
        let mut s = ProjectSettings::default();
        let all = [Merge, Squash, Rebase];
        assert_eq!(preferred_method(TaskKind::Subtask, &s, &all), Some(Squash));
        assert_eq!(
            preferred_method(TaskKind::Normal, &s, &[Rebase, Merge]),
            Some(Rebase)
        );
        assert_eq!(
            preferred_method(TaskKind::ParentTask, &s, &all),
            Some(Merge)
        );
        s.parent_task_merge = serde_json::from_str("\"rebase\"").unwrap();
        assert_eq!(
            preferred_method(TaskKind::ParentTask, &s, &all),
            Some(Rebase)
        );
        // Not allowed: the first allowed.
        assert_eq!(
            preferred_method(TaskKind::ParentTask, &s, &[Squash, Merge]),
            Some(Squash)
        );
        assert_eq!(
            preferred_method(TaskKind::Subtask, &s, &[Merge]),
            Some(Merge)
        );
        assert_eq!(preferred_method(TaskKind::Subtask, &s, &[]), None);
    }
}
