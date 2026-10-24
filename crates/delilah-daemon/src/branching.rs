//! Free tasks start without a branch or a real name: the first analysis creates a spec
//! folder, and the task takes its name (and branch) from it. This module finds that
//! folder and runs the triggers; two triggers never run together for one task.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use delilah_core::{ProjectKind, Task, TaskKey, TitleSource};
use delilah_git::{GitOptions, Workspace};

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

/// The first spec folder (with a `spec.md`) of `ws` that is not in `at_start`: its
/// repo's label and its path.
fn find_new_spec(ws: &Workspace, at_start: &[String]) -> Option<(String, PathBuf)> {
    for (label, dir) in spec_repos(ws) {
        for name in delilah_git::spec_dirs(&dir) {
            if !at_start.contains(&format!("{label}:{name}")) {
                return Some((label, dir.join("specs").join(name)));
            }
        }
    }
    None
}

/// The first spec folder (with a `spec.md`) that was not there when the task started:
/// its repo's label and its path. `None` without a workspace, a record of the start,
/// or a new folder.
pub(crate) async fn new_spec_folder(app: &AppState, key: &TaskKey) -> Option<(String, PathBuf)> {
    let (_, ws) = app.store.workspace(key).await.ok()??;
    let at_start = app.store.spec_dirs_at_start(key).await.ok()??;
    find_new_spec(&ws, &at_start)
}

/// A branch name from a title: lower-case ASCII (accents folded), runs of other
/// characters as `-`, at most 4 words and 40 characters. `task` when nothing is left.
pub(crate) fn slug(title: &str) -> String {
    let folded: String = title
        .chars()
        .flat_map(char::to_lowercase)
        .map(|c| match c {
            'á' | 'à' | 'â' | 'ä' | 'ã' | 'å' => 'a',
            'é' | 'è' | 'ê' | 'ë' => 'e',
            'í' | 'ì' | 'î' | 'ï' => 'i',
            'ó' | 'ò' | 'ô' | 'ö' | 'õ' => 'o',
            'ú' | 'ù' | 'û' | 'ü' => 'u',
            'ñ' => 'n',
            'ç' => 'c',
            c => c,
        })
        .collect();
    let words: Vec<&str> = folded
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|w| !w.is_empty())
        .take(4)
        .collect();
    let mut slug = words.join("-");
    slug.truncate(40);
    let slug = slug.trim_matches('-');
    if slug.is_empty() {
        "task".into()
    } else {
        slug.into()
    }
}

/// Whether the task still has something to take from its first analysis: a free task
/// that has no branch yet, still carries its initial title, or has a branch that a
/// submodule is not on yet (and no failure to wait for the user's retry on).
async fn pending(app: &AppState, task: &Task) -> bool {
    if !is_free(&task.key) {
        return false;
    }
    if task.title_source == TitleSource::Initial {
        return true;
    }
    let Ok(Some((_, ws))) = app.store.workspace(&task.key).await else {
        return false;
    };
    if !ws.has_branch() {
        return true;
    }
    if task.branch_error.is_some() || ws.kind != ProjectKind::Parent {
        return false;
    }
    // Branch recorded but maybe not applied everywhere (the daemon stopped in between).
    if !matches!(app.store.spec_dirs_at_start(&task.key).await, Ok(Some(_))) {
        return false;
    }
    let opts = git_options(app);
    tokio::task::spawn_blocking(move || !all_on_branch(&ws, &opts))
        .await
        .unwrap_or(false)
}

fn git_options(app: &AppState) -> GitOptions {
    GitOptions {
        allow_file_protocol: app.config.allow_file_protocol,
    }
}

/// Whether every repo of the task (the main one and each chosen submodule) is on its
/// branch.
fn all_on_branch(ws: &Workspace, opts: &GitOptions) -> bool {
    spec_repos(ws).into_iter().all(|(_, dir)| {
        delilah_git::current_branch(&dir, opts)
            .ok()
            .flatten()
            .as_deref()
            == Some(ws.branch.as_str())
    })
}

/// Who asks for the branch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)] // Guard and Retry are used by the guard hook and the retry route
pub(crate) enum Why {
    /// A tool ran or a turn ended: the analysis may be there now.
    Activity,
    /// The guard is about to let an edit or a commit through: the branch is needed now,
    /// from the title if nothing else names it.
    Guard,
    /// The user asked to try again.
    Retry,
}

