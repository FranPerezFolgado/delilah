//! Archive on merge (research R7): when every PR of a task is merged, the task is marked
//! done and its workspace released, unless that would lose work. Never deletes a remote
//! branch. How an archive is done is `crate::archive`'s; this is the policy: what must
//! hold first, and what the user is told.

use std::collections::HashMap;

use delilah_core::hierarchy::TaskKind;
use delilah_core::{Event, Task, TaskKey};
use delilah_git::GitOptions;
use serde_json::json;

use crate::archive::{
    ArchivePartial, CAUSE_MERGE, KIND_BLOCKED, KIND_PARTIAL, Outcome, archive_workspace,
    stop_sessions,
};
use crate::events::Topic;
use crate::inbox::{KIND_PARENT_TASK_LIVE, KIND_PARENT_TASK_READY};
use crate::pr_types::PrState;
use crate::state::AppState;
use crate::store::StoreError;
use crate::store_prs::{NewNotice, PrRow};

/// Whether the task's PRs are all merged: it has some, and none is open, closed
/// unmerged or not read yet (FR-017, FR-039).
pub(super) fn all_merged(rows: &[PrRow]) -> bool {
    !rows.is_empty()
        && rows.iter().all(|r| {
            r.snapshot
                .as_ref()
                .is_some_and(|s| s.state == PrState::Merged)
        })
}

/// How a merged task's archive ended.
enum Verdict {
    Archived,
    /// Released, but these local branches could not be deleted.
    Partial(Outcome),
    /// Refused, with nothing removed: why.
    Blocked(String),
}

/// The task's PRs are all merged: mark it done and archive it, keeping any work that
/// is not in the PRs (`archive-blocked`) and reporting a half-done archive
/// (`archive-partial`). Does nothing for a task already archived. Tells the user once.
pub(super) async fn on_merged(app: &AppState, key: &TaskKey, rows: &[PrRow]) {
    if let Err(e) = archive_merged(app, key, rows).await {
        tracing::warn!(task = %key, "archive on merge: {e}");
    }
}

async fn archive_merged(app: &AppState, key: &TaskKey, rows: &[PrRow]) -> Result<(), StoreError> {
    let Some(task) = app.store.get_task(key).await? else {
        return Ok(());
    };
    let workspace = app.store.workspace(key).await?;
    if task.status.is_terminal() && workspace.is_none() {
        return Ok(());
    }
    if !task.status.is_terminal()
        && let Err(e) = app.transition_quiet(key, Event::MarkDone).await
    {
        tracing::warn!(task = %key, "cannot mark it done after the merge: {e}");
        tell(app, key, &task, &Verdict::Blocked(e.to_string()));
        return Ok(());
    }
    // A parent task with live subtasks stays as it is: they still work on its branch.
    if task.kind == TaskKind::ParentTask {
        let live = match crate::integration::live_subtasks(&app.store, key).await {
            Ok(live) => live,
            Err(e) => {
                tracing::warn!(task = %key, "cannot list its subtasks, not archiving: {e}");
                return Ok(());
            }
        };
        if !live.is_empty() {
            let list: Vec<String> = live.iter().map(|k| k.to_string()).collect();
            let text = format!(
                "{}: {}",
                app.lang().pick(
                    "La PR de la tarea madre se mergeó con subtareas vivas, no se archiva",
                    "The parent task's PR merged with live subtasks; it is not archived"
                ),
                list.join(", ")
            );
            let fresh = notice(
                app,
                key,
                KIND_PARENT_TASK_LIVE,
                text,
                Some(json!({ "subtasks": list })),
            )
            .await?;
            if fresh {
                app.emit(Topic::Inbox);
            }
            app.emit(Topic::Tasks);
            return Ok(());
        }
    }
    // Whatever the state, nothing may write to the repos while they are checked.
    stop_sessions(app, key).await;

    let verdict = match workspace {
        Some((_, ws)) => release(app, key, &ws, rows).await,
        None => Verdict::Archived,
    };
    let fresh = match &verdict {
        Verdict::Archived => true,
        Verdict::Blocked(reason) => {
            let text = format!(
                "{}: {reason}",
                app.lang().pick(
                    "No se archivó la tarea mergeada",
                    "The merged task was not archived"
                )
            );
            notice(
                app,
                key,
                KIND_BLOCKED,
                text,
                Some(json!({ "reason": reason })),
            )
            .await?
        }
        Verdict::Partial(outcome) => {
            let text = format!(
                "{}: {}",
                app.lang().pick(
                    "Archivada a medias, quedan ramas locales",
                    "Archived only partly, local branches are left"
                ),
                outcome.problems.join("; ")
            );
            let detail = serde_json::to_value(ArchivePartial {
                branches: outcome.left.clone(),
            })
            .ok();
            notice(app, key, KIND_PARTIAL, text, detail).await?
        }
    };
    // A refusal that stands from an earlier check was already told.
    if fresh {
        tell(app, key, &task, &verdict);
    }
    for topic in [Topic::Tasks, Topic::Prs, Topic::Inbox, Topic::Specs] {
        app.emit(topic);
    }
    Ok(())
}

