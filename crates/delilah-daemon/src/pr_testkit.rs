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
