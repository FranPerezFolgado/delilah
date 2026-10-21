//! The PR loop: finds the PR of each task's branch, checks the open ones on a timer and
//! after the agent pushes (see `prs.rs` for the pure logic, `gh` for GitHub).

use std::collections::HashSet;
use std::time::{Duration, Instant};

use chrono::Utc;
use delilah_core::{SpecPhase, TaskKey};
use tokio::time::MissedTickBehavior;

use crate::events::Topic;
use crate::gh::{AccessProblem, GhError, classify};
use crate::phases;
use crate::pr_types::PrState;
use crate::state::AppState;
use crate::store_prs::{NewPr, PrRow};

const INTERVAL: Duration = Duration::from_secs(30);
/// How long after a push or `gh pr create` the PR is looked for: GitHub needs a moment.
const NUDGE_DELAY: Duration = if cfg!(test) {
    Duration::from_millis(20)
} else {
    Duration::from_secs(15)
};

/// Checks the PRs on a timer: the open ones, and the tasks in review that have none yet.
pub fn spawn(app: AppState) {
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(INTERVAL);
        ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);
        loop {
            ticker.tick().await;
            tick(&app).await;
        }
    });
}

async fn tick(app: &AppState) {
    let mut keys: Vec<String> = Vec::new();
    match app.store.open_prs().await {
        Ok(rows) => keys.extend(rows.into_iter().map(|r| r.task_key)),
        Err(e) => return tracing::warn!("pr loop: {e}"),
    }
    match app.store.list_tasks().await {
        Ok(tasks) => {
            for task in tasks {
                if task.phase.as_deref() != Some(SpecPhase::Review.name())
                    || task.status.is_terminal()
                {
                    continue;
                }
                if app
                    .store
                    .prs_of(&task.key)
                    .await
                    .is_ok_and(|rows| rows.is_empty())
                {
                    keys.push(task.key.as_str().to_string());
                }
            }
        }
        Err(e) => tracing::warn!("pr loop: {e}"),
    }
    let mut seen = HashSet::new();
    for key in keys.into_iter().filter(|k| seen.insert(k.clone())) {
        check_task(app, &TaskKey::new(key), false).await;
    }
}

/// The agent just pushed or opened a PR: look for it soon, ignoring any back-off.
pub fn nudge(app: &AppState, key: &TaskKey) {
    let app = app.clone();
    let key = key.clone();
    tokio::spawn(async move {
        tokio::time::sleep(NUDGE_DELAY).await;
        check_task(&app, &key, true).await;
    });
}

/// One check of a task: finds the PR of its branch in each repo that has none, then
/// reads every open PR. `force` also searches below review and ignores the back-off.
pub async fn check_task(app: &AppState, key: &TaskKey, force: bool) {
    let Ok(Some(task)) = app.store.get_task(key).await else {
        return;
    };
    let project = task.project.clone().unwrap_or_default();
    let in_review = task
        .phase
        .as_deref()
        .and_then(SpecPhase::parse)
        .is_some_and(|p| p >= SpecPhase::Review);
    let mut changed = false;

    if let Ok(Some((_, ws))) = app.store.workspace(key).await {
        let rows = app.store.prs_of(key).await.unwrap_or_default();
        for (repo, dir) in ws.repos() {
            let has_row = rows.iter().any(|r| r.repo == repo);
            if !force && (has_row || !in_review) {
                continue;
            }
            if let Ok(linked) = detect(app, key, &project, &repo, &dir, &ws.branch, force).await {
                changed |= linked;
            }
        }
    }

    for row in app.store.prs_of(key).await.unwrap_or_default() {
        let open = row
            .snapshot
            .as_ref()
            .is_none_or(|s| s.state == PrState::Open);
        if open || force {
            changed |= snapshot(app, &project, &row, force).await;
        }
    }
    if changed {
        app.emit(Topic::Prs);
    }
}

