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
    };
    let app = AppState::new(Store::in_memory().await.unwrap(), config).with_gh(gh);
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
