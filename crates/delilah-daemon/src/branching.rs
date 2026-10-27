//! Free tasks start without a branch or a real name: the first analysis creates a spec
//! folder, and the task takes its name (and branch) from it. This module finds that
//! folder and runs the triggers; two triggers never run together for one task.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use axum::Json;
use axum::extract::{Path as UrlPath, State};
use delilah_core::{Project, ProjectKind, Task, TaskKey, TitleSource};
use delilah_git::{GitOptions, Workspace};
use serde_json::{Value, json};

use crate::api::ApiError;
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

/// The placeholders of a task's commands: `{task}` is the title for a free task and the
/// key for a Jira one; the title is trimmed.
pub(crate) fn task_vars<'a>(
    key: &'a TaskKey,
    title: &'a str,
    spec_dir: Option<&'a str>,
) -> delilah_core::workflow::Vars<'a> {
    let title = title.trim();
    delilah_core::workflow::Vars {
        ticket: key.as_str(),
        title,
        task: if is_free(key) { title } else { key.as_str() },
        spec_dir,
    }
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

/// Records the spec folders now in the workspace as the ones from before the task's
/// first analysis, adding to any record already there (the setup command may have
/// created some).
pub(crate) async fn record_baseline(
    app: &AppState,
    key: &TaskKey,
    ws: &Workspace,
) -> Result<(), String> {
    let scanned = ws.clone();
    let mut dirs = tokio::task::spawn_blocking(move || scan_spec_dirs(&scanned))
        .await
        .map_err(|e| e.to_string())?;
    if let Some(before) = app
        .store
        .spec_dirs_at_start(key)
        .await
        .map_err(|e| e.to_string())?
    {
        for dir in before {
            if !dirs.contains(&dir) {
                dirs.push(dir);
            }
        }
    }
    app.store
        .set_spec_dirs_at_start(key, Some(&dirs))
        .await
        .map_err(|e| e.to_string())
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

/// A branch name from a title: the shared slug (lower-case ASCII, accents folded, at
/// most 40 characters) cut to its first 4 words. `task` when nothing is left.
pub(crate) fn slug(title: &str) -> String {
    let slug = delilah_core::slug(title)
        .split('-')
        .take(4)
        .collect::<Vec<_>>()
        .join("-");
    if slug.is_empty() { "task".into() } else { slug }
}

/// Whether a starting task begins without a branch (decision 65): a free task whose
/// request typed no branch, in a project with no branch template. Everything else is
/// named at start (typed branch, then template, then the proposal). A parent task and a
/// subtask always are: their branch is the integration point from the first moment.
pub(crate) fn starts_branchless(
    project: &Project,
    key: &TaskKey,
    typed: bool,
    kind: delilah_core::hierarchy::TaskKind,
) -> bool {
    kind == delilah_core::hierarchy::TaskKind::Normal
        && is_free(key)
        && !typed
        && !crate::branch_name::has_template(project)
}

/// Whether the task still has something to take from its first analysis: a free task
/// that has no branch yet, still carries its initial title, or has a branch that a
/// submodule is not on yet (and no failure to wait for the user's retry on).
async fn pending(app: &AppState, task: &Task) -> bool {
    if !is_free(&task.key) || task.kind != delilah_core::hierarchy::TaskKind::Normal {
        return false;
    }
    if task.title_source == TitleSource::Initial {
        return true;
    }
    // A task that started on a branch keeps it (decision 65): only the title is pending.
    if !matches!(app.store.started_branchless(&task.key).await, Ok(true)) {
        return false;
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

/// A name for the branch based on `wanted` that no branch of any repo of the task (the
/// main one and each chosen submodule) has: local or remote-tracking. `-2`, `-3`…
/// otherwise.
fn free_name(ws: &Workspace, wanted: &str, opts: &GitOptions) -> Result<String, String> {
    let dirs: Vec<PathBuf> = spec_repos(ws).into_iter().map(|(_, dir)| dir).collect();
    delilah_git::free_branch(&dirs, wanted, opts).map_err(|e| e.to_string())
}

/// Puts `dir` on `branch`: switches to it when the repo has it already (left by the user
/// or by a partial run), starts it at the current commit otherwise.
fn bring_onto(dir: &Path, branch: &str, opts: &GitOptions) -> Result<(), delilah_git::GitError> {
    if delilah_git::branch_taken(dir, branch, opts)? {
        delilah_git::switch_branch(dir, branch, opts)
    } else {
        delilah_git::apply_branch(dir, branch, opts)
    }
}

/// R2 on the repos: settle the branch name (adopt, or create from the spec folder or the
/// title), then bring every repo onto it, the main repo first.
fn settle(
    ws: &Workspace,
    previous: Option<&[(String, String)]>,
    at_start: &[String],
    title: &str,
    name_from_title: bool,
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
                    None if name_from_title => slug(title),
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
                if let Err(e) = bring_onto(&ws.path, &name, opts) {
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
        && let Err(e) = bring_onto(&ws.path, &branch, opts)
    {
        return fail(format!("the main repo: {e}"));
    }
    let mut failed = Vec::new();
    for (label, dir) in spec_repos(ws).into_iter().skip(1) {
        let on = delilah_git::current_branch(&dir, opts).ok().flatten();
        if on.as_deref() == Some(branch.as_str()) {
            continue;
        }
        if let Err(e) = bring_onto(&dir, &branch, opts) {
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
    // A task that started on a branch (typed, template) is never moved off it.
    match app.store.started_branchless(key).await {
        Ok(true) => {}
        Ok(false) => return Ok(None),
        Err(e) => return Err(e.to_string()),
    }
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
    // The guard needs a branch now; a retry of a failure is the same ask, so both fall
    // back to the title when no spec folder names it.
    let name_from_title = why == Why::Guard || (why == Why::Retry && task.branch_error.is_some());
    let settled = tokio::task::spawn_blocking(move || {
        settle(
            &ws,
            previous.as_deref(),
            &at_start,
            &title,
            name_from_title,
            &opts,
        )
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
            // Nothing made (no analysis yet): a failure on record stays until it is fixed.
            if task.branch_error.is_some() && settled.branch.is_some() {
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

/// How long the guard may take to put a task on its branch: inside the hook's `curl -m 15`.
const GUARD_BUDGET: std::time::Duration = std::time::Duration::from_secs(12);

/// Commands that publish or move work: they need the branch as much as an edit does.
const GUARDED_GIT: [&str; 4] = ["commit", "push", "merge", "rebase"];

/// Whether a shell command commits, pushes, merges, rebases or opens a PR, also behind
/// git's global options (`git -C dir commit`, `git -c k=v commit`), extra spaces, quotes
/// or a chain (`a && git commit`). It errs on the side of yes: the guard only makes the
/// branch earlier.
fn lands_work(command: &str) -> bool {
    command
        .split([';', '&', '|', '\n', '\r', '(', ')', '`'])
        .any(|simple| {
            let words: Vec<&str> = simple
                .split_whitespace()
                .map(|w| w.trim_matches(['\'', '"']))
                .collect();
            words.iter().enumerate().any(|(i, w)| {
                let rest = &words[i + 1..];
                match w.rsplit('/').next().unwrap_or(w) {
                    "git" => rest.iter().any(|r| GUARDED_GIT.contains(r)),
                    "gh" => rest.windows(2).any(|p| p == ["pr", "create"]),
                    _ => false,
                }
            })
        })
}

/// `a/./b/../c` as `a/c`, without touching the disk.
fn normalize(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for part in path.components() {
        match part {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                out.pop();
            }
            other => out.push(other),
        }
    }
    out
}

/// Whether a file is part of the repo's work and not of its specs: under one of the
/// task's repos (a submodule, or else the workspace) and outside that repo's `specs/` and
/// `.specify/`. A file elsewhere on the disk (the task's notes, say) is not.
fn is_repo_work(ws: &Workspace, file: &str) -> bool {
    let root = normalize(&ws.path);
    let file = normalize(&ws.path.join(file));
    let Ok(in_ws) = file.strip_prefix(&root) else {
        return false;
    };
    // The deepest repo that holds it decides what "under specs/" means.
    let mut inner = in_ws;
    for sub in &ws.submodules {
        let sub = normalize(Path::new(sub));
        if let Ok(rest) = in_ws.strip_prefix(&sub)
            && rest.components().count() < inner.components().count()
        {
            inner = rest;
        }
    }
    let first = inner.components().next();
    !matches!(
        first,
        Some(std::path::Component::Normal(n)) if n == "specs" || n == ".specify"
    )
}

/// Whether the tool call in a PreToolUse hook lands work: an edit of a file outside the
/// spec folders, or a command that commits, pushes, merges, rebases or opens a PR.
fn needs_branch(ws: &Workspace, hook: &Value) -> bool {
    let input = &hook["tool_input"];
    match hook["tool_name"].as_str() {
        Some("Edit" | "Write" | "MultiEdit") => input["file_path"]
            .as_str()
            .is_none_or(|f| is_repo_work(ws, f)),
        Some("NotebookEdit") => input["notebook_path"]
            .as_str()
            .is_none_or(|f| is_repo_work(ws, f)),
        Some("Bash") => input["command"].as_str().is_some_and(lands_work),
        _ => false,
    }
}

/// The guard's verdict on one tool call (R3): `None` lets it through; `Some(reason)`
/// stops it. A task without a branch gets one first, from the title if nothing else names
/// it, so no edit or commit lands on the base branch.
pub(crate) async fn guard(app: &AppState, key: &TaskKey, hook: &Value) -> Option<String> {
    if !is_free(key) {
        return None;
    }
    if !matches!(app.store.started_branchless(key).await, Ok(true)) {
        return None;
    }
    let Ok(Some((_, ws))) = app.store.workspace(key).await else {
        return None;
    };
    if !needs_branch(&ws, hook) {
        return None;
    }
    // On its branch (and nothing waiting to be finished): nothing to do.
    if ws.has_branch() {
        match app.store.get_task(key).await {
            Ok(Some(task)) if task.branch_error.is_none() => return None,
            Ok(Some(_)) => {}
            _ => return None,
        }
    }
    guard_within(app, key, GUARD_BUDGET).await
}

/// [`guard`]'s second half with the budget given. The work runs detached, under the
/// task's lock: when the budget runs out the guard only stops waiting, the work still
/// finishes and records its outcome, and nothing else touches the repos meanwhile.
async fn guard_within(
    app: &AppState,
    key: &TaskKey,
    budget: std::time::Duration,
) -> Option<String> {
    let (work_app, work_key) = (app.clone(), key.clone());
    let work = tokio::spawn(async move { ensure(&work_app, &work_key, Why::Guard).await });
    let reason = match tokio::time::timeout(budget, work).await {
        Ok(Ok(Ok(_))) => return None,
        Ok(Ok(Err(reason))) => reason,
        Ok(Err(e)) => e.to_string(),
        Err(_) => {
            return Some(
                "Delilah is still putting this task on its own branch, so nothing was edited \
                 or committed. Retry in a moment."
                    .to_string(),
            );
        }
    };
    Some(format!(
        "Delilah could not put this task on its own branch ({reason}), so nothing was \
         edited or committed. Stop and tell the user: they can retry from Delilah."
    ))
}

/// The PreToolUse hook's reply: empty to allow, a denial with its reason otherwise.
pub(crate) fn guard_reply(denial: Option<String>) -> Value {
    match denial {
        None => json!({}),
        Some(reason) => json!({
            "hookSpecificOutput": {
                "hookEventName": "PreToolUse",
                "permissionDecision": "deny",
                "permissionDecisionReason": reason,
            }
        }),
    }
}

/// `POST /api/v1/tasks/{key}/branch`: try again to put a free task on its branch (the
/// Bandeja's retry). 409 when there is nothing to retry: not a free task, or every repo is
/// on its branch already. The outcome is the task's `branch_error`.
pub async fn retry(
    State(app): State<AppState>,
    UrlPath(key): UrlPath<String>,
) -> Result<Json<Task>, ApiError> {
    let key = TaskKey::new(key);
    let _guard = app.branching.lock(&key).await;
    let task = app
        .store
        .get_task(&key)
        .await?
        .ok_or_else(|| ApiError::not_found(format!("no task {key}")))?;
    if !is_free(&key) {
        return Err(ApiError::conflict("only free tasks have a branch to retry"));
    }
    if !app.store.started_branchless(&key).await? {
        return Err(ApiError::conflict(
            "the task started on its own branch: there is nothing to retry",
        ));
    }
    let settled = match app.store.workspace(&key).await? {
        Some((_, ws)) if ws.has_branch() && task.branch_error.is_none() => {
            let opts = git_options(&app);
            tokio::task::spawn_blocking(move || all_on_branch(&ws, &opts))
                .await
                .map_err(|e| ApiError::internal(e.to_string()))?
        }
        _ => false,
    };
    if settled {
        return Err(ApiError::conflict("the task is on its branch already"));
    }
    // A failure is recorded on the task; the answer is the task as it is now.
    let _ = ensure_locked(&app, &key, Why::Retry).await;
    let task = app
        .store
        .get_task(&key)
        .await?
        .ok_or_else(|| ApiError::not_found(format!("no task {key}")))?;
    Ok(Json(task))
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
            claude_bin: "claude".into(),
            agent_program: None,
            ..Config::test_defaults()
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
        let mut task = Task::new(key.clone(), "Idea", "project");
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
        app.store.set_started_branchless(key, true).await.unwrap();
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
    fn task_vars_picks_the_task_and_trims_the_title() {
        let free = TaskKey::new("LIBRE-3");
        let v = task_vars(&free, "  Fix it ", None);
        assert_eq!((v.ticket, v.title, v.task), ("LIBRE-3", "Fix it", "Fix it"));
        let jira = TaskKey::new("PROJ-1");
        let v = task_vars(&jira, "Fix it", Some("specs/x"));
        assert_eq!((v.task, v.spec_dir), ("PROJ-1", Some("specs/x")));
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
        // The shared slug cuts at the last whole word that fits in 40.
        assert_eq!(long, "abcdefghijklmnopqrstuvwxyz");
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
        // The shared free-name check reads remote-tracking refs through the configured remotes.
        git(
            &root,
            &["remote", "add", "origin", "https://example.invalid/r.git"],
        );
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
        // `web` cannot take the name now: git is busy there.
        let lock = root.join("web/.git/index.lock");
        std::fs::write(&lock, "").unwrap();
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
        std::fs::remove_file(&lock).unwrap();
        assert_eq!(
            ensure(&app, &key, Why::Retry).await,
            Ok(Some("005-y".into()))
        );
        assert_eq!(on(&root.join("web")), "005-y");
        assert_eq!(task_of(&app, &key).await.branch_error, None);
    }

    #[tokio::test]
    async fn a_name_taken_in_any_repo_of_the_task_is_numbered() {
        let app = app().await;
        let (key, root) = real_task(&app, "LIBRE-1", &["api", "web"]).await;
        spec(&root, "005-y", true);
        git(&root.join("web"), &["branch", "005-y"]);
        assert_eq!(
            ensure(&app, &key, Why::Activity).await,
            Ok(Some("005-y-2".into()))
        );
        for dir in [root.clone(), root.join("api"), root.join("web")] {
            assert_eq!(on(&dir), "005-y-2");
        }
        assert_eq!(task_of(&app, &key).await.branch_error, None);
    }

    #[tokio::test]
    async fn a_branch_the_hook_made_is_adopted_and_a_submodule_that_has_it_is_switched() {
        let app = app().await;
        let (key, root) = real_task(&app, "LIBRE-1", &["web"]).await;
        git(&root.join("web"), &["branch", "006-x"]);
        git(&root, &["checkout", "-q", "-b", "006-x"]);
        assert_eq!(
            ensure(&app, &key, Why::Activity).await,
            Ok(Some("006-x".into()))
        );
        assert_eq!(on(&root.join("web")), "006-x");
        assert_eq!(task_of(&app, &key).await.branch_error, None);
    }

    #[tokio::test]
    async fn the_main_repo_switches_to_a_recorded_branch_it_already_has() {
        let app = app().await;
        let (key, root) = real_task(&app, "LIBRE-1", &[]).await;
        git(&root, &["branch", "007-z"]);
        app.store.set_workspace_branch(&key, "007-z").await.unwrap();
        app.store
            .set_branch_error(&key, Some("the main repo: boom"))
            .await
            .unwrap();
        assert_eq!(
            ensure(&app, &key, Why::Retry).await,
            Ok(Some("007-z".into()))
        );
        assert_eq!(on(&root), "007-z");
        assert_eq!(task_of(&app, &key).await.branch_error, None);
    }

    #[tokio::test]
    async fn a_retry_without_a_spec_folder_names_the_branch_from_the_title() {
        let app = app().await;
        let (key, root) = real_task(&app, "LIBRE-1", &[]).await;
        app.store.set_title(&key, "Mi idea").await.unwrap();
        app.store
            .set_branch_error(&key, Some("the main repo: boom"))
            .await
            .unwrap();
        let Json(task) = retry(State(app.clone()), UrlPath("LIBRE-1".into()))
            .await
            .ok()
            .unwrap();
        assert_eq!(on(&root), "mi-idea");
        assert_eq!(ws_branch(&app, &key).await, "mi-idea");
        assert_eq!(task.branch_error, None);
    }

    #[tokio::test]
    async fn nothing_made_keeps_the_failure_on_record() {
        let app = app().await;
        let (key, root) = real_task(&app, "LIBRE-1", &[]).await;
        app.store
            .set_branch_error(&key, Some("the main repo: boom"))
            .await
            .unwrap();
        // The activity path has no spec folder and no title fallback: it does nothing.
        assert_eq!(ensure(&app, &key, Why::Activity).await, Ok(None));
        assert_eq!(on(&root), "");
        assert!(task_of(&app, &key).await.branch_error.is_some());
    }

    #[tokio::test]
    async fn a_guard_that_runs_out_of_time_leaves_the_work_to_finish_under_the_lock() {
        let app = app().await;
        let (key, root) = real_task(&app, "LIBRE-1", &[]).await;
        app.store.set_title(&key, "Mi idea").await.unwrap();
        // Something else holds the task: the guard's work has to wait for it.
        let held = app.branching.lock(&key).await;
        let reason = guard_within(&app, &key, std::time::Duration::from_millis(50))
            .await
            .expect("denied");
        assert!(reason.contains("still"), "{reason}");
        // The retry waits for the same lock, and nothing ran meanwhile.
        let retrying = tokio::spawn({
            let app = app.clone();
            async move { retry(State(app), UrlPath("LIBRE-1".into())).await.is_ok() }
        });
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        assert!(!retrying.is_finished());
        assert_eq!(on(&root), "");
        drop(held);
        // The work finishes and records its branch; the retry then finds nothing to do.
        let _ = retrying.await.unwrap();
        for _ in 0..100 {
            if ws_branch(&app, &key).await == "mi-idea" {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
        assert_eq!(ws_branch(&app, &key).await, "mi-idea");
        assert_eq!(on(&root), "mi-idea");
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
    async fn a_task_that_did_not_start_branchless_is_left_alone_by_every_step() {
        let app = app().await;
        let (key, root) = task_with(&app, ProjectKind::Repo, &[]).await;
        record(&app, &key).await;
        app.store.set_started_branchless(&key, false).await.unwrap();
        spec(&root, "002-new", true);
        for why in [Why::Activity, Why::Guard, Why::Retry] {
            assert_eq!(ensure(&app, &key, why).await, Ok(None));
        }
        let edit = json!({ "tool_name": "Edit", "tool_input": { "file_path": "src/a.rs" } });
        assert_eq!(guard(&app, &key, &edit).await, None);
        // Only the title is still pending: the branch is not.
        retitle(&app, &key).await;
        let task = app.store.get_task(&key).await.unwrap().unwrap();
        assert!(!pending(&app, &task).await);
        let (_, ws) = app.store.workspace(&key).await.unwrap().unwrap();
        assert!(!ws.has_branch());
        assert_eq!(task.branch_error, None);
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

    fn hook(tool: &str, input: Value) -> Value {
        json!({ "hook_event_name": "PreToolUse", "tool_name": tool, "tool_input": input })
    }

    #[tokio::test]
    async fn the_guard_lets_spec_edits_through_without_a_branch() {
        let app = app().await;
        let (key, root) = real_task(&app, "LIBRE-1", &["api"]).await;
        for (tool, input) in [
            (
                "Write",
                json!({ "file_path": root.join("specs/001-x/spec.md") }),
            ),
            ("Edit", json!({ "file_path": "specs/001-x/plan.md" })),
            (
                "MultiEdit",
                json!({ "file_path": root.join(".specify/memory/c.md") }),
            ),
            (
                "Write",
                json!({ "file_path": root.join("api/specs/001-x/spec.md") }),
            ),
            (
                "NotebookEdit",
                json!({ "notebook_path": "./specs/n.ipynb" }),
            ),
            // Outside the workspace: not repo work.
            ("Write", json!({ "file_path": "/tmp/notes.md" })),
            ("Bash", json!({ "command": "ls -la && git status" })),
            ("Read", json!({ "file_path": "src/main.rs" })),
        ] {
            assert_eq!(
                guard(&app, &key, &hook(tool, input.clone())).await,
                None,
                "{tool} {input}"
            );
        }
        assert_eq!(ws_branch(&app, &key).await, "");
        assert_eq!(on(&root), "");
    }

    #[tokio::test]
    async fn the_guard_makes_the_branch_before_an_edit_or_a_commit() {
        for (tool, input) in [
            ("Edit", json!({ "file_path": "src/main.rs" })),
            ("Write", json!({ "file_path": "specs/../src/a.rs" })),
            ("Write", json!({ "file_path": "api/src/a.rs" })),
            ("Write", json!({ "file_path": "myspecs/a.md" })),
            ("NotebookEdit", json!({ "notebook_path": "n.ipynb" })),
            ("Bash", json!({ "command": "git add . && git commit -m x" })),
            ("Bash", json!({ "command": "git push origin HEAD" })),
            ("Bash", json!({ "command": "gh pr create --fill" })),
            ("Bash", json!({ "command": "git merge main" })),
            ("Bash", json!({ "command": "git rebase main" })),
        ] {
            let app = app().await;
            let (key, root) = real_task(&app, "LIBRE-1", &["api"]).await;
            app.store.set_title(&key, "Mi idea").await.unwrap();
            assert_eq!(
                guard(&app, &key, &hook(tool, input.clone())).await,
                None,
                "{tool} {input}"
            );
            assert_eq!(on(&root), "mi-idea", "{tool} {input}");
            assert_eq!(on(&root.join("api")), "mi-idea");
            assert_eq!(ws_branch(&app, &key).await, "mi-idea");
        }
    }

    #[test]
    fn landing_commands_are_seen_behind_globals_spacing_and_chains() {
        for yes in [
            "git commit -m x",
            "git -C api commit -m x",
            "git -c user.name=x commit",
            "git  commit",
            "git\tcommit",
            "/usr/bin/git push",
            "cd api && git commit -am x",
            "echo ok; git rebase main",
            "gh -R o/r pr create --fill",
            "'git' 'push'",
        ] {
            assert!(lands_work(yes), "{yes:?}");
        }
        for no in ["git status", "git diff --stat", "gh pr view", "echo commit"] {
            assert!(!lands_work(no), "{no:?}");
        }
    }

    #[tokio::test]
    async fn the_guard_denies_when_the_branch_cannot_be_made() {
        let app = app().await;
        let (key, root) = real_task(&app, "LIBRE-1", &["web"]).await;
        spec(&root, "005-y", true);
        std::fs::write(root.join("web/.git/index.lock"), "").unwrap();
        let reason = guard(
            &app,
            &key,
            &hook("Bash", json!({ "command": "git commit -am x" })),
        )
        .await
        .expect("denied");
        assert!(reason.contains("web"), "{reason}");
        assert!(task_of(&app, &key).await.branch_error.is_some());
        let reply = guard_reply(Some(reason.clone()));
        assert_eq!(reply["hookSpecificOutput"]["permissionDecision"], "deny");
        assert_eq!(
            reply["hookSpecificOutput"]["permissionDecisionReason"],
            reason
        );
        assert_eq!(guard_reply(None), json!({}));
    }

    #[tokio::test]
    async fn the_guard_answers_at_once_for_a_task_with_its_branch() {
        let app = app().await;
        let (key, root) = real_task(&app, "LIBRE-1", &[]).await;
        app.store
            .set_workspace_branch(&key, "feature/x")
            .await
            .unwrap();
        let edit = hook("Edit", json!({ "file_path": "src/a.rs" }));
        assert_eq!(guard(&app, &key, &edit).await, None);
        // Nothing was touched: the repo is still detached.
        assert_eq!(on(&root), "");
        // Jira tasks and unknown ones are not the guard's.
        let (jira, _) = real_task(&app, "PROJ-1", &[]).await;
        assert_eq!(guard(&app, &jira, &edit).await, None);
        assert_eq!(guard(&app, &TaskKey::new("LIBRE-9"), &edit).await, None);
    }

    #[tokio::test]
    async fn retry_finishes_a_failed_branch_and_refuses_a_settled_one() {
        let app = app().await;
        let (key, root) = real_task(&app, "LIBRE-1", &["web"]).await;
        spec(&root, "005-y", true);
        let lock = root.join("web/.git/index.lock");
        std::fs::write(&lock, "").unwrap();
        assert!(ensure(&app, &key, Why::Activity).await.is_err());
        std::fs::remove_file(&lock).unwrap();
        let Json(task) = retry(State(app.clone()), UrlPath("LIBRE-1".into()))
            .await
            .ok()
            .unwrap();
        assert_eq!(task.branch_error, None);
        assert_eq!(on(&root.join("web")), "005-y");
        let err = retry(State(app.clone()), UrlPath("LIBRE-1".into()))
            .await
            .unwrap_err();
        assert_eq!(err.status, axum::http::StatusCode::CONFLICT);
    }
}

#[cfg(test)]
mod kind_tests {
    use super::*;
    use delilah_core::hierarchy::TaskKind;

    #[test]
    fn parent_tasks_and_subtasks_are_never_branchless() {
        let project = Project {
            name: "app".into(),
            path: "/tmp/app".into(),
            kind: delilah_core::ProjectKind::Repo,
            workflow: "project".into(),
            settings: Default::default(),
            preview_command: None,
        };
        let key = TaskKey::new("LIBRE-7");
        assert!(starts_branchless(&project, &key, false, TaskKind::Normal));
        assert!(!starts_branchless(
            &project,
            &key,
            false,
            TaskKind::ParentTask
        ));
        assert!(!starts_branchless(&project, &key, false, TaskKind::Subtask));
    }
}