/// A subtask's PRs are all merged: the parent task's progress changed (`changed`: this
/// check saw it, so the UI is told), and it may be ready now.
pub(super) async fn on_subtask_merged(app: &AppState, task: &Task, changed: bool) {
    if task.kind != TaskKind::Subtask {
        return;
    }
    if changed {
        app.emit(Topic::Tasks);
    }
    if let Some(parent) = &task.parent_task {
        parent_ready(app, parent).await;
    }
}

/// Tells once that every subtask of the parent task merged while its PR is still a
/// draft: the PR can go. Deduplicated by the parent task and the PR.
pub(super) async fn parent_ready(app: &AppState, parent: &TaskKey) {
    let ready = match crate::integration::progress(&app.store, parent).await {
        Ok(p) => p.ready,
        Err(e) => {
            tracing::warn!(task = %parent, "parent task progress: {e}");
            return;
        }
    };
    if !ready {
        return;
    }
    let rows = app.store.prs_of(parent).await.unwrap_or_default();
    for row in rows.iter().filter(|r| {
        r.snapshot
            .as_ref()
            .is_some_and(|s| s.state == PrState::Open && s.draft)
    }) {
        let text = app.lang().pick(
            "Todas las subtareas están mergeadas: la PR de la tarea madre puede salir",
            "Every subtask is merged: the parent task's PR can go",
        );
        let new = notice_for(app, parent, format!("pr {}", row.url), text.to_string()).await;
        if matches!(new, Ok(true)) {
            app.emit(Topic::Inbox);
        }
    }
}

async fn notice_for(
    app: &AppState,
    key: &TaskKey,
    cause: String,
    text: String,
) -> Result<bool, StoreError> {
    app.store
        .insert_notice(
            key,
            &NewNotice {
                repo: ".".into(),
                kind: KIND_PARENT_TASK_READY.into(),
                cause,
                text,
                detail: None,
            },
        )
        .await
}

/// Checks that nothing of the task's would be lost, then archives its workspace and
/// deletes the branches of the repos whose PR is merged.
async fn release(
    app: &AppState,
    key: &TaskKey,
    ws: &delilah_git::Workspace,
    rows: &[PrRow],
) -> Verdict {
    let opts = GitOptions {
        allow_file_protocol: app.config.allow_file_protocol,
    };
    let heads: HashMap<String, String> = rows
        .iter()
        .filter_map(|r| Some((r.repo.clone(), r.snapshot.as_ref()?.head.clone())))
        .collect();
    if let Some((repo, head)) = heads.iter().find(|(_, h)| !delilah_git::is_sha(h)) {
        return Verdict::Blocked(format!("the PR of {repo} has no valid head ({head:?})"));
    }
    let checked = {
        let ws = ws.clone();
        let opts = opts.clone();
        tokio::task::spawn_blocking(move || verify(&ws, &heads, &opts)).await
    };
    let verified = match checked {
        Ok(Ok(tips)) => tips,
        Ok(Err(reason)) => return Verdict::Blocked(reason),
        Err(e) => return Verdict::Blocked(e.to_string()),
    };
    // Git's own check of what the worktree holds is a second line behind the one above.
    match archive_workspace(app, key, false, &verified).await {
        Ok(outcome) if outcome.left.is_empty() => Verdict::Archived,
        Ok(outcome) => Verdict::Partial(outcome),
        Err(e) => Verdict::Blocked(e.message),
    }
}

/// Whether the workspace can go without losing work: every repo whose PR merged has
/// nothing beyond the PR's head, and every other repo has nothing uncommitted or
/// unpushed. Fails closed (a repo that cannot be fetched or read blocks it). The tip of
/// the task's branch in each merged repo, to delete exactly that.
fn verify(
    ws: &delilah_git::Workspace,
    heads: &HashMap<String, String>,
    opts: &GitOptions,
) -> Result<HashMap<String, String>, String> {
    let repos = ws.repos();
    // All the fetches together, like a task's start.
    let deadline = delilah_git::Deadline::after(delilah_git::NETWORK_LIMIT);
    for (label, dir) in &repos {
        if heads.contains_key(label) {
            delilah_git::fetch(dir, opts, deadline)
                .map_err(|e| format!("fetching {label}: {e}"))?;
        }
    }
    delilah_git::check_merged(ws, heads, opts).map_err(|e| e.to_string())?;
    let others: Vec<String> = repos
        .iter()
        .filter(|(label, _)| !heads.contains_key(label))
        .map(|(label, _)| label.clone())
        .collect();
    delilah_git::check_repos(ws, &others, opts).map_err(|e| e.to_string())?;

    let mut tips = HashMap::new();
    if ws.path.exists() {
        for (label, dir) in repos {
            if !heads.contains_key(&label) {
                continue;
            }
            if let Some(tip) = delilah_git::branch_tip(&dir, &ws.branch, opts)
                .map_err(|e| format!("{label}: {e}"))?
            {
                tips.insert(label, tip);
            }
        }
    }
    Ok(tips)
}

/// Raises the task's notice of `kind`; whether it is new.
async fn notice(
    app: &AppState,
    key: &TaskKey,
    kind: &str,
    text: String,
    detail: Option<serde_json::Value>,
) -> Result<bool, StoreError> {
    app.store
        .insert_notice(
            key,
            &NewNotice {
                repo: ".".into(),
                kind: kind.into(),
                cause: CAUSE_MERGE.into(),
                text,
                detail,
            },
        )
        .await
}

