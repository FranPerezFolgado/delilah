//! Free tasks start without a branch or a real name: the first analysis creates a spec
//! folder, and the task takes its name (and branch) from it. This module finds that
//! folder and runs the triggers; two triggers never run together for one task.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use delilah_core::{Task, TaskKey, TitleSource};
use delilah_git::Workspace;

use crate::state::AppState;

/// One lock per task, so a hook and a recovery tick never work on the same task at once.
#[derive(Default)]
pub struct BranchLocks {
    locks: Mutex<HashMap<String, Arc<tokio::sync::Mutex<()>>>>,
}

impl BranchLocks {
    pub async fn lock(&self, key: &TaskKey) -> tokio::sync::OwnedMutexGuard<()> {
        let lock = {
            let mut locks = self.locks.lock().unwrap_or_else(|e| e.into_inner());
            locks.entry(key.to_string()).or_default().clone()
        };
        lock.lock_owned().await
    }
}

/// Whether the key is a free task's (`LIBRE-n`), as opposed to a Jira ticket's.
pub(crate) fn is_free(key: &TaskKey) -> bool {
    key.as_str().starts_with("LIBRE-")
}

/// The repos to look for spec folders in, by label: the main repo (`.`) and, for a
/// parent project, each chosen submodule (by path).
pub(crate) fn spec_repos(ws: &Workspace) -> Vec<(String, PathBuf)> {
    let mut repos = vec![(".".to_string(), ws.path.clone())];
    for sub in &ws.submodules {
        repos.push((sub.clone(), ws.path.join(sub)));
    }
    repos
}

/// The spec folders now in the workspace, as `label:name`.
pub(crate) fn scan_spec_dirs(ws: &Workspace) -> Vec<String> {
    spec_repos(ws)
        .into_iter()
        .flat_map(|(label, dir)| {
            delilah_git::spec_dirs(&dir)
                .into_iter()
                .map(move |name| format!("{label}:{name}"))
        })
        .collect()
}

/// The first spec folder (with a `spec.md`) that was not there when the task started:
/// its repo's label and its path. `None` without a workspace, a record of the start,
/// or a new folder.
#[allow(dead_code)] // used by the retitle step
pub(crate) async fn new_spec_folder(app: &AppState, key: &TaskKey) -> Option<(String, PathBuf)> {
    let (_, ws) = app.store.workspace(key).await.ok()??;
    let at_start = app.store.spec_dirs_at_start(key).await.ok()??;
    for (label, dir) in spec_repos(&ws) {
        for name in delilah_git::spec_dirs(&dir) {
            if !at_start.contains(&format!("{label}:{name}")) {
                return Some((label, dir.join("specs").join(name)));
            }
        }
    }
    None
}

/// Whether the task still has something to take from its first analysis: a free task
/// that has no branch yet or still carries its initial title.
async fn pending(app: &AppState, task: &Task) -> bool {
    if !is_free(&task.key) {
        return false;
    }
    if task.title_source == TitleSource::Initial {
        return true;
    }
    match app.store.workspace(&task.key).await {
        Ok(Some((_, ws))) => !ws.has_branch(),
        _ => false,
    }
}

/// A free task did something (a tool ran, a turn ended): its analysis may be there now.
pub async fn on_activity(app: &AppState, key: &TaskKey) {
    if !is_free(key) {
        return;
    }
    let task = match app.store.get_task(key).await {
        Ok(Some(task)) => task,
        Ok(None) => return,
        Err(e) => return tracing::warn!(task = %key, "branching: {e}"),
    };
    let _guard = app.branching.lock(key).await;
    if !pending(app, &task).await {
        return;
    }
    // The retitle and branch steps come here.
}