/// What the blocking part found and did.
struct Settled {
    /// The task's branch, once the main repo is on it.
    branch: Option<String>,
    /// Why a repo could not be put on it, with the repo's name.
    error: Option<String>,
}

/// The branch the hook made: the main repo is on one it was not on at start.
fn adopted(
    ws: &Workspace,
    previous: Option<&[(String, String)]>,
    opts: &GitOptions,
) -> Result<Option<String>, String> {
    let Some(current) = delilah_git::current_branch(&ws.path, opts).map_err(|e| e.to_string())?
    else {
        return Ok(None);
    };
    match previous {
        // A worktree starts detached: any branch is new.
        None => Ok(Some(current)),
        Some(prev) => Ok(prev
            .iter()
            .find(|(label, _)| label == ".")
            .filter(|(_, was)| *was != current)
            .map(|_| current)),
    }
}

/// A name for the branch based on `wanted` that no branch of the main repo has: local
/// or remote-tracking. `-2`, `-3`… otherwise.
fn free_name(ws: &Workspace, wanted: &str, opts: &GitOptions) -> Result<String, String> {
    let mut name = wanted.to_string();
    let mut n = 1;
    while delilah_git::branch_exists(&ws.path, &name, opts).map_err(|e| e.to_string())? {
        n += 1;
        name = format!("{wanted}-{n}");
    }
    Ok(name)
}

/// R2 on the repos: settle the branch name (adopt, or create from the spec folder or the
/// title), then bring every repo onto it, the main repo first.
fn settle(
    ws: &Workspace,
    previous: Option<&[(String, String)]>,
    at_start: &[String],
    title: &str,
    why: Why,
    opts: &GitOptions,
) -> Settled {
    let fail = |error: String| Settled {
        branch: None,
        error: Some(error),
    };
    let branch = if ws.has_branch() {
        ws.branch.clone()
    } else {
        match adopted(ws, previous, opts) {
            Err(e) => return fail(format!("the main repo: {e}")),
            Ok(Some(branch)) => branch,
            Ok(None) => {
                let wanted = match find_new_spec(ws, at_start) {
                    Some((_, folder)) => folder
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_else(|| slug(title)),
                    None if why == Why::Guard => slug(title),
                    // Nothing to name it after yet.
                    None => {
                        return Settled {
                            branch: None,
                            error: None,
                        };
                    }
                };
                let name = match free_name(ws, &wanted, opts) {
                    Ok(name) => name,
                    Err(e) => return fail(format!("the main repo: {e}")),
                };
                if let Err(e) = delilah_git::apply_branch(&ws.path, &name, opts) {
                    return fail(format!("the main repo: {e}"));
                }
                name
            }
        }
    };
    if delilah_git::current_branch(&ws.path, opts)
        .ok()
        .flatten()
        .as_deref()
        != Some(branch.as_str())
        && let Err(e) = delilah_git::apply_branch(&ws.path, &branch, opts)
    {
        return fail(format!("the main repo: {e}"));
    }
    let mut failed = Vec::new();
    for (label, dir) in spec_repos(ws).into_iter().skip(1) {
        let on = delilah_git::current_branch(&dir, opts).ok().flatten();
        if on.as_deref() == Some(branch.as_str()) {
            continue;
        }
        if let Err(e) = delilah_git::apply_branch(&dir, &branch, opts) {
            failed.push(format!("{label}: {e}"));
        }
    }
    Settled {
        branch: Some(branch),
        error: (!failed.is_empty()).then(|| failed.join("; ")),
    }
}

/// Settles the branch of a free task, holding its lock: the branch Spec Kit made is
/// adopted, or Delilah makes one (from the spec folder, or from the title for the guard),
/// then every repo of the task is put on it. `Ok(None)`: nothing to do yet or ever (not a
/// free task, started with a branch, no analysis); `Ok(Some(branch))`: the task is on its
/// branch; `Err(reason)`: it could not be applied, recorded as the task's `branch_error`.
/// Idempotent.
#[allow(dead_code)] // used by the guard hook and the retry route
pub(crate) async fn ensure(
    app: &AppState,
    key: &TaskKey,
    why: Why,
) -> Result<Option<String>, String> {
    let _guard = app.branching.lock(key).await;
    ensure_locked(app, key, why).await
}

