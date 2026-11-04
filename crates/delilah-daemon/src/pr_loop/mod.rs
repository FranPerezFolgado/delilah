//! The PR loop: finds the PR of each task's branch, checks the open ones on a timer and
//! after the agent pushes (see `prs.rs` for the pure logic, `gh` for GitHub, `access.rs`
//! for the problems the user can fix).

pub(crate) mod access;
pub(crate) mod capture;
mod merge;
mod notices;

use std::collections::HashSet;
use std::path::Path;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use delilah_core::{ProjectKind, SpecPhase, TaskKey};
use tokio::time::MissedTickBehavior;

use self::access::{failed, succeeded};
use self::notices::{Checked, record};
use crate::events::Topic;
use crate::gh::{GhError, PrRef, classify, parse_pr_url};
use crate::phases;
use crate::pr_types::{PrState, Snapshot};
use crate::state::AppState;
use crate::store::StoreError;
use crate::store_prs::{NewPr, PrRow};

/// How long after a push or `gh pr create` the PR is looked for: GitHub needs a moment.
const NUDGE_DELAY: Duration = Duration::from_secs(15);

/// Why a PR could not be linked by its URL.
#[derive(Debug)]
pub(crate) enum LinkError {
    /// Not the URL of a pull request.
    NotAPr,
    /// `gh` could not read the PR.
    Gh(GhError),
    Store(StoreError),
}

impl From<StoreError> for LinkError {
    fn from(e: StoreError) -> Self {
        Self::Store(e)
    }
}

/// Marks a task as being checked; dropping it lets the next check run.
pub(crate) struct CheckGuard {
    checking: Arc<Mutex<HashSet<String>>>,
    key: String,
}

impl Drop for CheckGuard {
    fn drop(&mut self) {
        self.checking
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(&self.key);
    }
}

/// Takes the task's check; `None` when another one is running.
pub(crate) fn claim(app: &AppState, key: &TaskKey) -> Option<CheckGuard> {
    let fresh = app
        .pr_checking
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .insert(key.as_str().to_string());
    fresh.then(|| CheckGuard {
        checking: app.pr_checking.clone(),
        key: key.as_str().to_string(),
    })
}

/// Whether a check of the task is running now.
pub(crate) fn is_checking(app: &AppState, key: &TaskKey) -> bool {
    app.pr_checking
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .contains(key.as_str())
}

/// Checks the PRs on a timer: the open ones, and the tasks in review that have none yet.
pub fn spawn(app: AppState) {
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(app.config.pr_poll);
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
    nudge_after(app, key, NUDGE_DELAY);
}

/// [`nudge`] after `delay`. Nudges of a task that are waiting make one check.
pub(crate) fn nudge_after(app: &AppState, key: &TaskKey, delay: Duration) {
    let fresh = app
        .pr_nudges
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .insert(key.as_str().to_string());
    if !fresh {
        return;
    }
    let app = app.clone();
    let key = key.clone();
    tokio::spawn(async move {
        tokio::time::sleep(delay).await;
        // Out of the set before the check: a push during it asks for another.
        app.pr_nudges
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(key.as_str());
        check_task(&app, &key, true).await;
    });
}

/// One check of a task: finds the PR of its branch in each repo that has none, then
/// reads every open PR. `force` also searches below review and ignores the back-off.
/// Only one check of a task runs at a time: a second call returns at once.
pub async fn check_task(app: &AppState, key: &TaskKey, force: bool) {
    let Some(guard) = claim(app, key) else {
        return;
    };
    run_check(app, key, force, guard).await;
}

