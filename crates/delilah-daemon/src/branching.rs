//! Free tasks start without a branch or a real name: the first analysis creates a spec
//! folder, and the task takes its name (and branch) from it. This module finds that
//! folder and runs the triggers; two triggers never run together for one task.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
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

/// The feature name in a `spec.md`: the text after `# Feature Specification:`, trimmed.
/// `None` without the heading, with an empty name or with a template placeholder
/// (`[FEATURE NAME]`, any `[...]`-only name).
pub(crate) fn spec_title(path: &Path) -> Option<String> {
    let text = std::fs::read_to_string(path).ok()?;
    let name = text
        .lines()
        .find_map(|line| line.trim_start().strip_prefix("# Feature Specification:"))?
        .trim();
    let placeholder = name.starts_with('[') && name.ends_with(']');
    (!name.is_empty() && !placeholder).then(|| name.to_string())
}

/// Gives a free task still on its initial title the feature name of its first analysis,
/// once. Call it holding the task's lock.
pub(crate) async fn retitle(app: &AppState, key: &TaskKey) {
    if !is_free(key) {
        return;
    }
    let Some((_, folder)) = new_spec_folder(app, key).await else {
        return;
    };
    let Some(title) = spec_title(&folder.join("spec.md")) else {
        return;
    };
    // The WHERE on the initial title keeps it to once, whatever else is going on.
    match app.store.rename_from_spec(key, &title).await {
        Ok(Some(_)) => app.emit(crate::events::Topic::Tasks),
        Ok(None) => {}
        Err(e) => tracing::warn!(task = %key, "branching: {e}"),
    }
}

/// Takes what is pending from the first analysis, on the task as it is now.
async fn step(app: &AppState, key: &TaskKey) {
    let task = match app.store.get_task(key).await {
        Ok(Some(task)) => task,
        Ok(None) => return,
        Err(e) => return tracing::warn!(task = %key, "branching: {e}"),
    };
    if !pending(app, &task).await {
        return;
    }
    if task.title_source == TitleSource::Initial {
        retitle(app, key).await;
    }
    // The branch step comes here.
}

/// A free task did something (a tool ran, a turn ended): its analysis may be there now.
pub async fn on_activity(app: &AppState, key: &TaskKey) {
    if !is_free(key) {
        return;
    }
    let _guard = app.branching.lock(key).await;
    step(app, key).await;
}

/// Catches up on an analysis written while the daemon was down (or missed): at start
/// and on every tick.
pub async fn recover(app: &AppState, task: &Task) {
    if !is_free(&task.key) {
        return;
    }
    let _guard = app.branching.lock(&task.key).await;
    step(app, &task.key).await;
}