/// The one notification of a merge (the per-PR one is held back for it, see
/// `notices::record`). The open app notifies itself.
fn tell(app: &AppState, key: &TaskKey, task: &Task, verdict: &Verdict) {
    if app.events.listening() {
        return;
    }
    let lang = app.lang();
    let (title, body) = match verdict {
        Verdict::Archived => (
            lang.pick("mergeada y archivada", "merged and archived"),
            task.title.clone(),
        ),
        Verdict::Blocked(reason) => (
            lang.pick("mergeada, sin archivar", "merged, not archived"),
            reason.clone(),
        ),
        Verdict::Partial(outcome) => (
            lang.pick("mergeada, sin archivar", "merged, not archived"),
            outcome.problems.join("; "),
        ),
    };
    app.notifier.send(format!("{key} {title}"), body);
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::path::{Path, PathBuf};
    use std::process::Command;
    use std::sync::{Arc, Mutex};

    use delilah_core::{ProjectKind, SpecPhase, TaskStatus};
    use delilah_git::{Workspace, WorkspaceSpec, create_workspace};
    use futures_util::future::BoxFuture;

    use super::*;
    use crate::gh::fake::{FakeGh, PR_LIST, REPO_VIEW, ci_snapshot};
    use crate::gh::{Gh, GhError};
    use crate::pr_loop::check_task;
    use crate::pr_testkit::{TempDir, task_with_workspace};

    const BRANCH: &str = "feature/PROJ-1";

    fn git(dir: &Path, args: &[&str]) -> String {
        let out = Command::new("git")
            .args(["-c", "user.name=t", "-c", "user.email=t@t"])
            .args([
                "-c",
                "commit.gpgsign=false",
                "-c",
                "protocol.file.allow=always",
            ])
            .arg("-C")
            .arg(dir)
            .args(args)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "git {args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    }

    fn opts() -> GitOptions {
        GitOptions {
            allow_file_protocol: true,
        }
    }

    /// A project repo `root` on `main`, with a bare `origin`.
    fn project_repo(base: &Path) -> PathBuf {
        let (root, origin) = (base.join("root"), base.join("origin.git"));
        std::fs::create_dir_all(&root).unwrap();
        std::fs::create_dir_all(&origin).unwrap();
        git(&origin, &["init", "-q", "--bare", "-b", "main"]);
        git(&root, &["init", "-q", "-b", "main"]);
        git(&root, &["commit", "-q", "--allow-empty", "-m", "base"]);
        git(
            &root,
            &["remote", "add", "origin", origin.to_str().unwrap()],
        );
        git(&root, &["push", "-q", "origin", "main"]);
        root
    }

    fn commit_and_push(dir: &Path) -> String {
        std::fs::write(dir.join("work.txt"), "work").unwrap();
        git(dir, &["add", "."]);
        git(dir, &["commit", "-q", "-m", "work"]);
        git(dir, &["push", "-q", "origin", BRANCH]);
        git(dir, &["rev-parse", "HEAD"])
    }

    fn merged_gh(head: &str, state: &str) -> Arc<FakeGh> {
        Arc::new(
            FakeGh::new()
                .on(&["repo", "view"], Ok(REPO_VIEW))
                .on(&["pr", "list"], Ok(PR_LIST))
                .on(
                    &["api", "graphql"],
                    Ok(&ci_snapshot(state, head, "SUCCESS")),
                ),
        )
    }

    struct Env {
        app: AppState,
        key: TaskKey,
        root: PathBuf,
        ws: Workspace,
        head: String,
        _dir: TempDir,
    }

    impl Env {
        /// A task of a plain repo, in a worktree or (`checkout`) in the project's checkout.
        async fn repo(checkout: bool, state: &str) -> Self {
            let (app, key, dir) =
                task_with_workspace(Arc::new(FakeGh::new()), Some(SpecPhase::Review)).await;
            let base = dir.path().join("repo-env");
            let root = project_repo(&base);
            let ws = if checkout {
                git(&root, &["checkout", "-q", "-b", BRANCH]);
                Workspace {
                    path: root.clone(),
                    branch: BRANCH.into(),
                    kind: ProjectKind::Repo,
                    submodules: vec![],
                }
            } else {
                let spec = WorkspaceSpec {
                    base: None,
                    path: base.join("ws"),
                    branch: Some(BRANCH.into()),
                    kind: ProjectKind::Repo,
                    submodules: vec![],
                };
                create_workspace(
                    &root,
                    &spec,
                    &opts(),
                    delilah_git::Deadline::after(std::time::Duration::from_secs(30)),
                )
                .unwrap()
                .0
            };
            let head = commit_and_push(&ws.path);
            app.store.save_workspace(&key, &root, &ws).await.unwrap();
            if checkout {
                app.store
                    .set_checkout_previous(&key, &[(".".into(), "main".into())])
                    .await
                    .unwrap();
            }
            let app = app.with_gh(merged_gh(&head, state));
            Self {
                app,
                key,
                root,
                ws,
                head,
                _dir: dir,
            }
        }

        async fn status(&self) -> TaskStatus {
            self.app
                .store
                .get_task(&self.key)
                .await
                .unwrap()
                .unwrap()
                .status
        }

        async fn kinds(&self) -> Vec<String> {
            let mut kinds: Vec<String> = self
                .app
                .store
                .open_notices()
                .await
                .unwrap()
                .into_iter()
                .map(|n| n.kind)
                .collect();
            kinds.sort();
            kinds
        }

        fn branch_exists(&self, dir: &Path) -> bool {
            !git(dir, &["branch", "--list", BRANCH]).is_empty()
        }

        fn archived_titles(&self) -> usize {
            let text = self
                .app
                .lang()
                .pick("mergeada y archivada", "merged and archived");
            self.app
                .notifier
                .recorded()
                .iter()
                .filter(|n| n.title.contains("PROJ-1") && n.title.ends_with(text))
                .count()
        }
    }

    #[tokio::test]
    async fn a_merged_worktree_task_is_done_released_and_its_branch_deleted() {
        let env = Env::repo(false, "MERGED").await;
        assert!(env.branch_exists(&env.root));

        check_task(&env.app, &env.key, true).await;

        assert_eq!(env.status().await, TaskStatus::Done);
        assert!(!env.ws.path.exists());
        assert!(!env.branch_exists(&env.root));
        assert!(env.app.store.workspace(&env.key).await.unwrap().is_none());
        assert!(env.kinds().await.is_empty());
        assert_eq!(env.archived_titles(), 1);
        // The remote branch is never touched.
        assert!(!git(&env.root, &["ls-remote", "origin", BRANCH]).is_empty());
        assert!(!env.head.is_empty());

        // Checking again does not archive (or tell) twice.
        check_task(&env.app, &env.key, true).await;
        assert_eq!(env.archived_titles(), 1);
    }

    #[tokio::test]
    async fn a_merged_checkout_task_goes_back_to_its_branch_and_loses_the_task_branch() {
        let env = Env::repo(true, "MERGED").await;
        assert_eq!(git(&env.root, &["branch", "--show-current"]), BRANCH);

        check_task(&env.app, &env.key, true).await;

        assert_eq!(env.status().await, TaskStatus::Done);
        assert_eq!(git(&env.root, &["branch", "--show-current"]), "main");
        assert!(!env.branch_exists(&env.root));
        assert!(env.root.exists());
        assert_eq!(env.archived_titles(), 1);
    }

    #[tokio::test]
    async fn uncommitted_work_blocks_the_archive_once() {
        let env = Env::repo(false, "MERGED").await;
        std::fs::write(env.ws.path.join("wip.txt"), "wip").unwrap();

        check_task(&env.app, &env.key, true).await;
        check_task(&env.app, &env.key, true).await;

        assert_eq!(env.status().await, TaskStatus::Done);
        assert!(env.ws.path.join("wip.txt").exists());
        assert!(env.branch_exists(&env.root));
        assert!(env.app.store.workspace(&env.key).await.unwrap().is_some());
        assert_eq!(env.kinds().await, ["archive-blocked"]);
        let notice = env.app.store.open_notices().await.unwrap().remove(0);
        assert_eq!(notice.cause, "merge");
        assert!(
            notice.detail.unwrap()["reason"]
                .as_str()
                .unwrap()
                .contains("uncommitted")
        );
        assert_eq!(env.archived_titles(), 0);
    }

    #[tokio::test]
    async fn commits_that_are_not_in_the_merged_pr_block_the_archive() {
        let env = Env::repo(false, "MERGED").await;
        std::fs::write(env.ws.path.join("later.txt"), "later").unwrap();
        git(&env.ws.path, &["add", "."]);
        git(&env.ws.path, &["commit", "-q", "-m", "later"]);

        check_task(&env.app, &env.key, true).await;

        assert!(env.ws.path.exists());
        assert_eq!(env.kinds().await, ["archive-blocked"]);
    }

    #[tokio::test]
    async fn a_branch_that_cannot_go_after_the_release_leaves_an_archive_partial() {
        let env = Env::repo(true, "MERGED").await;
        // Already "back" on the task's branch: it is released but cannot be deleted.
        env.app
            .store
            .set_checkout_previous(&env.key, &[(".".into(), BRANCH.into())])
            .await
            .unwrap();

        check_task(&env.app, &env.key, true).await;

        assert_eq!(env.status().await, TaskStatus::Done);
        assert!(env.app.store.workspace(&env.key).await.unwrap().is_none());
        assert_eq!(env.kinds().await, ["archive-partial"]);
        let notice = env.app.store.open_notices().await.unwrap().remove(0);
        let detail: ArchivePartial = serde_json::from_value(notice.detail.unwrap()).unwrap();
        assert_eq!(detail.branches.len(), 1);
        assert_eq!(detail.branches[0].branch, BRANCH);
        assert!(env.branch_exists(&env.root));
    }

    #[tokio::test]
    async fn a_pr_closed_without_merging_only_notifies() {
        let env = Env::repo(false, "OPEN").await;

        check_task(&env.app, &env.key, true).await;
        let app = env.app.clone().with_gh(merged_gh(&env.head, "CLOSED"));
        check_task(&app, &env.key, true).await;
        check_task(&app, &env.key, true).await;

        assert_eq!(env.status().await, TaskStatus::Running);
        assert!(env.ws.path.exists());
        assert!(env.branch_exists(&env.root));
        assert!(env.kinds().await.is_empty());
        assert_eq!(env.archived_titles(), 0);
        assert_eq!(env.app.notifier.recorded().len(), 1, "told once");
    }

    // --- changesets -----------------------------------------------------------------

    /// `gh` that answers per repo folder: each submodule has its own PR.
    struct DirGh {
        /// Submodule folder name -> (PR number, state, head).
        prs: Mutex<HashMap<String, (i64, String, String)>>,
    }

    impl DirGh {
        fn set(&self, sub: &str, state: &str) {
            self.prs.lock().unwrap().get_mut(sub).unwrap().1 = state.into();
        }
    }

    impl Gh for DirGh {
        fn run(
            &self,
            dir: Option<&Path>,
            args: Vec<String>,
            _stdin: Option<String>,
        ) -> BoxFuture<'_, Result<String, GhError>> {
            let prs = self.prs.lock().unwrap().clone();
            let sub = dir
                .and_then(|d| d.file_name())
                .map(|n| n.to_string_lossy().into_owned());
            let reply = match (args[0].as_str(), args[1].as_str()) {
                ("repo", "view") => {
                    let sub = sub.unwrap();
                    Ok(REPO_VIEW.replace("o/r", &format!("o/{sub}")))
                }
                ("pr", "list") => {
                    let sub = sub.unwrap();
                    let Some((n, state, _)) = prs.get(&sub) else {
                        return Box::pin(async { Ok("[]".to_string()) });
                    };
                    Ok(format!(
                        r#"[{{"number":{n},"url":"https://github.com/o/{sub}/pull/{n}","state":"{state}","updatedAt":"2026-10-20T10:00:00Z"}}]"#
                    ))
                }
                ("api", "graphql") => {
                    let name = args.iter().find_map(|a| a.strip_prefix("name=")).unwrap();
                    let (_, state, head) = &prs[name];
                    Ok(ci_snapshot(state, head, "SUCCESS"))
                }
                _ => Err(GhError::Failed {
                    code: None,
                    stderr: "unsupported".into(),
                }),
            };
            Box::pin(async move { reply })
        }
    }

    struct Changeset {
        app: AppState,
        key: TaskKey,
        gh: Arc<DirGh>,
        root: PathBuf,
        ws: Workspace,
        _dir: TempDir,
    }

    /// The fixture parent project under `dir`; returns its root.
    fn fixture_parent(dir: &Path) -> PathBuf {
        let script =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scripts/make-fixture-parent.sh");
        let env_dir = dir.join("env");
        let out = Command::new("bash")
            .arg(script)
            .arg(&env_dir)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        env_dir.join("parent")
    }

    async fn changeset() -> Changeset {
        let (app, key, dir) =
            task_with_workspace(Arc::new(FakeGh::new()), Some(SpecPhase::Review)).await;
        let root = fixture_parent(dir.path());
        let subs = ["svc-payments", "svc-orders"];
        let spec = WorkspaceSpec {
            base: None,
            path: dir.path().join("cs-ws"),
            branch: Some(BRANCH.into()),
            kind: ProjectKind::Parent,
            submodules: subs.iter().map(ToString::to_string).collect(),
        };
        let ws = create_workspace(
            &root,
            &spec,
            &opts(),
            delilah_git::Deadline::after(std::time::Duration::from_secs(30)),
        )
        .unwrap()
        .0;
        let mut prs = HashMap::new();
        for (i, sub) in subs.iter().enumerate() {
            let head = commit_and_push(&ws.path.join(sub));
            prs.insert(sub.to_string(), (i as i64 + 1, "OPEN".to_string(), head));
        }
        app.store.save_workspace(&key, &root, &ws).await.unwrap();
        let gh = Arc::new(DirGh {
            prs: Mutex::new(prs),
        });
        let mut app = app.with_gh(gh.clone());
        let mut config = (*app.config).clone();
        config.allow_file_protocol = true;
        app.config = Arc::new(config);
        Changeset {
            app,
            key,
            gh,
            root,
            ws,
            _dir: dir,
        }
    }

    #[tokio::test]
    async fn a_changeset_has_a_pr_per_submodule_and_is_archived_when_all_are_merged() {
        let cs = changeset().await;
        // A branch of the parent's own: it must stay.
        git(&cs.root, &["branch", BRANCH]);

        check_task(&cs.app, &cs.key, true).await;
        let rows = cs.app.store.prs_of(&cs.key).await.unwrap();
        let mut repos: Vec<&str> = rows.iter().map(|r| r.repo.as_str()).collect();
        repos.sort_unstable();
        assert_eq!(repos, ["svc-orders", "svc-payments"]);
        let summary = crate::prs::summary("PROJ-1", &rows).unwrap();
        assert_eq!((summary.state, summary.count), (PrState::Open, 2));

        // One merged: the other is still open.
        cs.gh.set("svc-payments", "MERGED");
        check_task(&cs.app, &cs.key, true).await;
        assert_eq!(
            cs.app
                .store
                .get_task(&cs.key)
                .await
                .unwrap()
                .unwrap()
                .status,
            TaskStatus::Running
        );
        assert!(cs.ws.path.exists());
        let rows = cs.app.store.prs_of(&cs.key).await.unwrap();
        assert_eq!(
            crate::prs::summary("PROJ-1", &rows).unwrap().state,
            PrState::Open
        );

        // Both merged.
        cs.gh.set("svc-orders", "MERGED");
        check_task(&cs.app, &cs.key, true).await;
        let task = cs.app.store.get_task(&cs.key).await.unwrap().unwrap();
        assert_eq!(task.status, TaskStatus::Done);
        assert!(!cs.ws.path.exists());
        assert!(cs.app.store.workspace(&cs.key).await.unwrap().is_none());
        assert!(
            !git(&cs.root, &["branch", "--list", BRANCH]).is_empty(),
            "parent branch stays"
        );
        for sub in ["svc-payments", "svc-orders"] {
            assert!(git(&cs.root.join(sub), &["branch", "--list", BRANCH]).is_empty());
        }
        assert!(cs.app.store.open_notices().await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn a_changeset_with_one_pr_closed_is_not_archived() {
        let cs = changeset().await;
        check_task(&cs.app, &cs.key, true).await;
        cs.gh.set("svc-payments", "MERGED");
        cs.gh.set("svc-orders", "CLOSED");
        check_task(&cs.app, &cs.key, true).await;

        let task = cs.app.store.get_task(&cs.key).await.unwrap().unwrap();
        assert_eq!(task.status, TaskStatus::Running);
        assert!(cs.ws.path.exists());
        let rows = cs.app.store.prs_of(&cs.key).await.unwrap();
        let summary = crate::prs::summary("PROJ-1", &rows).unwrap();
        assert_eq!((summary.state, summary.count), (PrState::Closed, 2));
        let sent = cs.app.notifier.recorded();
        assert!(sent.iter().any(|n| n.title.contains("PROJ-1")));
        assert!(cs.app.store.open_notices().await.unwrap().is_empty());
    }

    // --- repos without a merged PR -------------------------------------------------

    /// A parent task in the project's checkout touching svc-payments (with a PR, merged)
    /// and svc-orders (no PR). The checkout never refuses to be let go of, so only the
    /// archive's own checks stand between svc-orders and a loss.
    async fn half_changeset() -> Changeset {
        let (app, key, dir) =
            task_with_workspace(Arc::new(FakeGh::new()), Some(SpecPhase::Review)).await;
        let root = fixture_parent(dir.path());
        let spec = WorkspaceSpec {
            base: None,
            path: dir.path().join("unused"),
            branch: Some(BRANCH.into()),
            kind: ProjectKind::Parent,
            submodules: vec!["svc-payments".into(), "svc-orders".into()],
        };
        let deadline = delilah_git::Deadline::after(std::time::Duration::from_secs(30));
        let (ws, previous, _) = delilah_git::use_checkout(&root, &spec, &opts(), deadline).unwrap();
        let head = commit_and_push(&root.join("svc-payments"));
        app.store.save_workspace(&key, &root, &ws).await.unwrap();
        app.store
            .set_checkout_previous(&key, &previous)
            .await
            .unwrap();
        let prs = HashMap::from([("svc-payments".to_string(), (1, "MERGED".to_string(), head))]);
        let gh = Arc::new(DirGh {
            prs: Mutex::new(prs),
        });
        let mut app = app.with_gh(gh.clone());
        let mut config = (*app.config).clone();
        config.allow_file_protocol = true;
        app.config = Arc::new(config);
        Changeset {
            app,
            key,
            gh,
            root,
            ws,
            _dir: dir,
        }
    }

    async fn assert_blocked_and_untouched(cs: &Changeset) {
        let task = cs.app.store.get_task(&cs.key).await.unwrap().unwrap();
        assert_eq!(task.status, TaskStatus::Done);
        assert!(cs.app.store.workspace(&cs.key).await.unwrap().is_some());
        for sub in ["svc-payments", "svc-orders"] {
            let dir = cs.root.join(sub);
            assert_eq!(git(&dir, &["branch", "--show-current"]), BRANCH);
        }
        let kinds: Vec<_> = cs
            .app
            .store
            .open_notices()
            .await
            .unwrap()
            .into_iter()
            .map(|n| n.kind)
            .collect();
        assert_eq!(kinds, ["archive-blocked"]);
        let sent = cs.app.notifier.recorded();
        assert_eq!(sent.len(), 1, "{sent:?}");
        let not_archived = cs
            .app
            .lang()
            .pick("mergeada, sin archivar", "merged, not archived");
        assert_eq!(sent[0].title, format!("PROJ-1 {not_archived}"));
    }

    #[tokio::test]
    async fn a_repo_without_a_pr_and_with_uncommitted_work_blocks_the_archive() {
        let cs = half_changeset().await;
        std::fs::write(cs.root.join("svc-orders/wip.txt"), "wip").unwrap();

        check_task(&cs.app, &cs.key, true).await;
        check_task(&cs.app, &cs.key, true).await;

        assert_blocked_and_untouched(&cs).await;
        assert!(cs.root.join("svc-orders/wip.txt").exists());
    }

    #[tokio::test]
    async fn a_repo_without_a_pr_and_with_an_unpushed_commit_blocks_the_archive() {
        let cs = half_changeset().await;
        let orders = cs.root.join("svc-orders");
        std::fs::write(orders.join("mine.txt"), "mine").unwrap();
        git(&orders, &["add", "."]);
        git(&orders, &["commit", "-q", "-m", "mine"]);

        check_task(&cs.app, &cs.key, true).await;

        assert_blocked_and_untouched(&cs).await;
    }

    #[tokio::test]
    async fn a_clean_repo_without_a_pr_does_not_stop_the_archive_and_keeps_its_branch() {
        let cs = half_changeset().await;

        check_task(&cs.app, &cs.key, true).await;

        let task = cs.app.store.get_task(&cs.key).await.unwrap().unwrap();
        assert_eq!(task.status, TaskStatus::Done);
        assert!(cs.app.store.workspace(&cs.key).await.unwrap().is_none());
        assert!(cs.app.store.open_notices().await.unwrap().is_empty());
        // Merged: its branch goes. Not merged (no PR): it stays.
        assert!(git(&cs.root.join("svc-payments"), &["branch", "--list", BRANCH]).is_empty());
        assert!(!git(&cs.root.join("svc-orders"), &["branch", "--list", BRANCH]).is_empty());
        let _ = &cs.ws;
        let sent = cs.app.notifier.recorded();
        assert_eq!(sent.len(), 1, "{sent:?}");
        let archived = cs
            .app
            .lang()
            .pick("mergeada y archivada", "merged and archived");
        assert_eq!(sent[0].title, format!("PROJ-1 {archived}"));
    }

    #[tokio::test]
    async fn a_branch_that_moved_since_the_check_is_not_deleted() {
        let env = Env::repo(true, "MERGED").await;
        // The sessions are gone and the check passed: now something commits.
        let tip = git(&env.root, &["rev-parse", BRANCH]);
        git(&env.root, &["checkout", "-q", "main"]);
        git(&env.root, &["checkout", "-q", BRANCH]);
        std::fs::write(env.root.join("late.txt"), "late").unwrap();
        git(&env.root, &["add", "."]);
        git(&env.root, &["commit", "-q", "-m", "late"]);
        git(&env.root, &["checkout", "-q", "main"]);

        let outcome = crate::archive::archive_workspace(
            &env.app,
            &env.key,
            false,
            &HashMap::from([(".".to_string(), tip)]),
        )
        .await
        .map_err(|e| e.message)
        .unwrap();

        assert_eq!(outcome.left.len(), 1);
        assert!(env.branch_exists(&env.root));
    }

    #[tokio::test]
    async fn one_notification_per_merge_and_per_pr_for_all_but_the_last() {
        let env = Env::repo(false, "MERGED").await;
        check_task(&env.app, &env.key, true).await;
        let sent = env.app.notifier.recorded();
        assert_eq!(sent.len(), 1, "{sent:?}");
        // The merge-driven MarkDone is quiet: no "terminada" either.
        let done = env.app.lang().pick("terminada", "done");
        assert!(sent.iter().all(|n| !n.title.ends_with(done)));

        let cs = changeset().await;
        check_task(&cs.app, &cs.key, true).await;
        cs.gh.set("svc-payments", "MERGED");
        check_task(&cs.app, &cs.key, true).await;
        let merged = cs.app.lang().pick("mergeada", "merged");
        let sent = cs.app.notifier.recorded();
        assert_eq!(sent.len(), 1, "{sent:?}");
        assert_eq!(sent[0].title, format!("PROJ-1 {merged}"));
        cs.gh.set("svc-orders", "MERGED");
        check_task(&cs.app, &cs.key, true).await;
        let archived = cs
            .app
            .lang()
            .pick("mergeada y archivada", "merged and archived");
        let sent = cs.app.notifier.recorded();
        assert_eq!(sent.len(), 2, "{sent:?}");
        assert_eq!(sent[1].title, format!("PROJ-1 {archived}"));
    }

    #[tokio::test]
    async fn a_refusal_is_told_once_however_often_it_is_checked() {
        let env = Env::repo(false, "MERGED").await;
        std::fs::write(env.ws.path.join("wip.txt"), "wip").unwrap();
        check_task(&env.app, &env.key, true).await;
        check_task(&env.app, &env.key, true).await;
        let sent = env.app.notifier.recorded();
        assert_eq!(sent.len(), 1, "{sent:?}");
    }

    #[tokio::test]
    async fn a_failed_fetch_blocks_the_archive() {
        let env = Env::repo(false, "MERGED").await;
        git(
            &env.root,
            &["remote", "set-url", "origin", "/nonexistent/origin.git"],
        );

        check_task(&env.app, &env.key, true).await;

        assert!(env.ws.path.exists());
        assert!(env.branch_exists(&env.root));
        assert_eq!(env.kinds().await, ["archive-blocked"]);
    }

    /// A parent task P-1 with subtasks C-1..C-n (one PR each, `states`), and a PR of
    /// P-1's own (a draft when `draft`).
    async fn hierarchy(states: &[PrState], draft: bool) -> AppState {
        use crate::store_prs::NewPr;
        use delilah_core::hierarchy::TaskKind;

        let app = AppState::new(
            crate::store::Store::in_memory().await.unwrap(),
            crate::config::Config::test_defaults(),
        );
        let mut p = Task::new(TaskKey::new("P-1"), "parent", "app");
        p.kind = TaskKind::ParentTask;
        app.store.create_task(&p).await.unwrap();
        let put = |key: String, n: i64, state: PrState, draft: bool| {
            let app = app.clone();
            async move {
                let row = app
                    .store
                    .link_pr(
                        &TaskKey::new(key),
                        &NewPr {
                            repo: ".".into(),
                            host: "github.com".into(),
                            owner_name: "me/repo".into(),
                            number: n,
                            url: format!("https://github.com/me/repo/pull/{n}"),
                            linked_by: "detected".into(),
                        },
                    )
                    .await
                    .unwrap();
                let mut snap = crate::prs::tests::snapshot(state, vec![]);
                snap.draft = draft;
                app.store
                    .save_check(
                        row.id,
                        &snap,
                        &Default::default(),
                        chrono::Utc::now(),
                        &[],
                        &[],
                    )
                    .await
                    .unwrap();
            }
        };
        for (i, state) in states.iter().enumerate() {
            let key = format!("C-{}", i + 1);
            let mut c = Task::new(TaskKey::new(&key), "child", "app");
            c.kind = TaskKind::Subtask;
            c.parent_task = Some(TaskKey::new("P-1"));
            app.store.create_task(&c).await.unwrap();
            put(key, 10 + i as i64, *state, false).await;
        }
        put("P-1".into(), 1, PrState::Open, draft).await;
        app
    }

    async fn ready_notices(app: &AppState) -> usize {
        app.store
            .open_notices()
            .await
            .unwrap()
            .iter()
            .filter(|n| n.kind == KIND_PARENT_TASK_READY && n.task_key == "P-1")
            .count()
    }

    #[tokio::test]
    async fn a_subtask_merge_tells_the_tasks_topic_and_the_last_one_makes_the_parent_ready_once() {
        let app = hierarchy(&[PrState::Merged, PrState::Merged], true).await;
        let mut rx = app.events.subscribe();
        let c = app
            .store
            .get_task(&TaskKey::new("C-2"))
            .await
            .unwrap()
            .unwrap();
        on_subtask_merged(&app, &c, true).await;
        let mut seen = vec![];
        while let Ok(t) = rx.try_recv() {
            seen.push(t);
        }
        assert!(seen.contains(&Topic::Tasks) && seen.contains(&Topic::Inbox));
        assert_eq!(ready_notices(&app).await, 1);
        // Later ticks do not push it again.
        on_subtask_merged(&app, &c, false).await;
        parent_ready(&app, &TaskKey::new("P-1")).await;
        assert_eq!(ready_notices(&app).await, 1);
        assert!(rx.try_recv().is_err(), "nothing new to tell");
    }

    #[tokio::test]
    async fn the_parent_is_not_ready_with_an_open_subtask_or_without_a_draft_pr() {
        let app = hierarchy(&[PrState::Merged, PrState::Open], true).await;
        parent_ready(&app, &TaskKey::new("P-1")).await;
        assert_eq!(ready_notices(&app).await, 0);
        let app = hierarchy(&[PrState::Merged, PrState::Merged], false).await;
        parent_ready(&app, &TaskKey::new("P-1")).await;
        assert_eq!(ready_notices(&app).await, 0);
    }

    #[tokio::test]
    async fn two_subtasks_merged_in_one_tick_give_the_right_progress() {
        let app = hierarchy(&[PrState::Merged, PrState::Merged, PrState::Open], true).await;
        let p = crate::integration::progress(&app.store, &TaskKey::new("P-1"))
            .await
            .unwrap();
        assert_eq!((p.merged, p.counted, p.ready), (2, 3, false));
        for k in ["C-1", "C-2"] {
            let c = app.store.get_task(&TaskKey::new(k)).await.unwrap().unwrap();
            on_subtask_merged(&app, &c, true).await;
        }
        assert_eq!(ready_notices(&app).await, 0, "the third is still open");
    }

    #[tokio::test]
    async fn a_merged_parent_task_with_live_subtasks_is_done_but_not_archived() {
        let env = Env::repo(false, "MERGED").await;
        env.app
            .store
            .set_kind(&env.key, TaskKind::ParentTask, None)
            .await
            .unwrap();
        let mut c = Task::new(TaskKey::new("C-9"), "child", "app");
        c.kind = TaskKind::Subtask;
        c.parent_task = Some(env.key.clone());
        env.app.store.create_task(&c).await.unwrap();

        check_task(&env.app, &env.key, true).await;

        assert_eq!(env.status().await, TaskStatus::Done);
        assert!(env.ws.path.exists());
        assert!(env.app.store.workspace(&env.key).await.unwrap().is_some());
        assert_eq!(env.kinds().await, ["parent-task-live"]);
        // Told once.
        check_task(&env.app, &env.key, true).await;
        assert_eq!(env.kinds().await, ["parent-task-live"]);
    }
}
