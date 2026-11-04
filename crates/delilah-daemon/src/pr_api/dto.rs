//! What the PR routes send and receive.

use chrono::{DateTime, Utc};
use delilah_core::hierarchy::TaskKind;
use delilah_core::{ProjectSettings, TaskKey};
use serde::{Deserialize, Serialize};

use crate::api::ApiError;
use crate::autofix::{self, PrAutofix, State, StopReason};
use crate::integration::{self, Base};
use crate::pr_types::{
    Check, MergeMethod, PrState, ReviewComment, ReviewDecision, Rollup, Snapshot,
};
use crate::prs;
use crate::state::AppState;
use crate::store_prs::PrRow;

#[derive(Serialize)]
pub struct SnapshotView {
    state: PrState,
    draft: bool,
    title: String,
    head: String,
    base: String,
    checks: Vec<Check>,
    ci: Rollup,
    review_decision: Option<ReviewDecision>,
    comments: usize,
    review_comments: Vec<ReviewComment>,
    merge_methods: Vec<MergeMethod>,
}

#[derive(Serialize)]
pub struct PrView {
    id: i64,
    repo: String,
    host: String,
    owner_name: String,
    number: i64,
    url: String,
    linked_by: String,
    /// The id of the PR that replaced this closed one.
    replaced_by: Option<i64>,
    /// The PR's head branch, from its snapshot.
    head_branch: Option<String>,
    checked_at: Option<DateTime<Utc>>,
    error: Option<String>,
    snapshot: Option<SnapshotView>,
    /// `null` until CI autofix first acts on the PR.
    autofix: Option<PrAutofixView>,
    /// What Merge will use; `null` while the repository's methods are unknown.
    method: Option<MergeMethod>,
    /// The branch the PR targets (`null` until it is read).
    base_branch: Option<String>,
    /// The branch it should target: the parent task's for a subtask, else `null`.
    expected_base: Option<String>,
}

/// What a task's PRs need beyond the PR itself: its kind and project settings (the merge
/// method) and, for a subtask, the parent task's branch.
pub(super) struct TaskContext {
    kind: TaskKind,
    settings: ProjectSettings,
    expected_base: Option<String>,
}

impl TaskContext {
    pub(super) async fn of(app: &AppState, key: &TaskKey) -> Result<Self, ApiError> {
        let Some(task) = app.store.get_task(key).await? else {
            return Ok(Self {
                kind: TaskKind::Normal,
                settings: ProjectSettings::default(),
                expected_base: None,
            });
        };
        let settings = match &task.project {
            Some(name) => app
                .store
                .project(name)
                .await
                .map(|p| p.settings)
                .unwrap_or_default(),
            None => ProjectSettings::default(),
        };
        // A subtask whose parent has no workspace has no expected base to compare.
        let expected_base = match integration::base_for(&app.store, &task).await {
            Ok(Base::Branch(b)) => Some(b),
            _ => None,
        };
        Ok(Self {
            kind: task.kind,
            settings,
            expected_base,
        })
    }

    /// The method Merge uses when the request names none.
    pub(super) fn method(&self, allowed: &[MergeMethod]) -> Option<MergeMethod> {
        integration::preferred_method(self.kind, &self.settings, allowed)
    }
}

/// What clients see of a PR's autofix state: the internal `session` stays out.
#[derive(Serialize)]
pub struct PrAutofixView {
    state: State,
    attempts: u32,
    limit: u32,
    sha: Option<String>,
    sent_at: Option<String>,
    reason: Option<StopReason>,
}

#[derive(Serialize)]
pub struct Problem {
    pub(super) kind: String,
    pub(super) message: String,
}

#[derive(Serialize)]
pub struct TaskPrs {
    pub(super) account: Option<String>,
    pub(super) problem: Option<Problem>,
    pub(super) prs: Vec<PrView>,
}

#[derive(Deserialize)]
pub struct LinkRequest {
    pub(super) url: String,
}

#[derive(Deserialize)]
pub struct ProposalQuery {
    pub(super) repo: Option<String>,
}

#[derive(Serialize)]
pub struct Proposal {
    pub(super) repo: String,
    pub(super) title: String,
    pub(super) body: String,
    pub(super) base: String,
    pub(super) needs_push: bool,
    /// The task's repos that have no open PR yet.
    pub(super) repos: Vec<String>,
}

#[derive(Deserialize)]
pub struct OpenRequest {
    #[serde(default)]
    pub(super) repo: Option<String>,
    pub(super) title: String,
    #[serde(default)]
    pub(super) body: String,
    #[serde(default)]
    pub(super) draft: bool,
    /// Parent task only: commit `chore: start <KEY> <title>` when the branch has none.
    #[serde(default)]
    pub(super) empty_commit: bool,
}