/// [`ensure`] for a caller that already holds the task's lock.
pub(crate) async fn ensure_locked(
    app: &AppState,
    key: &TaskKey,
    why: Why,
) -> Result<Option<String>, String> {
    let task = match app.store.get_task(key).await {
        Ok(Some(task)) if is_free(key) => task,
        Ok(_) => return Ok(None),
        Err(e) => return Err(e.to_string()),
    };
    // A failure waits for the user's retry (or the guard), not for the next tool call.
    if why == Why::Activity && task.branch_error.is_some() {
        return Ok(None);
    }
    let (ws, at_start, previous) = match (
        app.store.workspace(key).await,
        app.store.spec_dirs_at_start(key).await,
        app.store.checkout_previous(key).await,
    ) {
        (Ok(Some((_, ws))), Ok(Some(at_start)), Ok(previous)) => (ws, at_start, previous),
        (Ok(_), Ok(_), Ok(_)) => return Ok(None),
        (Err(e), ..) => return Err(e.to_string()),
        (_, Err(e), _) => return Err(e.to_string()),
        (_, _, Err(e)) => return Err(e.to_string()),
    };
    let opts = git_options(app);
    let title = task.title.clone();
    let recorded = ws.clone();
    let settled = tokio::task::spawn_blocking(move || {
        settle(&ws, previous.as_deref(), &at_start, &title, why, &opts)
    })
    .await
    .map_err(|e| e.to_string())?;

    let mut changed = false;
    if let Some(branch) = &settled.branch
        && !recorded.has_branch()
    {
        match app.store.set_workspace_branch(key, branch).await {
            Ok(()) => changed = true,
            Err(e) => {
                tracing::warn!(task = %key, "branching: {e}");
                return Err(e.to_string());
            }
        }
    }
    match settled.error {
        Some(error) => {
            tracing::warn!(task = %key, "branching: {error}");
            if task.branch_error.as_deref() != Some(error.as_str()) {
                if let Err(e) = app.store.set_branch_error(key, Some(&error)).await {
                    tracing::warn!(task = %key, "branching: {e}");
                }
                changed = true;
            }
            if changed {
                app.emit(crate::events::Topic::Tasks);
                app.emit(crate::events::Topic::Inbox);
            }
            Err(error)
        }
        None => {
            if task.branch_error.is_some() {
                if let Err(e) = app.store.set_branch_error(key, None).await {
                    tracing::warn!(task = %key, "branching: {e}");
                }
                changed = true;
                app.emit(crate::events::Topic::Inbox);
            }
            if changed {
                app.emit(crate::events::Topic::Tasks);
            }
            Ok(settled.branch)
        }
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
    // The outcome is recorded on the task; nothing else to do with it here.
    let _ = ensure_locked(app, key, Why::Activity).await;
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

    #[test]
    fn slug_folds_and_trims() {
        assert_eq!(slug("Reintentos de tarjeta"), "reintentos-de-tarjeta");
        assert_eq!(slug("  ¡Añadir ración, ÉXITO!  "), "anadir-racion-exito");
        assert_eq!(
            slug("uno dos tres cuatro cinco seis"),
            "uno-dos-tres-cuatro"
        );
        assert_eq!(slug("--a__b--"), "a-b");
        assert_eq!(slug("日本語 ¿?"), "task");
        assert_eq!(slug(""), "task");
        let long = slug("abcdefghijklmnopqrstuvwxyz abcdefghijklmnopqrstuvwxyz");
        assert!(long.len() <= 40 && !long.ends_with('-'), "{long}");
        assert_eq!(long, "abcdefghijklmnopqrstuvwxyz-abcdefghijklm");
        assert_eq!(slug("x".repeat(60).as_str()).len(), 40);
        // A cut that lands on a separator leaves no trailing dash.
        assert_eq!(slug(&format!("{} b", "a".repeat(39))), "a".repeat(39));
    }

    fn git(dir: &Path, args: &[&str]) -> String {
        let out = std::process::Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(["-c", "user.name=t", "-c", "user.email=t@t"])
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

    fn init_repo(dir: &Path) {
        std::fs::create_dir_all(dir).unwrap();
        git(dir, &["init", "-q", "-b", "main"]);
        git(dir, &["commit", "-q", "--allow-empty", "-m", "base"]);
    }

    fn on(dir: &Path) -> String {
        git(dir, &["branch", "--show-current"])
    }

    /// A free task whose workspace is a real, branchless repo (detached, like a new
    /// worktree), with the start recorded. `subs` are nested repos of a parent.
    async fn real_task(app: &AppState, key: &str, subs: &[&str]) -> (TaskKey, PathBuf) {
        let kind = if subs.is_empty() {
            ProjectKind::Repo
        } else {
            ProjectKind::Parent
        };
        let (key, root) = task_with_key_kind(app, key, kind, subs).await;
        init_repo(&root);
        git(&root, &["checkout", "-q", "--detach"]);
        for sub in subs {
            init_repo(&root.join(sub));
            git(&root.join(sub), &["checkout", "-q", "--detach"]);
        }
        record(app, &key).await;
        (key, root)
    }

    async fn ws_branch(app: &AppState, key: &TaskKey) -> String {
        app.store.workspace(key).await.unwrap().unwrap().1.branch
    }

    #[tokio::test]
    async fn ensure_adopts_the_branch_the_hook_made() {
        let app = app().await;
        let (key, root) = real_task(&app, "LIBRE-1", &[]).await;
        assert_eq!(ensure(&app, &key, Why::Activity).await, Ok(None));
        git(&root, &["checkout", "-q", "-b", "006-x"]);
        assert_eq!(
            ensure(&app, &key, Why::Activity).await,
            Ok(Some("006-x".into()))
        );
        assert_eq!(ws_branch(&app, &key).await, "006-x");
    }

    #[tokio::test]
    async fn ensure_adopts_in_the_checkout_only_when_it_moved() {
        let app = app().await;
        let (key, root) = real_task(&app, "LIBRE-1", &[]).await;
        git(&root, &["checkout", "-q", "main"]);
        app.store
            .set_checkout_previous(&key, &[(".".into(), "main".into())])
            .await
            .unwrap();
        // Still on `main` where it started: not the task's.
        assert_eq!(ensure(&app, &key, Why::Activity).await, Ok(None));
        assert_eq!(ws_branch(&app, &key).await, "");
        git(&root, &["checkout", "-q", "-b", "006-x"]);
        assert_eq!(
            ensure(&app, &key, Why::Activity).await,
            Ok(Some("006-x".into()))
        );
    }

    #[tokio::test]
    async fn ensure_creates_the_branch_from_the_spec_folder_keeping_files() {
        let app = app().await;
        let (key, root) = real_task(&app, "LIBRE-1", &[]).await;
        spec(&root, "005-y", true);
        assert_eq!(
            ensure(&app, &key, Why::Activity).await,
            Ok(Some("005-y".into()))
        );
        assert_eq!(on(&root), "005-y");
        assert!(root.join("specs/005-y/spec.md").is_file());
        assert_eq!(ws_branch(&app, &key).await, "005-y");
    }

    #[tokio::test]
    async fn ensure_numbers_a_taken_name() {
        let app = app().await;
        let (key, root) = real_task(&app, "LIBRE-1", &[]).await;
        git(&root, &["branch", "005-y"]);
        git(
            &root,
            &["update-ref", "refs/remotes/origin/005-y-2", "HEAD"],
        );
        spec(&root, "005-y", true);
        assert_eq!(
            ensure(&app, &key, Why::Activity).await,
            Ok(Some("005-y-3".into()))
        );
        assert_eq!(on(&root), "005-y-3");
    }

    #[tokio::test]
    async fn only_the_guard_names_a_branch_after_the_title() {
        let app = app().await;
        let (key, root) = real_task(&app, "LIBRE-1", &[]).await;
        app.store
            .set_title(&key, "Reintentos de tarjeta")
            .await
            .unwrap();
        assert_eq!(ensure(&app, &key, Why::Activity).await, Ok(None));
        assert_eq!(on(&root), "");
        assert_eq!(
            ensure(&app, &key, Why::Guard).await,
            Ok(Some("reintentos-de-tarjeta".into()))
        );
        assert_eq!(on(&root), "reintentos-de-tarjeta");
    }

    #[tokio::test]
    async fn ensure_puts_a_parent_and_its_submodules_on_the_branch() {
        let app = app().await;
        let (key, root) = real_task(&app, "LIBRE-1", &["api", "web"]).await;
        spec(&root, "005-y", true);
        // `web` cannot take the name: it has it already.
        git(&root.join("web"), &["branch", "005-y"]);
        let err = ensure(&app, &key, Why::Activity).await.unwrap_err();
        assert!(err.contains("web") && !err.contains("api"), "{err}");
        assert_eq!(on(&root), "005-y");
        assert_eq!(on(&root.join("api")), "005-y");
        assert_eq!(on(&root.join("web")), "");
        assert_eq!(ws_branch(&app, &key).await, "005-y");
        assert_eq!(
            task_of(&app, &key).await.branch_error.as_deref(),
            Some(err.as_str())
        );

        // The tick and the tool calls leave a failure alone; the retry finishes it.
        assert_eq!(ensure(&app, &key, Why::Activity).await, Ok(None));
        git(&root.join("web"), &["branch", "-D", "005-y"]);
        assert_eq!(
            ensure(&app, &key, Why::Retry).await,
            Ok(Some("005-y".into()))
        );
        assert_eq!(on(&root.join("web")), "005-y");
        assert_eq!(task_of(&app, &key).await.branch_error, None);
    }

    #[tokio::test]
    async fn a_recorded_branch_with_a_submodule_left_is_pending_and_finished() {
        let app = app().await;
        let (key, root) = real_task(&app, "LIBRE-1", &["api"]).await;
        app.store
            .set_title_source(&key, TitleSource::Spec, None)
            .await
            .unwrap();
        git(&root, &["checkout", "-q", "-b", "005-y"]);
        app.store.set_workspace_branch(&key, "005-y").await.unwrap();
        assert!(pending(&app, &task_of(&app, &key).await).await);
        on_activity(&app, &key).await;
        assert_eq!(on(&root.join("api")), "005-y");
        assert!(!pending(&app, &task_of(&app, &key).await).await);
    }

    #[tokio::test]
    async fn ensure_leaves_alone_what_is_not_its_to_settle() {
        let app = app().await;
        // Jira task.
        let (jira, root) = real_task(&app, "PROJ-1", &[]).await;
        spec(&root, "005-y", true);
        assert_eq!(ensure(&app, &jira, Why::Guard).await, Ok(None));
        assert_eq!(on(&root), "");
        // Started before the feature (no record of the start).
        let (old, root) = real_task(&app, "LIBRE-2", &[]).await;
        app.store.set_spec_dirs_at_start(&old, None).await.unwrap();
        spec(&root, "005-y", true);
        assert_eq!(ensure(&app, &old, Why::Guard).await, Ok(None));
        assert_eq!(on(&root), "");
        // With a branch from the start.
        app.store
            .set_workspace_branch(&old, "feature/x")
            .await
            .unwrap();
        assert_eq!(ensure(&app, &old, Why::Retry).await, Ok(None));
        assert_eq!(ws_branch(&app, &old).await, "feature/x");
    }

    #[tokio::test]
    async fn ensure_twice_gives_the_same_and_success_clears_the_error() {
        let app = app().await;
        let (key, root) = real_task(&app, "LIBRE-1", &[]).await;
        spec(&root, "005-y", true);
        app.store.set_branch_error(&key, Some("old")).await.unwrap();
        let mut rx = app.events.subscribe();
        let first = ensure(&app, &key, Why::Retry).await;
        assert_eq!(first, Ok(Some("005-y".into())));
        assert_eq!(task_of(&app, &key).await.branch_error, None);
        let mut topics = Vec::new();
        while let Ok(t) = rx.try_recv() {
            topics.push(t);
        }
        assert!(topics.contains(&crate::events::Topic::Tasks), "{topics:?}");
        assert_eq!(ensure(&app, &key, Why::Retry).await, first);
        assert_eq!(ensure(&app, &key, Why::Activity).await, first);
        assert_eq!(on(&root), "005-y");
        assert_eq!(rx.try_recv().ok(), None, "the second time changes nothing");
    }
}