/// Catches up on an analysis written while the daemon was down (or missed): at start
/// and on every tick.
pub async fn recover(app: &AppState, task: &Task) {
    if !is_free(&task.key) {
        return;
    }
    let _guard = app.branching.lock(&task.key).await;
    if !pending(app, task).await {
        return;
    }
    // The retitle and branch steps come here.
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use delilah_core::ProjectKind;

    use super::*;
    use crate::config::Config;
    use crate::store::Store;

    async fn app() -> AppState {
        let config = Config {
            home: std::env::temp_dir().join(format!("delilah-branching-{}", uuid::Uuid::new_v4())),
            addr: "127.0.0.1:7420".parse().unwrap(),
            allow_file_protocol: false,
            notify: false,
            ui_origin: None,
            agent_program: None,
        };
        AppState::new(Store::in_memory().await.unwrap(), config)
    }

    fn spec(root: &Path, name: &str, with_spec: bool) {
        let dir = root.join("specs").join(name);
        std::fs::create_dir_all(&dir).unwrap();
        if with_spec {
            std::fs::write(dir.join("spec.md"), "# spec").unwrap();
        }
    }

    /// A free task with a branchless workspace of `kind` in a fresh folder.
    async fn task_with(app: &AppState, kind: ProjectKind, subs: &[&str]) -> (TaskKey, PathBuf) {
        let root = std::env::temp_dir().join(format!("delilah-ws-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let key = TaskKey::new("LIBRE-1");
        let mut task = Task::new(key.clone(), "Idea", "feature");
        task.title_source = TitleSource::Initial;
        app.store.create_task(&task).await.unwrap();
        let ws = Workspace {
            path: root.clone(),
            branch: String::new(),
            kind,
            submodules: subs.iter().map(|s| s.to_string()).collect(),
        };
        app.store.save_workspace(&key, &root, &ws).await.unwrap();
        (key, root)
    }

    async fn record(app: &AppState, key: &TaskKey) {
        let (_, ws) = app.store.workspace(key).await.unwrap().unwrap();
        let dirs = scan_spec_dirs(&ws);
        app.store
            .set_spec_dirs_at_start(key, Some(&dirs))
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn folders_from_before_the_start_are_ignored() {
        let app = app().await;
        let (key, root) = task_with(&app, ProjectKind::Repo, &[]).await;
        spec(&root, "001-old", true);
        record(&app, &key).await;
        assert_eq!(new_spec_folder(&app, &key).await, None);
        spec(&root, "002-new", true);
        assert_eq!(
            new_spec_folder(&app, &key).await,
            Some((".".into(), root.join("specs/002-new")))
        );
    }

    #[tokio::test]
    async fn a_new_folder_in_a_submodule_is_found() {
        let app = app().await;
        let (key, root) = task_with(&app, ProjectKind::Parent, &["api"]).await;
        spec(&root, "001-old", true);
        record(&app, &key).await;
        spec(&root.join("api"), "003-card", true);
        assert_eq!(
            new_spec_folder(&app, &key).await,
            Some(("api".into(), root.join("api/specs/003-card")))
        );
    }

    #[tokio::test]
    async fn a_folder_without_spec_md_is_ignored() {
        let app = app().await;
        let (key, root) = task_with(&app, ProjectKind::Repo, &[]).await;
        record(&app, &key).await;
        spec(&root, "001-draft", false);
        assert_eq!(new_spec_folder(&app, &key).await, None);
    }

    #[tokio::test]
    async fn jira_tasks_and_unrecorded_starts_have_no_new_folder() {
        let app = app().await;
        let (key, root) = task_with(&app, ProjectKind::Repo, &[]).await;
        spec(&root, "001-x", true);
        // Never recorded (started before the feature, or with a branch).
        assert_eq!(new_spec_folder(&app, &key).await, None);
        assert!(is_free(&key));
        assert!(!is_free(&TaskKey::new("PROJ-1")));
    }

    #[tokio::test]
    async fn the_locks_are_per_task() {
        let app = app().await;
        let a = app.branching.lock(&TaskKey::new("LIBRE-1")).await;
        // Another task is not held up by it.
        let _b = app.branching.lock(&TaskKey::new("LIBRE-2")).await;
        drop(a);
        let _again = app.branching.lock(&TaskKey::new("LIBRE-1")).await;
    }
}