/// Looks for the PR of `branch` in one repo and links it. Whether the rows changed;
/// `Err` when `gh` failed (already reported).
async fn detect(
    app: &AppState,
    key: &TaskKey,
    project: &str,
    repo: &str,
    dir: &std::path::Path,
    branch: &str,
    force: bool,
) -> Result<bool, ()> {
    let dir_key = format!("dir:{}", dir.display());
    if !force && !due(app, &dir_key) {
        return Err(());
    }
    let info = match app.gh.repo_view(dir).await {
        Ok(info) => info,
        Err(e) => {
            failed(app, project, &dir_key, &e, force).await;
            return Err(());
        }
    };
    let repo_key = format!("{}/{}", info.host, info.owner_name);
    if !force && !due(app, &repo_key) {
        return Err(());
    }
    let listed = match app.gh.pr_list(dir, branch).await {
        Ok(listed) => listed,
        Err(e) => {
            failed(app, project, &repo_key, &e, force).await;
            return Err(());
        }
    };
    succeeded(app, project, &[&dir_key, &repo_key]).await;
    let Some(pr) = crate::prs::pick(&listed) else {
        return Ok(false);
    };
    if app.store.is_unlinked(key, &pr.url).await.unwrap_or(false) {
        return Ok(false);
    }
    let existing = app.store.prs_of(key).await.unwrap_or_default();
    if existing.iter().any(|r| r.repo == repo && r.url == pr.url) {
        return Ok(false);
    }
    let new = NewPr {
        repo: repo.to_string(),
        host: info.host,
        owner_name: info.owner_name,
        number: pr.number,
        url: pr.url.clone(),
        linked_by: "detected".into(),
    };
    match app.store.link_pr(key, &new).await {
        Ok(_) => {
            phases::advance(app, key, SpecPhase::Review).await;
            Ok(true)
        }
        Err(e) => {
            tracing::warn!(%key, "could not link the PR: {e}");
            Err(())
        }
    }
}

/// Reads a linked PR and saves what it found. Whether anything shown changed.
async fn snapshot(app: &AppState, project: &str, row: &PrRow, force: bool) -> bool {
    let backoff_key = format!("{}/{}", row.host, row.owner_name);
    if !force && !due(app, &backoff_key) {
        return false;
    }
    match app
        .gh
        .pr_snapshot(&row.host, &row.owner_name, row.number)
        .await
    {
        Ok(snap) => {
            succeeded(app, project, &[&backoff_key]).await;
            let changed = row.snapshot.as_ref() != Some(&snap) || row.error.is_some();
            // The notices come with the next package: the marks stay as they were.
            if let Err(e) = app
                .store
                .save_check(row.id, &snap, &row.marks, Utc::now(), &[], &[])
                .await
            {
                tracing::warn!(task = %row.task_key, "could not save the PR check: {e}");
                return false;
            }
            changed
        }
        Err(e) => {
            failed(app, project, &backoff_key, &e, force).await;
            let text = e.to_string();
            if row.error.as_deref() == Some(text.as_str()) {
                return false;
            }
            app.store.set_pr_error(row.id, Some(&text)).await.is_ok()
        }
    }
}

fn due(app: &AppState, key: &str) -> bool {
    app.pr_backoff
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .due(key, Instant::now())
}

/// `gh` failed for `backoff_key`: back off, and tell the user once if it is something
/// they can fix.
async fn failed(app: &AppState, project: &str, backoff_key: &str, err: &GhError, force: bool) {
    let problem = classify(err);
    let now = Instant::now();
    let reset = match &problem {
        AccessProblem::RateLimited { reset: Some(at) } => {
            Some(now + (*at - Utc::now()).to_std().unwrap_or_default())
        }
        _ => None,
    };
    if !force || reset.is_some() {
        app.pr_backoff
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .fail(backoff_key, now, reset);
    }
    let Some((title, body)) = access_message(app, project, &problem) else {
        return;
    };
    let kind = problem.kind().to_string();
    let flag = format!("pr_access:{project}");
    let current: Option<Option<String>> = app.store.setting(&flag).await.ok().flatten();
    if current.flatten().as_deref() == Some(kind.as_str()) {
        return;
    }
    if app.store.set_setting(&flag, &Some(kind)).await.is_err() {
        return;
    }
    // With the app open the PR tab shows the problem; the notification is for when it is not.
    if !app.events.listening() {
        app.notifier.send(title, body);
    }
}