/// Like a forced [`check_task`], but waits up to `timeout` for a check that is running
/// instead of returning, so the check that runs started after the call. Whether a PR
/// snapshot was read and saved by it: `false` when the claim timed out or `gh` failed.
pub(crate) async fn check_task_waiting(app: &AppState, key: &TaskKey, timeout: Duration) -> bool {
    let deadline = Instant::now() + timeout;
    let guard = loop {
        if let Some(guard) = claim(app, key) {
            break guard;
        }
        if Instant::now() >= deadline {
            return false;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    };
    run_check(app, key, true, guard).await
}

/// The check `check_task` and `check_task_waiting` run once they hold the claim. Whether
/// a snapshot of a PR was saved.
async fn run_check(app: &AppState, key: &TaskKey, force: bool, guard: CheckGuard) -> bool {
    let Ok(Some(task)) = app.store.get_task(key).await else {
        return false;
    };
    let mut fresh = false;
    let project = task.project.clone().unwrap_or_default();
    let in_review = task
        .phase
        .as_deref()
        .and_then(SpecPhase::parse)
        .is_some_and(|p| p >= SpecPhase::Review);
    let mut changed = false;
    let mut inbox = false;
    // Whether every repo that had to be searched was, without error.
    let mut searched = true;

    if let Ok(Some((_, ws))) = app.store.workspace(key).await {
        let rows = app.store.prs_of(key).await.unwrap_or_default();
        // A task without a branch has no PR to find.
        let opts = delilah_git::GitOptions {
            allow_file_protocol: app.config.allow_file_protocol,
        };
        let repos = if ws.has_branch() {
            ws.pr_repos(&opts)
        } else {
            vec![]
        };
        for (repo, dir) in repos {
            if !force && (has_task_pr(&rows, &repo, &ws.branch) || !in_review) {
                continue;
            }
            match detect(app, key, &project, &repo, &dir, &ws.branch, force).await {
                Ok(linked) => changed |= linked,
                Err(()) => searched = false,
            }
        }
    }

    for row in app.store.prs_of(key).await.unwrap_or_default() {
        let open = row
            .snapshot
            .as_ref()
            .is_none_or(|s| s.state == PrState::Open);
        if open || force {
            let (checked, saved) = snapshot(app, &project, &row, force).await;
            fresh |= saved;
            changed |= checked.changed;
            inbox |= checked.notices_changed;
        }
    }
    if changed {
        app.emit(Topic::Prs);
    }
    if inbox {
        app.emit(Topic::Inbox);
    }
    // Every PR merged (none open or closed unmerged): the task is done and archived,
    // unless its agent is mid-turn (it may still open another PR) or a PR it opened in
    // another repo is not linked yet.
    let mut rows = app.store.prs_of(key).await.unwrap_or_default();
    if merge::all_merged(&rows) && !primary_working(app, key) {
        // Fail closed: archive only if every repo was searched, and re-check once the
        // slow search is over (the agent may have started a turn or opened a PR).
        let mut safe = searched && !primary_working(app, key);
        if !force {
            let (linked, saved, searched) = pre_archive_detect(app, key, &project).await;
            if linked {
                changed = true;
                app.emit(Topic::Prs);
            }
            fresh |= saved;
            rows = app.store.prs_of(key).await.unwrap_or_default();
            safe = searched && !primary_working(app, key);
            if !searched {
                tracing::warn!(task = %key, "PR search failed before archive; retrying next poll");
            }
        }
        if safe && merge::all_merged(&rows) {
            merge::on_merged(app, key, &rows).await;
            merge::on_subtask_merged(app, &task, changed).await;
        }
    }
    if task.kind == delilah_core::hierarchy::TaskKind::ParentTask {
        merge::parent_ready(app, key).await;
    }
    // Autofix looks at what this check saw, once the next check may start.
    drop(guard);
    let (app, key) = (app.clone(), key.clone());
    tokio::spawn(async move { crate::autofix::evaluate(&app, &key).await });
    fresh
}

/// Whether the task's primary session is in the middle of a turn.
fn primary_working(app: &AppState, key: &TaskKey) -> bool {
    app.sessions
        .primary(key.as_str())
        .is_some_and(|l| l.state() == crate::sessions::LiveState::Working)
}

/// One forced detection over the task's `pr_repos` before it is archived, so a PR from
/// its branch in another repo is linked first. Whether one was linked, and whether a
/// snapshot was saved, and whether every repo was searched without error.
async fn pre_archive_detect(app: &AppState, key: &TaskKey, project: &str) -> (bool, bool, bool) {
    let Ok(Some((_, ws))) = app.store.workspace(key).await else {
        return (false, false, true);
    };
    if !ws.has_branch() {
        return (false, false, true);
    }
    let opts = delilah_git::GitOptions {
        allow_file_protocol: app.config.allow_file_protocol,
    };
    let mut linked = false;
    let mut searched = true;
    for (repo, dir) in ws.pr_repos(&opts) {
        match detect(app, key, project, &repo, &dir, &ws.branch, true).await {
            Ok(l) => linked |= l,
            Err(()) => searched = false,
        }
    }
    let mut saved = false;
    if linked {
        for row in app.store.prs_of(key).await.unwrap_or_default() {
            if row.snapshot.is_none() {
                saved |= snapshot(app, project, &row, true).await.1;
            }
        }
    }
    (linked, saved, searched)
}

/// Whether `repo` already has a counted `Open` or `Merged` PR from `branch`, so there is
/// nothing to look for. A repo with only closed PRs, or PRs from other branches (linked
/// by hand), is still searched. A snapshot from before the head branch was read counts
/// as the task's.
fn has_task_pr(rows: &[PrRow], repo: &str, branch: &str) -> bool {
    crate::store_prs::counted(rows)
        .filter(|r| r.repo == repo)
        .filter_map(|r| r.snapshot.as_ref())
        .any(|s| s.state != PrState::Closed && s.head_branch.as_deref().is_none_or(|b| b == branch))
}

/// Looks for the PR of `branch` in one repo and links it. Whether the rows changed;
/// `Err` when `gh` failed (already reported).
async fn detect(
    app: &AppState,
    key: &TaskKey,
    project: &str,
    repo: &str,
    dir: &Path,
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
    // The same form the pasted links are stored in.
    let url = parse_pr_url(&pr.url).map_or_else(|| pr.url.clone(), |r| r.url());
    if app.store.is_unlinked(key, &url).await.unwrap_or(false) {
        return Ok(false);
    }
    let existing = app.store.prs_of(key).await.unwrap_or_default();
    if existing.iter().any(|r| r.url == url) {
        return Ok(false);
    }
    let new = NewPr {
        repo: repo.to_string(),
        host: info.host,
        owner_name: info.owner_name,
        number: pr.number,
        url,
        linked_by: "detected".into(),
    };
    match app.store.link_pr_on_branch(key, &new, Some(branch)).await {
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

/// Reads a linked PR and saves what it found. Whether to tell the open clients: after
/// every good check (they show when it was made) and when the problem changed. Also
/// whether the snapshot was read and saved.
async fn snapshot(app: &AppState, project: &str, row: &PrRow, force: bool) -> (Checked, bool) {
    let backoff_key = format!("{}/{}", row.host, row.owner_name);
    if !force && !due(app, &backoff_key) {
        return (Checked::default(), false);
    }
    match app
        .gh
        .pr_snapshot(&row.host, &row.owner_name, row.number)
        .await
    {
        Ok(snap) => {
            succeeded(app, project, &[&backoff_key]).await;
            match record(app, row, &snap).await {
                Ok(checked) => (checked, true),
                Err(e) => {
                    tracing::warn!(task = %row.task_key, "could not save the PR check: {e}");
                    (Checked::default(), false)
                }
            }
        }
        Err(e) => {
            failed(app, project, &backoff_key, &e, force).await;
            let kind = classify(&e).kind();
            let text = e.to_string();
            if row.error.as_deref() == Some(text.as_str())
                && row.error_kind.as_deref() == Some(kind)
            {
                return (Checked::default(), false);
            }
            let checked = Checked {
                changed: app
                    .store
                    .set_pr_error(row.id, Some((kind, &text)))
                    .await
                    .is_ok(),
                notices_changed: false,
            };
            (checked, false)
        }
    }
}

fn due(app: &AppState, key: &str) -> bool {
    app.pr_backoff
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .due(key, Instant::now())
}

/// Links the PR at `url` to the task, in the repo of its workspace it belongs to, and
/// moves the task to review. `linked_by` is `pasted` or `opened`.
pub(crate) async fn link_by_url(
    app: &AppState,
    key: &TaskKey,
    url: &str,
    linked_by: &str,
) -> Result<PrRow, LinkError> {
    let pr = parse_pr_url(url).ok_or(LinkError::NotAPr)?;
    let (_, snapshot) = app.gh.pr_view_url(&pr.url()).await.map_err(LinkError::Gh)?;
    let repo = repo_of(app, key, &pr).await?;
    link_known(app, key, repo, pr, &snapshot, linked_by).await
}

/// Links `pr`, already read as `snapshot` and known to belong to `repo` of the task's
/// workspace, and moves the task to review.
pub(crate) async fn link_known(
    app: &AppState,
    key: &TaskKey,
    repo: String,
    pr: PrRef,
    snapshot: &Snapshot,
    linked_by: &str,
) -> Result<PrRow, LinkError> {
    let new = NewPr {
        repo,
        url: pr.url(),
        host: pr.host,
        owner_name: pr.owner_name,
        number: pr.number,
        linked_by: linked_by.to_string(),
    };
    let row = app
        .store
        .link_pr_on_branch(key, &new, snapshot.head_branch.as_deref())
        .await?;
    let checked = record(app, &row, snapshot).await?;
    phases::advance(app, key, SpecPhase::Review).await;
    app.emit(Topic::Prs);
    if checked.notices_changed {
        app.emit(Topic::Inbox);
    }
    Ok(app.store.pr(row.id).await?.unwrap_or(row))
}

/// The repo of the task that `pr` belongs to: `.` or a submodule of its workspace (or,
/// without one, of its project's checkout), else the label `host/owner/name`, which is
/// how a PR of any other repo is told apart.
pub(super) async fn repo_of(
    app: &AppState,
    key: &TaskKey,
    pr: &PrRef,
) -> Result<String, LinkError> {
    let is_pr_repo = |dir: &Path| {
        let dir = dir.to_path_buf();
        async move {
            app.gh.repo_view(&dir).await.is_ok_and(|info| {
                info.host.eq_ignore_ascii_case(&pr.host)
                    && info.owner_name.eq_ignore_ascii_case(&pr.owner_name)
            })
        }
    };
    let foreign = || format!("{}/{}", pr.host, pr.owner_name);
    if let Some((_, ws)) = app.store.workspace(key).await? {
        let opts = delilah_git::GitOptions {
            allow_file_protocol: app.config.allow_file_protocol,
        };
        let repos = if ws.has_branch() {
            ws.pr_repos(&opts)
        } else {
            ws.repos()
        };
        for (name, dir) in repos {
            if is_pr_repo(&dir).await {
                return Ok(name);
            }
        }
        return Ok(foreign());
    }
    let project = match app.store.get_task(key).await?.and_then(|t| t.project) {
        Some(name) => app.store.project(&name).await?,
        None => return Ok(foreign()),
    };
    if is_pr_repo(&project.path).await {
        return Ok(".".into());
    }
    if project.kind == ProjectKind::Parent {
        let root = project.path.clone();
        let subs = tokio::task::spawn_blocking(move || delilah_git::list_submodules(&root))
            .await
            .ok()
            .and_then(Result::ok)
            .unwrap_or_default();
        for sub in subs {
            // Only a set-up submodule has a folder to ask `gh` in.
            let Some(dir) = delilah_git::Workspace::home_of(&project.path, &sub.path) else {
                continue;
            };
            if is_pr_repo(&dir).await {
                return Ok(sub.path);
            }
        }
    }
    Ok(foreign())
}

#[cfg(test)]
mod tests {
    use delilah_core::{Project, Task};

    use super::*;
    use crate::gh::fake::{FakeGh, PR_URL, REPO_VIEW, bare_snapshot};
    use crate::pr_testkit::task_with_workspace;

    fn gh_failed(stderr: &str) -> GhError {
        GhError::Failed {
            code: None,
            stderr: stderr.into(),
        }
    }

    const SIGNED_OUT: &str = "To get started with GitHub CLI, please run: gh auth login";

    fn drain(rx: &mut tokio::sync::broadcast::Receiver<Topic>) -> Vec<Topic> {
        let mut topics = vec![];
        while let Ok(t) = rx.try_recv() {
            topics.push(t);
        }
        topics
    }

    #[tokio::test]
    async fn a_pr_from_the_branch_is_linked_snapshotted_and_announced() {
        let fake = Arc::new(FakeGh::happy());
        let (app, key, _dir) = task_with_workspace(fake, Some(SpecPhase::Implement)).await;
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
        assert!(drain(&mut rx).contains(&Topic::Prs));
    }

    #[tokio::test]
    async fn check_task_waiting_waits_for_the_running_check_and_reports_a_fresh_snapshot() {
        let fake = Arc::new(FakeGh::happy());
        let (app, key, _dir) = task_with_workspace(fake, Some(SpecPhase::Implement)).await;
        check_task(&app, &key, true).await;

        // A check is running and outlasts the timeout: nothing was read.
        let held = claim(&app, &key).unwrap();
        assert!(!check_task_waiting(&app, &key, Duration::from_millis(100)).await);

        // It ends while we wait: our own check runs after it.
        let release = tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(150)).await;
            drop(held);
        });
        let before = chrono::Utc::now();
        assert!(check_task_waiting(&app, &key, Duration::from_secs(10)).await);
        release.await.unwrap();
        let row = app.store.prs_of(&key).await.unwrap().remove(0);
        assert!(row.checked_at.unwrap() >= before);
    }

    #[tokio::test]
    async fn a_branchless_task_is_not_searched_for_a_pr() {
        let fake = Arc::new(FakeGh::happy());
        let (app, key, _dir) = task_with_workspace(fake.clone(), Some(SpecPhase::Implement)).await;
        let (root, mut ws) = app.store.workspace(&key).await.unwrap().unwrap();
        ws.branch = String::new();
        app.store.save_workspace(&key, &root, &ws).await.unwrap();
        check_task(&app, &key, true).await;
        assert!(fake.calls().is_empty());
        assert!(app.store.prs_of(&key).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn every_good_check_announces_even_when_nothing_changed() {
        let fake = Arc::new(FakeGh::happy());
        let (app, key, _dir) = task_with_workspace(fake, Some(SpecPhase::Review)).await;
        check_task(&app, &key, false).await;
        let mut rx = app.events.subscribe();
        check_task(&app, &key, true).await;
        assert!(drain(&mut rx).contains(&Topic::Prs));
    }

    #[tokio::test]
    async fn an_unlinked_url_is_not_linked_again() {
        let fake = Arc::new(FakeGh::happy());
        let (app, key, _dir) = task_with_workspace(fake, Some(SpecPhase::Review)).await;
        check_task(&app, &key, false).await;
        let row = app.store.prs_of(&key).await.unwrap().remove(0);
        app.store.unlink_pr(row.id).await.unwrap();
        check_task(&app, &key, true).await;
        assert!(app.store.prs_of(&key).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn below_review_without_a_pr_nothing_is_searched_unless_forced() {
        let fake = Arc::new(FakeGh::happy());
        let (app, key, _dir) = task_with_workspace(fake.clone(), Some(SpecPhase::Implement)).await;
        check_task(&app, &key, false).await;
        assert!(fake.calls().is_empty());
        assert!(app.store.prs_of(&key).await.unwrap().is_empty());
        check_task(&app, &key, true).await;
        assert!(!fake.calls().is_empty());
        assert_eq!(app.store.prs_of(&key).await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn a_network_error_keeps_the_snapshot_and_backs_off() {
        // The PR is from the task's branch, so detection leaves the repo alone.
        let from_branch = Arc::new(
            FakeGh::new()
                .on(&["repo", "view"], Ok(REPO_VIEW))
                .on(&["pr", "list"], Ok(crate::gh::fake::PR_LIST))
                .on(
                    &["api", "graphql"],
                    Ok(&bare_snapshot("OPEN", false).replace("feat/x", "feature/PROJ-1")),
                ),
        );
        let (app, key, _dir) = task_with_workspace(from_branch, Some(SpecPhase::Review)).await;
        check_task(&app, &key, false).await;
        let before = app.store.prs_of(&key).await.unwrap().remove(0);
        assert!(before.snapshot.is_some());

        let failing =
            Arc::new(FakeGh::new().on(&["api", "graphql"], Err(gh_failed("connection reset"))));
        let app = app.with_gh(failing.clone());
        check_task(&app, &key, false).await;
        let after = app.store.prs_of(&key).await.unwrap().remove(0);
        assert_eq!(after.snapshot, before.snapshot);
        assert!(after.error.as_deref().unwrap().contains("connection reset"));
        assert_eq!(after.error_kind.as_deref(), Some("network"));
        assert_eq!(failing.calls().len(), 1);

        check_task(&app, &key, false).await;
        assert_eq!(failing.calls().len(), 1, "inside the back-off window");
        assert!(app.notifier.recorded().is_empty());
    }

    #[tokio::test]
    async fn the_kind_of_a_failed_snapshot_is_stored_and_cleared_on_success() {
        let (app, key, _dir) =
            task_with_workspace(Arc::new(FakeGh::happy()), Some(SpecPhase::Review)).await;
        check_task(&app, &key, false).await;
        let missing = Arc::new(FakeGh::new().on(&["api", "graphql"], Err(GhError::NotFound)));
        let app = app.with_gh(missing);
        check_task(&app, &key, true).await;
        let row = app.store.prs_of(&key).await.unwrap().remove(0);
        assert_eq!(row.error_kind.as_deref(), Some("missing"));

        let app = app.with_gh(Arc::new(FakeGh::happy()));
        check_task(&app, &key, true).await;
        let row = app.store.prs_of(&key).await.unwrap().remove(0);
        assert_eq!((row.error, row.error_kind), (None, None));
    }

    #[tokio::test]
    async fn an_access_problem_is_kept_notified_once_and_cleared_on_success() {
        let signed_out = Arc::new(FakeGh::new().on(&["repo", "view"], Err(gh_failed(SIGNED_OUT))));
        let (app, key, _dir) = task_with_workspace(signed_out, Some(SpecPhase::Review)).await;
        check_task(&app, &key, true).await;
        check_task(&app, &key, true).await;
        let sent = app.notifier.recorded();
        assert_eq!(sent.len(), 1);
        assert!(sent[0].title.contains("proj"));
        let found = access::problem_of(&app, "proj").await.unwrap();
        assert_eq!(found.kind, "signed-out");
        assert!(found.message.contains("gh auth login"));

        // Another access problem replaces the stored one, and is told once more.
        let no_access = Arc::new(FakeGh::new().on(
            &["repo", "view"],
            Err(gh_failed("gh: Not Found (HTTP 404)")),
        ));
        let app = app.with_gh(no_access);
        check_task(&app, &key, true).await;
        assert_eq!(
            access::problem_of(&app, "proj").await.unwrap().kind,
            "no-access"
        );
        assert_eq!(app.notifier.recorded().len(), 2);

        let app = app.with_gh(Arc::new(FakeGh::happy()));
        let mut rx = app.events.subscribe();
        check_task(&app, &key, true).await;
        assert_eq!(access::problem_of(&app, "proj").await, None);
        assert!(drain(&mut rx).contains(&Topic::Prs));
        assert_eq!(app.store.prs_of(&key).await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn a_listening_app_gets_no_notification() {
        let signed_out = Arc::new(FakeGh::new().on(&["repo", "view"], Err(gh_failed(SIGNED_OUT))));
        let (app, key, _dir) = task_with_workspace(signed_out, Some(SpecPhase::Review)).await;
        let mut rx = app.events.subscribe();
        check_task(&app, &key, true).await;
        assert!(drain(&mut rx).contains(&Topic::Prs), "the open tab updates");
        assert!(app.notifier.recorded().is_empty());
        assert!(access::problem_of(&app, "proj").await.is_some());
    }

    #[tokio::test]
    async fn only_one_check_of_a_task_runs_at_a_time() {
        let fake = Arc::new(FakeGh::happy());
        let (app, key, _dir) = task_with_workspace(fake.clone(), Some(SpecPhase::Review)).await;
        let running = claim(&app, &key).unwrap();
        assert!(is_checking(&app, &key));
        assert!(claim(&app, &key).is_none());
        check_task(&app, &key, true).await;
        assert!(fake.calls().is_empty(), "a second check does nothing");
        drop(running);
        assert!(!is_checking(&app, &key));

        check_task(&app, &key, true).await;
        assert!(!fake.calls().is_empty());
        assert!(!is_checking(&app, &key), "the check lets go when done");
    }

    #[tokio::test]
    async fn nudges_in_the_window_make_one_delayed_check() {
        let fake = Arc::new(FakeGh::happy());
        let (app, key, _dir) = task_with_workspace(fake.clone(), Some(SpecPhase::Implement)).await;
        for _ in 0..3 {
            nudge_after(&app, &key, Duration::from_millis(30));
        }
        assert!(fake.calls().is_empty(), "not before the delay");
        for _ in 0..100 {
            if !app.store.prs_of(&key).await.unwrap().is_empty() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
        let searches = fake
            .calls()
            .iter()
            .filter(|c| c[..2] == ["pr", "list"])
            .count();
        assert_eq!(searches, 1);
        assert!(app.pr_nudges.lock().unwrap().is_empty());

        // Once it has run, the next push is looked for again.
        nudge_after(&app, &key, Duration::from_millis(10));
        tokio::time::sleep(Duration::from_millis(200)).await;
        let searches = fake
            .calls()
            .iter()
            .filter(|c| c[..2] == ["pr", "list"])
            .count();
        assert_eq!(searches, 2);
    }

    #[tokio::test]
    async fn linking_by_url_stores_the_canonical_url_and_moves_to_review() {
        let fake = Arc::new(FakeGh::happy());
        let (app, key, _dir) = task_with_workspace(fake, Some(SpecPhase::Implement)).await;
        let mut rx = app.events.subscribe();
        let row = link_by_url(&app, &key, &format!("{PR_URL}/files?x=1#top"), "opened")
            .await
            .unwrap();
        assert_eq!(row.url, PR_URL);
        assert_eq!(row.linked_by, "opened");
        assert_eq!(row.repo, ".");
        assert!(row.snapshot.is_some());
        let task = app.store.get_task(&key).await.unwrap().unwrap();
        assert_eq!(task.phase.as_deref(), Some("review"));
        assert!(drain(&mut rx).contains(&Topic::Prs));

        // Unlinking by the canonical URL stops detection from bringing it back.
        app.store.unlink_pr(row.id).await.unwrap();
        check_task(&app, &key, true).await;
        assert!(app.store.prs_of(&key).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn linking_refuses_urls_that_are_not_prs_and_labels_other_repos_by_host() {
        let fake = Arc::new(FakeGh::happy());
        let (app, key, _dir) = task_with_workspace(fake.clone(), Some(SpecPhase::Implement)).await;
        let err = link_by_url(&app, &key, "https://example.com/x", "pasted").await;
        assert!(matches!(err, Err(LinkError::NotAPr)));

        // The repo of the workspace is o/r: another repo's PR is linked by where it lives.
        let row = link_by_url(&app, &key, "https://github.com/other/repo/pull/7", "pasted")
            .await
            .unwrap();
        assert_eq!(row.repo, "github.com/other/repo");
        let row = link_by_url(&app, &key, "https://ghe.corp/o/r/pull/7", "pasted")
            .await
            .unwrap();
        assert_eq!(row.repo, "ghe.corp/o/r");

        let unreadable = Arc::new(FakeGh::new().on(&["api", "graphql"], Err(gh_failed("boom"))));
        let app = app.with_gh(unreadable);
        let err = link_by_url(&app, &key, PR_URL, "pasted").await;
        assert!(matches!(err, Err(LinkError::Gh(_))));
    }

    #[tokio::test]
    async fn without_a_workspace_the_projects_checkout_is_dot_and_others_are_labelled() {
        let fake = Arc::new(FakeGh::happy());
        let (app, _key, _dir) = task_with_workspace(fake, None).await;
        app.store
            .create_project(&Project {
                name: "par".into(),
                path: "/par".into(),
                kind: ProjectKind::Parent,
                workflow: "project".into(),
                settings: Default::default(),
                preview_command: None,
            })
            .await
            .unwrap();
        for (name, project) in [("PROJ-2", "proj"), ("PROJ-3", "par")] {
            let mut task = Task::new(TaskKey::new(name), "t", "project");
            task.project = Some(project.into());
            app.store.create_task(&task).await.unwrap();
        }
        let row = link_by_url(&app, &TaskKey::new("PROJ-2"), PR_URL, "pasted")
            .await
            .unwrap();
        assert_eq!(row.repo, ".");
        let row = link_by_url(&app, &TaskKey::new("PROJ-3"), PR_URL, "pasted")
            .await
            .unwrap();
        assert_eq!(row.repo, ".");

        // The checkout is another repo than the PR's.
        let other = Arc::new(
            FakeGh::new()
                .on(&["repo", "view"], Ok(&REPO_VIEW.replace("o/r", "x/y")))
                .on(&["api", "graphql"], Ok(&bare_snapshot("OPEN", false))),
        );
        let app = app.with_gh(other);
        let row = link_by_url(&app, &TaskKey::new("PROJ-2"), PR_URL, "pasted")
            .await
            .unwrap();
        assert_eq!(row.repo, "github.com/o/r");
    }

    #[tokio::test]
    async fn without_a_workspace_a_parents_submodule_is_matched_before_the_host_label() {
        let gh = Arc::new(crate::pr_testkit::DirGh::new().with_pr("svc-a", 9, "OPEN", "def"));
        let (app, _key, dir) = task_with_workspace(Arc::new(FakeGh::new()), None).await;
        let (root, _ws) =
            crate::pr_testkit::parent_workspace(&dir.path().join("pw"), "feature/PROJ-1");
        let app = app.with_gh(gh);
        app.store
            .create_project(&Project {
                name: "par".into(),
                path: root,
                kind: ProjectKind::Parent,
                workflow: "project".into(),
                settings: Default::default(),
                preview_command: None,
            })
            .await
            .unwrap();
        let mut task = Task::new(TaskKey::new("PROJ-3"), "t", "project");
        task.project = Some("par".into());
        app.store.create_task(&task).await.unwrap();
        let key = TaskKey::new("PROJ-3");
        let row = link_by_url(&app, &key, "https://github.com/o/svc-a/pull/9", "pasted")
            .await
            .unwrap();
        assert_eq!(row.repo, "svc-a");
        let row = link_by_url(&app, &key, "https://github.com/o/parent/pull/9", "pasted").await;
        // DirGh only knows svc-a's PR: the parent's own is unreadable, not mislabelled.
        assert!(matches!(row, Err(LinkError::Gh(_))));
    }

    /// A review-phase task whose parent workspace is on the task's branch in the parent
    /// too, talking to `gh`.
    async fn parent_task(
        gh: Arc<crate::pr_testkit::DirGh>,
    ) -> (AppState, TaskKey, crate::pr_testkit::TempDir) {
        let (app, key, dir) =
            task_with_workspace(Arc::new(FakeGh::new()), Some(SpecPhase::Review)).await;
        let (root, ws) =
            crate::pr_testkit::parent_workspace(&dir.path().join("pw"), "feature/PROJ-1");
        app.store.save_workspace(&key, &root, &ws).await.unwrap();
        (app.with_gh(gh), key, dir)
    }

    fn urls(rows: &[PrRow]) -> Vec<(String, String)> {
        let mut v: Vec<_> = rows
            .iter()
            .map(|r| (r.repo.clone(), r.url.clone()))
            .collect();
        v.sort();
        v
    }

    #[tokio::test]
    async fn a_pr_in_the_parents_own_repo_is_detected_as_dot() {
        // The parent's workspace folder is `ws`.
        let gh = Arc::new(crate::pr_testkit::DirGh::new().with_pr("ws", 5, "OPEN", "abc"));
        let (app, key, _dir) = parent_task(gh).await;
        check_task(&app, &key, false).await;
        let rows = app.store.prs_of(&key).await.unwrap();
        assert_eq!(
            urls(&rows),
            [(
                ".".to_string(),
                "https://github.com/o/ws/pull/5".to_string()
            )]
        );
    }

    #[tokio::test]
    async fn a_pr_of_another_branch_in_a_submodule_does_not_stop_detection() {
        let gh = Arc::new(
            crate::pr_testkit::DirGh::new()
                .with_pr("ws", 5, "OPEN", "abc")
                .with_pr("svc-a", 9, "OPEN", "def"),
        );
        let (app, key, _dir) = parent_task(gh).await;
        // Linked by hand; its head branch (`feat/x` in the fake) is not the task's.
        link_by_url(&app, &key, "https://github.com/o/svc-a/pull/9", "pasted")
            .await
            .unwrap();
        check_task(&app, &key, false).await;
        let rows = app.store.prs_of(&key).await.unwrap();
        assert!(rows.iter().all(|r| r.replaced_by.is_none()), "{rows:?}");
        assert_eq!(
            urls(&rows),
            [
                (
                    ".".to_string(),
                    "https://github.com/o/ws/pull/5".to_string()
                ),
                (
                    "svc-a".to_string(),
                    "https://github.com/o/svc-a/pull/9".to_string()
                ),
            ]
        );
    }

    #[tokio::test]
    async fn a_repo_whose_only_pr_is_closed_is_still_searched() {
        let gh = Arc::new(crate::pr_testkit::DirGh::new().with_pr("svc-a", 9, "CLOSED", "def"));
        let (app, key, _dir) = parent_task(gh.clone()).await;
        link_by_url(&app, &key, "https://github.com/o/svc-a/pull/9", "pasted")
            .await
            .unwrap();
        check_task(&app, &key, false).await;
        gh.set_pr("svc-a", 10, "OPEN", "ghi");
        check_task(&app, &key, false).await;
        let rows = app.store.prs_of(&key).await.unwrap();
        assert!(
            rows.iter()
                .any(|r| r.url == "https://github.com/o/svc-a/pull/10"),
            "{rows:?}"
        );
    }

    #[tokio::test]
    async fn a_detected_pr_replaces_the_closed_one_of_the_same_branch() {
        let gh = Arc::new(
            crate::pr_testkit::DirGh::new()
                .with_pr("svc-a", 9, "CLOSED", "def")
                .with_head_ref("feature/PROJ-1"),
        );
        let (app, key, _dir) = parent_task(gh.clone()).await;
        link_by_url(&app, &key, "https://github.com/o/svc-a/pull/9", "pasted")
            .await
            .unwrap();
        check_task(&app, &key, false).await;
        gh.set_pr("svc-a", 10, "OPEN", "ghi");
        check_task(&app, &key, false).await;
        let rows = app.store.prs_of(&key).await.unwrap();
        let old = rows.iter().find(|r| r.number == 9).unwrap();
        let new = rows.iter().find(|r| r.number == 10).unwrap();
        assert_eq!(old.replaced_by, Some(new.id), "{rows:?}");
        assert_eq!(crate::store_prs::counted(&rows).count(), 1);
    }

    /// A primary session played by `cat`, in `state`.
    fn primary_session(app: &AppState, state: crate::sessions::LiveState) {
        use delilah_agents::pty::PtySession;
        use portable_pty::CommandBuilder;

        let mut cmd = CommandBuilder::new("cat");
        cmd.args(["-u"]);
        let pty = PtySession::spawn(cmd, 200, 24, None).unwrap();
        app.sessions.insert(
            "s-1",
            "PROJ-1",
            crate::store_sessions::SessionKind::Claude,
            true,
            pty,
        );
        app.sessions.primary("PROJ-1").unwrap().set_state(state);
    }

    async fn status(app: &AppState, key: &TaskKey) -> delilah_core::TaskStatus {
        app.store.get_task(key).await.unwrap().unwrap().status
    }

    #[tokio::test]
    async fn the_pre_archive_detection_links_a_pr_the_normal_search_missed() {
        let gh = Arc::new(crate::pr_testkit::DirGh::new().with_pr("svc-a", 9, "MERGED", "abc"));
        let (mut app, key, _dir) = parent_task(gh.clone()).await;
        let mut config = (*app.config).clone();
        config.allow_file_protocol = true;
        app.config = Arc::new(config);
        // While the agent works nothing is archived.
        primary_session(&app, crate::sessions::LiveState::Working);
        check_task(&app, &key, false).await;
        assert_eq!(app.store.prs_of(&key).await.unwrap().len(), 1);

        // Another repo gets an open PR; its search is backed off, so only the forced
        // detection before the archive can see it.
        gh.set_pr("svc-b", 3, "OPEN", "def");
        let (_, ws) = app.store.workspace(&key).await.unwrap().unwrap();
        app.pr_backoff.lock().unwrap().fail(
            &format!("dir:{}", ws.path.join("svc-b").display()),
            Instant::now(),
            None,
        );
        app.sessions
            .primary("PROJ-1")
            .unwrap()
            .set_state(crate::sessions::LiveState::Waiting);
        check_task(&app, &key, false).await;

        let rows = app.store.prs_of(&key).await.unwrap();
        assert!(
            rows.iter()
                .any(|r| r.url == "https://github.com/o/svc-b/pull/3"),
            "{rows:?}"
        );
        assert_ne!(status(&app, &key).await, delilah_core::TaskStatus::Done);
    }

    #[tokio::test]
    async fn nothing_is_archived_mid_turn_but_the_next_check_after_it_archives() {
        let gh = Arc::new(crate::pr_testkit::DirGh::new().with_pr("svc-a", 9, "MERGED", "abc"));
        let (mut app, key, _dir) = parent_task(gh).await;
        let mut config = (*app.config).clone();
        config.allow_file_protocol = true;
        app.config = Arc::new(config);
        primary_session(&app, crate::sessions::LiveState::Working);

        check_task(&app, &key, true).await;
        assert_ne!(status(&app, &key).await, delilah_core::TaskStatus::Done);
        assert!(app.store.workspace(&key).await.unwrap().is_some());

        app.sessions
            .primary("PROJ-1")
            .unwrap()
            .set_state(crate::sessions::LiveState::Waiting);
        check_task(&app, &key, true).await;
        assert!(
            app.store.workspace(&key).await.unwrap().is_none()
                || status(&app, &key).await == delilah_core::TaskStatus::Done
        );
    }

    #[tokio::test]
    async fn a_failed_search_before_the_archive_keeps_the_task() {
        let gh = Arc::new(crate::pr_testkit::DirGh::new().with_pr("svc-a", 9, "MERGED", "abc"));
        let (mut app, key, _dir) = parent_task(gh.clone()).await;
        let mut config = (*app.config).clone();
        config.allow_file_protocol = true;
        app.config = Arc::new(config);
        // The normal pass skips svc-b (backed off); the forced search of it fails.
        let (_, ws) = app.store.workspace(&key).await.unwrap().unwrap();
        gh.fail_list("svc-b");
        app.pr_backoff.lock().unwrap().fail(
            &format!("dir:{}", ws.path.join("svc-b").display()),
            Instant::now(),
            None,
        );
        check_task(&app, &key, false).await;
        assert_ne!(status(&app, &key).await, delilah_core::TaskStatus::Done);
        assert!(app.store.workspace(&key).await.unwrap().is_some());
    }

    #[tokio::test]
    async fn a_failed_search_on_a_forced_check_keeps_the_task() {
        let gh = Arc::new(crate::pr_testkit::DirGh::new().with_pr("svc-a", 9, "MERGED", "abc"));
        let (mut app, key, _dir) = parent_task(gh.clone()).await;
        let mut config = (*app.config).clone();
        config.allow_file_protocol = true;
        app.config = Arc::new(config);
        gh.fail_list("svc-b");
        check_task(&app, &key, true).await;
        assert_ne!(status(&app, &key).await, delilah_core::TaskStatus::Done);
        assert!(app.store.workspace(&key).await.unwrap().is_some());
    }

    #[tokio::test]
    async fn an_agent_that_starts_a_turn_during_the_search_is_not_archived() {
        let gh = Arc::new(crate::pr_testkit::DirGh::new().with_pr("svc-a", 9, "MERGED", "abc"));
        let (mut app, key, _dir) = parent_task(gh.clone()).await;
        let mut config = (*app.config).clone();
        config.allow_file_protocol = true;
        app.config = Arc::new(config);
        primary_session(&app, crate::sessions::LiveState::Waiting);
        let (_, ws) = app.store.workspace(&key).await.unwrap().unwrap();
        app.pr_backoff.lock().unwrap().fail(
            &format!("dir:{}", ws.path.join("svc-b").display()),
            Instant::now(),
            None,
        );
        let sessions = app.sessions.clone();
        gh.on_list(move || {
            if let Some(l) = sessions.primary("PROJ-1") {
                l.set_state(crate::sessions::LiveState::Working);
            }
        });
        check_task(&app, &key, false).await;
        assert_ne!(status(&app, &key).await, delilah_core::TaskStatus::Done);
        assert!(app.store.workspace(&key).await.unwrap().is_some());
    }
}
