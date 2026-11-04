//! Fixtures shared by the tests of the PR loop and the PR routes.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use delilah_core::{Event, Project, ProjectKind, SpecPhase, Task, TaskKey};
use delilah_git::Workspace;

use crate::config::Config;
use crate::gh::fake::FakeGh;
use crate::state::AppState;
use crate::store::Store;

/// A folder under the system temp dir, removed on drop.
pub(crate) struct TempDir(PathBuf);

impl TempDir {
    pub(crate) fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Task `PROJ-1` of the repo project `proj`, running, with a workspace on
/// `feature/PROJ-1`, advanced to `phase`, talking to `gh`. Keep the guard alive.
pub(crate) async fn task_with_workspace(
    gh: Arc<FakeGh>,
    phase: Option<SpecPhase>,
) -> (AppState, TaskKey, TempDir) {
    let dir = TempDir(std::env::temp_dir().join(format!("delilah-pr-{}", uuid::Uuid::new_v4())));
    let ws_path = dir.path().join("ws");
    std::fs::create_dir_all(&ws_path).unwrap();
    let config = Config {
        home: dir.path().join("home"),
        addr: "127.0.0.1:7420".parse().unwrap(),
        allow_file_protocol: false,
        notify: false,
        ui_origin: None,
        claude_bin: "claude".into(),
        agent_program: None,
        ..crate::config::Config::test_defaults()
    };
    let app = AppState::new(Store::in_memory().await.unwrap(), config).with_gh(gh);
    app.lang.set(crate::lang::Lang::Es); // Spanish fixtures
    let key = TaskKey::new("PROJ-1");
    app.store
        .create_project(&Project {
            name: "proj".into(),
            path: "/parent".into(),
            kind: ProjectKind::Repo,
            workflow: "project".into(),
            settings: Default::default(),
            preview_command: None,
        })
        .await
        .unwrap();
    let mut task = Task::new(key.clone(), "t", "project");
    task.project = Some("proj".into());
    app.store.create_task(&task).await.unwrap();
    app.transition(&key, Event::Start).await.unwrap();
    app.transition(&key, Event::WorkspaceReady).await.unwrap();
    let ws = Workspace {
        path: ws_path,
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
    (app, key, dir)
}

fn git(dir: &Path, args: &[&str]) {
    let out = std::process::Command::new("git")
        .args([
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@t",
            "-c",
            "commit.gpgsign=false",
        ])
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// Makes `repo` a git repo on `main` with a bare `origin` at `origin`, then checks out
/// `branch` with one commit per subject in `subjects` (named `<subject>.txt`).
pub(crate) fn git_repo_with_origin(repo: &Path, origin: &Path, branch: &str, subjects: &[&str]) {
    std::fs::create_dir_all(repo).unwrap();
    std::fs::create_dir_all(origin).unwrap();
    git(origin, &["init", "--bare", "-b", "main"]);
    git(repo, &["init", "-b", "main"]);
    git(repo, &["commit", "--allow-empty", "-m", "base"]);
    git(repo, &["remote", "add", "origin", origin.to_str().unwrap()]);
    git(repo, &["checkout", "-b", branch]);
    for subject in subjects {
        std::fs::write(repo.join(format!("{subject}.txt")), subject).unwrap();
        git(repo, &["add", "."]);
        git(repo, &["commit", "-m", subject]);
    }
}

/// `gh` that answers per repo folder (the folder name is the repo name under `o/`):
/// `pr list`, `repo view` and `api graphql`. Folders without a PR answer `[]`.
#[derive(Default)]
pub(crate) struct DirGh {
    /// Folder name -> (PR number, state, head sha).
    prs: std::sync::Mutex<std::collections::HashMap<String, (i64, String, String)>>,
    /// Head branch the snapshots report, when not the default of `ci_snapshot`.
    head_ref: std::sync::Mutex<Option<String>>,
    /// Folders whose `pr list` fails.
    failing: std::sync::Mutex<std::collections::HashSet<String>>,
    /// Runs on every `pr list`, to change the world while a search is under way.
    on_list: std::sync::Mutex<Option<ListHook>>,
}

type ListHook = Box<dyn Fn() + Send + Sync>;

impl DirGh {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// Gives `folder` a PR (`https://github.com/o/<folder>/pull/<number>`).
    pub(crate) fn with_pr(self, folder: &str, number: i64, state: &str, head: &str) -> Self {
        self.set_pr(folder, number, state, head);
        self
    }

    /// Makes the snapshots report `branch` as their head branch.
    pub(crate) fn with_head_ref(self, branch: &str) -> Self {
        *self.head_ref.lock().unwrap() = Some(branch.into());
        self
    }

    /// Makes `pr list` fail in `folder`.
    pub(crate) fn fail_list(&self, folder: &str) {
        self.failing.lock().unwrap().insert(folder.into());
    }

    /// Runs `hook` on every `pr list`.
    pub(crate) fn on_list(&self, hook: impl Fn() + Send + Sync + 'static) {
        *self.on_list.lock().unwrap() = Some(Box::new(hook));
    }

    pub(crate) fn set_pr(&self, folder: &str, number: i64, state: &str, head: &str) {
        self.prs
            .lock()
            .unwrap()
            .insert(folder.into(), (number, state.into(), head.into()));
    }
}

impl crate::gh::Gh for DirGh {
    fn run(
        &self,
        dir: Option<&Path>,
        args: Vec<String>,
        _stdin: Option<String>,
    ) -> futures_util::future::BoxFuture<'_, Result<String, crate::gh::GhError>> {
        use crate::gh::fake::{REPO_VIEW, ci_snapshot};
        let prs = self.prs.lock().unwrap().clone();
        let folder = dir
            .and_then(|d| d.file_name())
            .map(|n| n.to_string_lossy().into_owned());
        let unsupported = || crate::gh::GhError::Failed {
            code: None,
            stderr: "unsupported".into(),
        };
        let reply = match (
            args.first().map(String::as_str),
            args.get(1).map(String::as_str),
        ) {
            (Some("repo"), Some("view")) => folder
                .map(|f| REPO_VIEW.replace("o/r", &format!("o/{f}")))
                .ok_or_else(unsupported),
            (Some("pr"), Some("list")) => {
                if let Some(hook) = self.on_list.lock().unwrap().as_ref() {
                    hook();
                }
                if folder
                    .as_ref()
                    .is_some_and(|f| self.failing.lock().unwrap().contains(f))
                {
                    return Box::pin(async move { Err(unsupported()) });
                }
                match folder.and_then(|f| prs.get(&f).map(|p| (f, p.clone()))) {
                    Some((f, (n, state, _))) => Ok(format!(
                        r#"[{{"number":{n},"url":"https://github.com/o/{f}/pull/{n}","state":"{state}","updatedAt":"2026-10-20T10:00:00Z"}}]"#
                    )),
                    None => Ok("[]".to_string()),
                }
            }
            (Some("api"), Some("graphql")) => args
                .iter()
                .find_map(|a| a.strip_prefix("name="))
                .and_then(|name| prs.get(name))
                .map(|(_, state, head)| {
                    let snap = ci_snapshot(state, head, "SUCCESS");
                    match self.head_ref.lock().unwrap().as_deref() {
                        Some(b) => snap.replace("feat/x", b),
                        None => snap,
                    }
                })
                .ok_or_else(unsupported),
            _ => Err(unsupported()),
        };
        Box::pin(async move { reply })
    }
}

/// A parent project under `base` with submodules `svc-a` and `svc-b` (bare remotes under
/// `base/remotes`, as GitHub) and a workspace `base/ws` on `branch` created from it, as
/// the daemon does for a parent task, with the parent's own repo on `branch` too. Returns the parent checkout and the workspace; the
/// submodule folders are `ws.path/svc-a` and `ws.path/svc-b`. Pair it with [`DirGh`].
pub(crate) fn parent_workspace(base: &Path, branch: &str) -> (PathBuf, Workspace) {
    use delilah_git::{Deadline, GitOptions, WorkspaceSpec, create_workspace};

    let remotes = base.join("remotes");
    let sub_remote = |name: &str| {
        let (seed, bare) = (
            base.join("seed").join(name),
            remotes.join(format!("{name}.git")),
        );
        git_repo_with_origin(&seed, &bare, "tmp", &[]);
        git(&seed, &["push", "origin", "main"]);
        bare
    };
    let (a, b) = (sub_remote("svc-a"), sub_remote("svc-b"));
    let root = base.join("parent");
    let origin = remotes.join("parent.git");
    git_repo_with_origin(&root, &origin, "tmp", &[]);
    git(&root, &["checkout", "main"]);
    for (name, remote) in [("svc-a", &a), ("svc-b", &b)] {
        git(
            &root,
            &[
                "-c",
                "protocol.file.allow=always",
                "submodule",
                "add",
                "-b",
                "main",
                remote.to_str().unwrap(),
                name,
            ],
        );
    }
    git(&root, &["commit", "-m", "add submodules"]);
    git(&root, &["push", "origin", "main"]);
    let spec = WorkspaceSpec {
        base: None,
        path: base.join("ws"),
        branch: Some(branch.into()),
        kind: ProjectKind::Parent,
        submodules: vec!["svc-a".into(), "svc-b".into()],
    };
    let opts = GitOptions {
        allow_file_protocol: true,
    };
    let ws = create_workspace(
        &root,
        &spec,
        &opts,
        Deadline::after(std::time::Duration::from_secs(30)),
    )
    .unwrap()
    .0;
    // A new workspace starts detached in the parent's own repo; a task whose descriptive
    // branch was made after the start is on it (`Workspace::parent_has_branch`).
    git(&ws.path, &["checkout", "-b", branch]);
    (root, ws)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gh::Gh;

    #[test]
    fn parent_workspace_has_both_submodules_on_the_task_branch() {
        let dir =
            TempDir(std::env::temp_dir().join(format!("delilah-pw-{}", uuid::Uuid::new_v4())));
        let (_root, ws) = parent_workspace(dir.path(), "feature/PROJ-1");
        for sub in ["svc-a", "svc-b"] {
            assert!(ws.path.join(sub).join(".git").exists(), "{sub}");
        }
        assert_eq!(ws.branch, "feature/PROJ-1");
        let head = std::process::Command::new("git")
            .args(["branch", "--show-current"])
            .current_dir(&ws.path)
            .output()
            .unwrap();
        assert_eq!(
            String::from_utf8_lossy(&head.stdout).trim(),
            "feature/PROJ-1"
        );
    }

    #[tokio::test]
    async fn dir_gh_answers_per_repo_folder() {
        let gh = DirGh::new().with_pr("svc-a", 12, "OPEN", "abc");
        let a = gh
            .run(
                Some(Path::new("/x/svc-a")),
                vec!["pr".into(), "list".into()],
                None,
            )
            .await
            .unwrap();
        assert!(a.contains("https://github.com/o/svc-a/pull/12"));
        let b = gh
            .run(
                Some(Path::new("/x/svc-b")),
                vec!["pr".into(), "list".into()],
                None,
            )
            .await
            .unwrap();
        assert_eq!(b, "[]");
    }
}