/// What to tell the user about an access problem; `None` for the ones that pass alone.
fn access_message(
    app: &AppState,
    project: &str,
    problem: &AccessProblem,
) -> Option<(String, String)> {
    let lang = app.lang();
    let body = match problem {
        AccessProblem::Missing => lang.pick(
            "Instala la CLI de GitHub (gh).",
            "Install the GitHub CLI (gh).",
        ),
        AccessProblem::SignedOut => lang.pick(
            "Inicia sesión con `gh auth login`.",
            "Sign in with `gh auth login`.",
        ),
        AccessProblem::NoAccess => lang.pick(
            "Tu cuenta de gh no tiene acceso al repositorio.",
            "Your gh account has no access to the repository.",
        ),
        AccessProblem::UnsupportedRemote => lang.pick(
            "El remoto de git no es de GitHub.",
            "The git remote is not a GitHub one.",
        ),
        AccessProblem::Network | AccessProblem::RateLimited { .. } => return None,
    };
    let title = format!(
        "{} {project}",
        lang.pick("PRs no disponibles en", "PRs unavailable in")
    );
    Some((title, body.to_string()))
}

/// `gh` answered: forget the back-off and any access problem of the project.
async fn succeeded(app: &AppState, project: &str, backoff_keys: &[&str]) {
    {
        let mut backoff = app
            .pr_backoff
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        for key in backoff_keys {
            backoff.ok(key);
        }
    }
    let flag = format!("pr_access:{project}");
    let current: Option<Option<String>> = app.store.setting(&flag).await.ok().flatten();
    if current.flatten().is_some() {
        let _ = app.store.set_setting(&flag, &None::<String>).await;
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;
    use std::sync::Arc;

    use delilah_core::{Event, Project, ProjectKind, SpecPhase, Task};
    use delilah_git::Workspace;

    use super::*;
    use crate::config::Config;
    use crate::gh::GhError;
    use crate::gh::fake::FakeGh;
    use crate::store::Store;

    const REPO_VIEW: &str = r#"{"nameWithOwner":"o/r","url":"https://github.com/o/r","defaultBranchRef":{"name":"main"}}"#;
    const PR_LIST: &str = r#"[{"number":7,"url":"https://github.com/o/r/pull/7","state":"OPEN","updatedAt":"2026-10-20T10:00:00Z"}]"#;
    const GRAPHQL: &str = r#"{"data":{"repository":{"pullRequest":{"state":"OPEN","isDraft":false,"merged":false,
      "headRefOid":"abc","baseRefName":"main","title":"t","url":"u","reviewDecision":null,
      "commits":{"nodes":[{"commit":{"statusCheckRollup":null}}]},
      "reviews":{"nodes":[]},"comments":{"nodes":[]},"reviewThreads":{"nodes":[]}}}}}"#;
    const PR_URL: &str = "https://github.com/o/r/pull/7";

    fn failed(stderr: &str) -> GhError {
        GhError::Failed {
            code: None,
            stderr: stderr.into(),
        }
    }

    fn happy() -> FakeGh {
        FakeGh::new()
            .on(&["repo", "view"], Ok(REPO_VIEW))
            .on(&["pr", "list"], Ok(PR_LIST))
            .on(&["api", "graphql"], Ok(GRAPHQL))
    }

    /// A task in `phase` with a workspace on `feature/PROJ-1`, talking to `fake`.
    async fn setup(fake: Arc<FakeGh>, phase: Option<SpecPhase>) -> (AppState, TaskKey) {
        let config = Config {
            home: std::env::temp_dir().join(format!("delilah-prloop-{}", uuid::Uuid::new_v4())),
            addr: "127.0.0.1:7420".parse().unwrap(),
            allow_file_protocol: false,
            notify: false,
            ui_origin: None,
        };
        let app = AppState::new(Store::in_memory().await.unwrap(), config).with_gh(fake);
        let key = TaskKey::new("PROJ-1");
        app.store
            .create_project(&Project {
                name: "proj".into(),
                path: "/parent".into(),
                kind: ProjectKind::Repo,
                workflow: "feature".into(),
            })
            .await
            .unwrap();
        let mut task = Task::new(key.clone(), "t", "feature");
        task.project = Some("proj".into());
        app.store.create_task(&task).await.unwrap();
        app.transition(&key, Event::Start).await.unwrap();
        app.transition(&key, Event::WorkspaceReady).await.unwrap();
        let ws = Workspace {
            path: std::env::temp_dir().join("delilah-prloop-ws"),
            branch: "feature/PROJ-1".into(),
            kind: ProjectKind::Repo,
            submodules: vec![],
        };
        app.store
            .save_workspace(&key, Path::new("/parent"), &ws)
            .await
            .unwrap();
        if let Some(phase) = phase {
            crate::phases::advance(&app, &key, phase).await;
        }
        (app, key)
    }

    #[tokio::test]
    async fn a_pr_from_the_branch_is_linked_snapshotted_and_announced() {
        let fake = Arc::new(happy());
        let (app, key) = setup(fake.clone(), Some(SpecPhase::Implement)).await;
        let mut rx = app.events.subscribe();
        check_task(&app, &key, true).await;

        let rows = app.store.prs_of(&key).await.unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].linked_by, "detected");
        assert_eq!(rows[0].url, PR_URL);
        assert!(rows[0].snapshot.is_some());
        assert!(rows[0].error.is_none());
        let task = app.store.get_task(&key).await.unwrap().unwrap();
        assert_eq!(task.phase.as_deref(), Some("review"));
        let mut topics = vec![];
        while let Ok(t) = rx.try_recv() {
            topics.push(t);
        }
        assert!(topics.contains(&Topic::Prs));
    }

    #[tokio::test]
    async fn an_unlinked_url_is_not_linked_again() {
        let fake = Arc::new(happy());
        let (app, key) = setup(fake, Some(SpecPhase::Review)).await;
        check_task(&app, &key, false).await;
        let row = app.store.prs_of(&key).await.unwrap().remove(0);
        app.store.unlink_pr(row.id).await.unwrap();
        check_task(&app, &key, true).await;
        assert!(app.store.prs_of(&key).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn below_review_without_a_pr_nothing_is_searched_unless_forced() {
        let fake = Arc::new(happy());
        let (app, key) = setup(fake.clone(), Some(SpecPhase::Implement)).await;
        check_task(&app, &key, false).await;
        assert!(fake.calls().is_empty());
        assert!(app.store.prs_of(&key).await.unwrap().is_empty());
        check_task(&app, &key, true).await;
        assert!(!fake.calls().is_empty());
        assert_eq!(app.store.prs_of(&key).await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn a_network_error_keeps_the_snapshot_and_backs_off() {
        let ok = Arc::new(happy());
        let (app, key) = setup(ok, Some(SpecPhase::Review)).await;
        check_task(&app, &key, false).await;
        let before = app.store.prs_of(&key).await.unwrap().remove(0);
        assert!(before.snapshot.is_some());

        let failing =
            Arc::new(FakeGh::new().on(&["api", "graphql"], Err(failed("connection reset"))));
        let app = app.with_gh(failing.clone());
        check_task(&app, &key, false).await;
        let after = app.store.prs_of(&key).await.unwrap().remove(0);
        assert_eq!(after.snapshot, before.snapshot);
        assert!(after.error.as_deref().unwrap().contains("connection reset"));
        assert_eq!(failing.calls().len(), 1);

        check_task(&app, &key, false).await;
        assert_eq!(failing.calls().len(), 1, "inside the back-off window");
        assert!(app.notifier.recorded().is_empty());
    }

    #[tokio::test]
    async fn an_access_problem_is_notified_once_and_cleared_on_success() {
        let signed_out = Arc::new(FakeGh::new().on(
            &["repo", "view"],
            Err(failed(
                "To get started with GitHub CLI, please run: gh auth login",
            )),
        ));
        let (app, key) = setup(signed_out, Some(SpecPhase::Review)).await;
        check_task(&app, &key, true).await;
        check_task(&app, &key, true).await;
        let sent = app.notifier.recorded();
        assert_eq!(sent.len(), 1);
        assert!(sent[0].title.contains("proj"));
        let flag: Option<Option<String>> = app.store.setting("pr_access:proj").await.unwrap();
        assert_eq!(flag.flatten().as_deref(), Some("signed-out"));

        let app = app.with_gh(Arc::new(happy()));
        check_task(&app, &key, true).await;
        let flag: Option<Option<String>> = app.store.setting("pr_access:proj").await.unwrap();
        assert_eq!(flag.flatten(), None);
        assert_eq!(app.store.prs_of(&key).await.unwrap().len(), 1);
    }
}
