//! Archive on merge (research R7): when every PR of a task is merged, the task is marked
//! done and its workspace released, unless that would lose work. Never deletes a remote
//! branch.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use delilah_core::{Event, TaskKey};
use delilah_git::GitOptions;
use serde_json::json;

use crate::actions::{ArchivePartial, LeftoverBranch, release_workspace};
use crate::events::Topic;
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

/// The repo folder that holds the task's branch once the workspace is gone: the
/// project's own checkout (`root`), or its submodule `label` when that one is set up.
fn home_of(root: &Path, label: &str) -> Option<PathBuf> {
    if label == "." {
        return Some(root.to_path_buf());
    }
    let dir = root.join(label);
    // An uninitialised submodule folder would make git answer for the parent.
    dir.join(".git").exists().then_some(dir)
}

/// The task's PRs are all merged: mark it done and archive it, keeping any work that
/// is not in the PRs (`archive-blocked`) and reporting a half-done archive
/// (`archive-partial`). Does nothing for a task already archived.
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
    if !task.status.is_terminal() {
        if let Err(e) = app.transition(key, Event::MarkDone).await {
            tracing::warn!(task = %key, "cannot mark it done after the merge: {e}");
            return Ok(());
        }
        for live in app.sessions.of_task(key.as_str()) {
            let _ = live.pty.kill();
        }
    }
    let lang = app.lang();
    if let Some((root, ws)) = workspace {
        let opts = GitOptions {
            allow_file_protocol: app.config.allow_file_protocol,
        };
        let heads: HashMap<String, String> = rows
            .iter()
            .filter_map(|r| Some((r.repo.clone(), r.snapshot.as_ref()?.head.clone())))
            .collect();
        let checked = {
            let (ws, opts) = (ws.clone(), opts.clone());
            tokio::task::spawn_blocking(move || {
                for (label, dir) in ws.repos() {
                    if heads.contains_key(&label)
                        && let Err(e) = delilah_git::fetch(&dir, &opts)
                    {
                        tracing::warn!("fetching {label}: {e}");
                    }
                }
                delilah_git::check_merged(&ws, &heads, &opts)
            })
            .await
        };
        let refused = match checked {
            Ok(Ok(())) => None,
            Ok(Err(e)) => Some(e.to_string()),
            Err(e) => Some(e.to_string()),
        };
        if let Some(reason) = refused {
            return blocked(app, key, &reason).await;
        }

        let previous_dirs: Vec<LeftoverBranch> = ws
            .repos()
            .into_iter()
            .filter_map(|(label, _)| home_of(&root, &label))
            .map(|dir| LeftoverBranch {
                dir,
                branch: ws.branch.clone(),
            })
            .collect();
        if let Err(e) = release_workspace(app, key, true).await {
            return blocked(app, key, &e.message).await;
        }
        let mut left: Vec<LeftoverBranch> = Vec::new();
        let mut problems: Vec<String> = Vec::new();
        if let Err(e) = app.store.delete_workspace(key).await {
            problems.push(e.to_string());
            left = previous_dirs.clone();
        } else {
            for leftover in previous_dirs {
                let (dir, branch, opts) =
                    (leftover.dir.clone(), leftover.branch.clone(), opts.clone());
                let result = tokio::task::spawn_blocking(move || {
                    delilah_git::delete_branch(&dir, &branch, &opts)
                })
                .await;
                match result {
                    Ok(Ok(())) => {}
                    Ok(Err(e)) => {
                        problems.push(e.to_string());
                        left.push(leftover);
                    }
                    Err(e) => {
                        problems.push(e.to_string());
                        left.push(leftover);
                    }
                }
            }
            // Nothing left to act on: the notices of the PRs go with the workspace.
            app.store.resolve_all_notices_of_task(key).await?;
        }
        if !left.is_empty() {
            let text = format!(
                "{}: {}",
                lang.pick(
                    "Archivada a medias, quedan ramas locales",
                    "Archived only partly, local branches are left"
                ),
                problems.join("; ")
            );
            let detail = serde_json::to_value(ArchivePartial { branches: left }).ok();
            app.store
                .insert_notice(
                    key,
                    &NewNotice {
                        repo: ".".into(),
                        kind: "archive-partial".into(),
                        cause: "merge".into(),
                        text,
                        detail,
                    },
                )
                .await?;
        }
    }
    if !app.events.listening() {
        app.notifier.send(
            format!(
                "{key} {}",
                lang.pick("mergeada y archivada", "merged and archived")
            ),
            task.title.clone(),
        );
    }
    for topic in [Topic::Tasks, Topic::Prs, Topic::Inbox, Topic::Specs] {
        app.emit(topic);
    }
    Ok(())
}

/// The archive was refused: the task stays done with its workspace, and the inbox says why.
async fn blocked(app: &AppState, key: &TaskKey, reason: &str) -> Result<(), StoreError> {
    let lang = app.lang();
    let text = format!(
        "{}: {reason}",
        lang.pick(
            "No se archivó la tarea mergeada",
            "The merged task was not archived"
        )
    );
    app.store
        .insert_notice(
            key,
            &NewNotice {
                repo: ".".into(),
                kind: "archive-blocked".into(),
                cause: "merge".into(),
                text,
                detail: Some(json!({ "reason": reason })),
            },
        )
        .await?;
    app.emit(Topic::Inbox);
    app.emit(Topic::Tasks);
    Ok(())
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
                    path: base.join("ws"),
                    branch: BRANCH.into(),
                    kind: ProjectKind::Repo,
                    submodules: vec![],
                };
                create_workspace(&root, &spec, &opts()).unwrap()
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
                    let (n, state, _) = &prs[&sub];
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

    async fn changeset() -> Changeset {
        let (app, key, dir) =
            task_with_workspace(Arc::new(FakeGh::new()), Some(SpecPhase::Review)).await;
        let script =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scripts/make-fixture-parent.sh");
        let env_dir = dir.path().join("env");
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
        let root = env_dir.join("parent");
        let subs = ["svc-payments", "svc-orders"];
        let spec = WorkspaceSpec {
            path: dir.path().join("cs-ws"),
            branch: BRANCH.into(),
            kind: ProjectKind::Parent,
            submodules: subs.iter().map(ToString::to_string).collect(),
        };
        let ws = create_workspace(&root, &spec, &opts()).unwrap();
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
}