#[cfg(test)]
mod tests {
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
            std::fs::write(
                dir.join("spec.md"),
                format!("# Feature Specification: {name} feature\n"),
            )
            .unwrap();
        }
    }

    /// A free task with a branchless workspace of `kind` in a fresh folder.
    async fn task_with(app: &AppState, kind: ProjectKind, subs: &[&str]) -> (TaskKey, PathBuf) {
        task_with_key_kind(app, "LIBRE-1", kind, subs).await
    }

    async fn task_with_key(app: &AppState, key: &str) -> (TaskKey, PathBuf) {
        task_with_key_kind(app, key, ProjectKind::Repo, &[]).await
    }

    async fn task_with_key_kind(
        app: &AppState,
        key: &str,
        kind: ProjectKind,
        subs: &[&str],
    ) -> (TaskKey, PathBuf) {
        let root = std::env::temp_dir().join(format!("delilah-ws-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let key = TaskKey::new(key);
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

    fn write_spec(path: &Path, text: &str) -> PathBuf {
        std::fs::write(path, text).unwrap();
        path.to_path_buf()
    }

    #[test]
    fn spec_title_reads_the_feature_name() {
        let dir = std::env::temp_dir().join(format!("delilah-title-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let f = dir.join("spec.md");
        write_spec(
            &f,
            "# Feature Specification:   Reintentos de tarjeta \n\nbody",
        );
        assert_eq!(spec_title(&f).as_deref(), Some("Reintentos de tarjeta"));
        for text in [
            "no heading",
            "# Feature Specification:   \n",
            "# Feature Specification: [FEATURE NAME]\n",
            "# Feature Specification: [anything]\n",
        ] {
            write_spec(&f, text);
            assert_eq!(spec_title(&f), None, "{text}");
        }
        assert_eq!(spec_title(&dir.join("missing.md")), None);
    }

    async fn task_of(app: &AppState, key: &TaskKey) -> Task {
        app.store.get_task(key).await.unwrap().unwrap()
    }

    #[tokio::test]
    async fn retitle_names_an_initial_task_once() {
        let app = app().await;
        let (key, root) = task_with(&app, ProjectKind::Repo, &[]).await;
        record(&app, &key).await;
        let mut rx = app.events.subscribe();
        spec(&root, "001-new", true);
        retitle(&app, &key).await;
        let t = task_of(&app, &key).await;
        assert_eq!(t.title, "001-new feature");
        assert_eq!(t.title_source, TitleSource::Spec);
        assert_eq!(t.previous_title.as_deref(), Some("Idea"));
        assert!(rx.try_recv().is_ok(), "tasks event");
        // Edited later: no second rename.
        write_spec(
            &root.join("specs/001-new/spec.md"),
            "# Feature Specification: Other\n",
        );
        retitle(&app, &key).await;
        assert_eq!(task_of(&app, &key).await.title, "001-new feature");
    }

    #[tokio::test]
    async fn retitle_leaves_alone_what_is_not_theirs() {
        let app = app().await;
        let (key, root) = task_with(&app, ProjectKind::Repo, &[]).await;
        record(&app, &key).await;
        spec(&root, "001-new", true);
        for source in [TitleSource::Manual, TitleSource::Spec] {
            app.store
                .set_title_source(&key, source, None)
                .await
                .unwrap();
            retitle(&app, &key).await;
            assert_eq!(task_of(&app, &key).await.title, "Idea");
        }
        // A placeholder heading.
        app.store
            .set_title_source(&key, TitleSource::Initial, None)
            .await
            .unwrap();
        write_spec(
            &root.join("specs/001-new/spec.md"),
            "# Feature Specification: [FEATURE NAME]\n",
        );
        retitle(&app, &key).await;
        assert_eq!(task_of(&app, &key).await.title, "Idea");
    }

    #[tokio::test]
    async fn an_old_folder_does_not_retitle() {
        let app = app().await;
        let (key, root) = task_with(&app, ProjectKind::Repo, &[]).await;
        spec(&root, "001-old", true);
        record(&app, &key).await;
        retitle(&app, &key).await;
        assert_eq!(task_of(&app, &key).await.title, "Idea");
    }

    #[tokio::test]
    async fn activity_and_recovery_retitle() {
        let app = app().await;
        let (key, root) = task_with(&app, ProjectKind::Repo, &[]).await;
        record(&app, &key).await;
        spec(&root, "001-new", true);
        on_activity(&app, &key).await;
        assert_eq!(task_of(&app, &key).await.title_source, TitleSource::Spec);

        // Recovery acts on the task as it is now, not on the snapshot given.
        let stale = task_of(&app, &key).await;
        app.store
            .set_title_source(&key, TitleSource::Initial, None)
            .await
            .unwrap();
        app.store.set_title(&key, "Mine").await.unwrap();
        recover(&app, &stale).await;
        assert_eq!(task_of(&app, &key).await.title, "Mine");

        let (key2, root2) = task_with_key(&app, "LIBRE-2").await;
        record(&app, &key2).await;
        spec(&root2, "002-x", true);
        let snapshot = task_of(&app, &key2).await;
        recover(&app, &snapshot).await;
        assert_eq!(task_of(&app, &key2).await.title_source, TitleSource::Spec);
    }
}