impl From<&Snapshot> for SnapshotView {
    fn from(s: &Snapshot) -> Self {
        Self {
            state: s.state,
            draft: s.draft,
            title: s.title.clone(),
            head: s.head.clone(),
            base: s.base.clone(),
            checks: s.checks.clone(),
            ci: prs::ci_outcome(&s.checks),
            review_decision: s.review_decision,
            comments: s.comments.len(),
            review_comments: s.review_comments.clone(),
            merge_methods: s.merge_methods.clone(),
        }
    }
}

impl From<&PrAutofix> for PrAutofixView {
    fn from(a: &PrAutofix) -> Self {
        Self {
            state: a.state,
            attempts: a.attempts,
            limit: autofix::LIMIT,
            sha: a.sha.clone(),
            sent_at: a.sent_at.clone(),
            reason: a.reason,
        }
    }
}

impl PrView {
    /// The PR with what the task's kind adds: the merge method and the expected base.
    pub(super) fn with(row: &PrRow, ctx: &TaskContext) -> Self {
        let method = row
            .snapshot
            .as_ref()
            .and_then(|s| ctx.method(&s.merge_methods));
        Self {
            method,
            expected_base: ctx.expected_base.clone(),
            ..Self::from(row)
        }
    }
}

impl From<&PrRow> for PrView {
    fn from(row: &PrRow) -> Self {
        Self {
            method: None,
            base_branch: row.snapshot.as_ref().map(|s| s.base.clone()),
            expected_base: None,
            id: row.id,
            repo: row.repo.clone(),
            host: row.host.clone(),
            owner_name: row.owner_name.clone(),
            number: row.number,
            url: row.url.clone(),
            linked_by: row.linked_by.clone(),
            replaced_by: row.replaced_by,
            head_branch: row.snapshot.as_ref().and_then(|s| s.head_branch.clone()),
            checked_at: row.checked_at,
            error: row.error.clone(),
            snapshot: row.snapshot.as_ref().map(SnapshotView::from),
            autofix: row.autofix.as_ref().map(PrAutofixView::from),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pr_api::support::{call, fake, setup};
    use crate::pr_loop;
    use delilah_core::TaskKey;

    #[tokio::test]
    async fn each_pr_carries_replaced_by_and_head_branch_and_comes_ordered() {
        let (router, app, _dir) = setup(fake()).await;
        let key = TaskKey::new("PROJ-1");
        pr_loop::check_task(&app, &key, true).await;
        let new = |repo: &str, n: i64, by: &str| crate::store_prs::NewPr {
            repo: repo.into(),
            host: "github.com".into(),
            owner_name: "me/other".into(),
            number: n,
            url: format!("https://github.com/me/other/pull/{n}"),
            linked_by: by.into(),
        };
        app.store
            .link_pr(&key, &new("github.com/me/other", 90, "agent"))
            .await
            .unwrap();
        app.store
            .link_pr(&key, &new(".", 91, "agent"))
            .await
            .unwrap();
        let (_, v) = call(&router, "GET", "/api/v1/tasks/PROJ-1/prs", None).await;
        let prs = v["prs"].as_array().unwrap();
        assert_eq!(prs.first().unwrap()["repo"], ".");
        assert_eq!(prs.last().unwrap()["repo"], "github.com/me/other");
        assert_eq!(prs.last().unwrap()["linked_by"], "agent");
        for p in prs {
            let o = p.as_object().unwrap();
            assert!(o.contains_key("replaced_by") && o.contains_key("head_branch"));
        }
    }

    #[tokio::test]
    async fn each_pr_carries_autofix_without_the_session_and_null_when_it_never_acted() {
        let (router, app, _dir) = setup(fake()).await;
        let key = TaskKey::new("PROJ-1");
        pr_loop::check_task(&app, &key, true).await;
        let (s, v) = call(&router, "GET", "/api/v1/tasks/PROJ-1/prs", None).await;
        assert_eq!(s, axum::http::StatusCode::OK);
        assert!(v["prs"][0]["autofix"].is_null());
        assert!(v["prs"][0].as_object().unwrap().contains_key("autofix"));

        let pr = app.store.prs_of(&key).await.unwrap().remove(0);
        let a = PrAutofix {
            state: State::Stopped,
            attempts: 3,
            sha: Some("abc".into()),
            session: Some("s-1".into()),
            sent_at: Some("2026-01-01T00:00:00Z".into()),
            reason: Some(StopReason::Limit),
        };
        assert!(
            app.store
                .update_pr_autofix(pr.id, None, Some(&a))
                .await
                .unwrap()
        );
        let (_, v) = call(&router, "GET", "/api/v1/tasks/PROJ-1/prs", None).await;
        assert_eq!(
            v["prs"][0]["autofix"],
            serde_json::json!({
                "state": "stopped", "attempts": 3, "limit": 3, "sha": "abc",
                "sent_at": "2026-01-01T00:00:00Z", "reason": "limit",
            })
        );
    }
}
