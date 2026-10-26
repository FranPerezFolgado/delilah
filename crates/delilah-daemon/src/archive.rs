//! Archiving a finished task, shared by the user's archive (`actions::archive`) and the
//! archive after a merge (`pr_loop::merge`): release the workspace, forget it, delete the
//! local branches that were verified merged, and resolve the notices left. What decides
//! whether a task may be archived is the caller's policy; this is only how it is done.

use std::collections::HashMap;

use delilah_core::TaskKey;
use delilah_git::{GitOptions, Workspace};

use crate::api::ApiError;
use crate::preview::{GRACE, end_group};
use crate::state::AppState;

/// Notice kind: archiving a merged task was refused, nothing was removed.
pub(crate) const KIND_BLOCKED: &str = "archive-blocked";
/// Notice kind: a merged task was archived but local branches are left.
pub(crate) const KIND_PARTIAL: &str = "archive-partial";
/// Notice cause: the archive was on a merge (not asked for by the user).
pub(crate) const CAUSE_MERGE: &str = "merge";

/// The `detail` of an `archive-partial` notice: the local branches a merge left behind.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub(crate) struct ArchivePartial {
    pub branches: Vec<LeftoverBranch>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub(crate) struct LeftoverBranch {
    /// The repo holding the branch, as `Workspace::repos` names it: `.` or a submodule
    /// path. The folder is worked out again from the project's root when needed.
    pub repo: String,
    pub branch: String,
    /// The commit the branch was verified merged at: only that is deleted.
    pub sha: String,
}

/// What an archive did beyond releasing the workspace.
#[derive(Debug, Default)]
pub(crate) struct Outcome {
    /// Verified branches that could not be deleted.
    pub left: Vec<LeftoverBranch>,
    /// Why, for the user.
    pub problems: Vec<String>,
}

/// Stops the task's sessions and waits until they are gone, so nothing writes to its
/// repos from here on.
pub(crate) async fn stop_sessions(app: &AppState, key: &TaskKey) {
    crate::permissions::release_task(app, key.as_str()).await;
    for live in app.sessions.of_task(key.as_str()) {
        // A terminal agent leads its own process group: what it started (a build, a
        // dev server) goes with it, not to keep writing in the workspace.
        let group = live.runtime.as_pty().and_then(|pty| pty.process_id());
        let _ = live.runtime.kill();
        live.runtime.wait().await;
        if let Some(pid) = group {
            end_group(pid, GRACE).await;
        }
    }
}

/// Stops the task's sessions and lets go of its workspace: a task in the project's
/// checkout goes back to where it was, a worktree goes away (unless work would be
/// lost and not `force`). The branch stays. Nothing to do if it has none (e.g. its
/// checkout was taken by a newer task). Its record stays for the caller to forget:
/// a deleted task takes it in the same statement as the task.
pub(crate) async fn release_workspace(
    app: &AppState,
    key: &TaskKey,
    force: bool,
) -> Result<(), ApiError> {
    // Its sessions stop even without a workspace: nothing else would track them.
    stop_sessions(app, key).await;
    let Some((root, ws)) = app.store.workspace(key).await? else {
        return Ok(());
    };
    // Nothing of it may still be writing in the folder while git deletes it, or the
    // removal stops halfway ("Directory not empty").
    app.previews.stop_and_wait(key.as_str()).await;
    let opts = GitOptions {
        allow_file_protocol: app.config.allow_file_protocol,
    };
    // A task in the project's checkout goes back to where it was; a worktree goes away.
    let previous = app.store.checkout_previous(key).await?;
    tokio::task::spawn_blocking(move || match previous {
        Some(previous) => {
            for stuck in delilah_git::leave_checkout(&root, &previous, &opts) {
                tracing::warn!(
                    "archiving {}: left on the task's branch: {stuck}",
                    ws.branch
                );
            }
            Ok(())
        }
        None => delilah_git::archive_workspace(&root, &ws, force, &opts),
    })
    .await
    .map_err(|e| ApiError::internal(e.to_string()))?
    .map_err(|e| ApiError::conflict(e.to_string()))?;
    Ok(())
}

/// Archives the task's workspace: releases it (see [`release_workspace`]), forgets it,
/// deletes the local task branch of each repo in `verified` (repo label -> the commit
/// it was verified merged at; only that commit is deleted) and resolves the task's PR
/// notices. A branch that cannot go is reported in the outcome, not as an error.
pub(crate) async fn archive_workspace(
    app: &AppState,
    key: &TaskKey,
    force: bool,
    verified: &HashMap<String, String>,
) -> Result<Outcome, ApiError> {
    let Some((root, ws)) = app.store.workspace(key).await? else {
        stop_sessions(app, key).await;
        return Ok(Outcome::default());
    };
    let opts = GitOptions {
        allow_file_protocol: app.config.allow_file_protocol,
    };
    // A parent's own repo has the task's branch too (the descriptive one made after the
    // start): after a merge it goes like a submodule's, at the tip the workspace had.
    let mut verified = verified.clone();
    if !verified.is_empty() && !verified.contains_key(".") {
        let (ws, root, opts) = (ws.clone(), root.clone(), opts.clone());
        let tip = tokio::task::spawn_blocking(move || {
            let home = Workspace::home_of(&root, ".")?;
            if !ws.parent_has_branch(&opts) {
                return None;
            }
            delilah_git::branch_tip(&home, &ws.branch, &opts).ok()?
        })
        .await
        .map_err(|e| ApiError::internal(e.to_string()))?;
        if let Some(tip) = tip {
            verified.insert(".".to_string(), tip);
        }
    }
    release_workspace(app, key, force).await?;
    app.store.delete_workspace(key).await?;

    let mut outcome = Outcome::default();
    let mut labels: Vec<_> = verified.iter().collect();
    labels.sort();
    for (label, sha) in labels {
        // Not set up in the project's checkout: the branch never lived there.
        let Some(dir) = Workspace::home_of(&root, label) else {
            continue;
        };
        // A task that never got a branch has none to delete.
        if !ws.has_branch() {
            continue;
        }
        let leftover = LeftoverBranch {
            repo: label.clone(),
            branch: ws.branch.clone(),
            sha: sha.clone(),
        };
        let (branch, expected, opts) = (leftover.branch.clone(), sha.clone(), opts.clone());
        let result = tokio::task::spawn_blocking(move || {
            delilah_git::delete_branch(&dir, &branch, Some(&expected), &opts)
        })
        .await;
        let problem = match result {
            Ok(Ok(())) => continue,
            Ok(Err(e)) => e.to_string(),
            Err(e) => e.to_string(),
        };
        outcome.problems.push(problem);
        outcome.left.push(leftover);
    }
    // Nothing left to act on: the notices of the PRs go with the workspace.
    app.store.resolve_all_notices_of_task(key).await?;
    Ok(outcome)
}

/// Deletes the branches an open `archive-partial` notice of the task lists and resolves
/// it. A failure (or a detail that cannot be read) keeps the notice and is a 409.
/// Whether a notice was resolved.
pub(crate) async fn finish_partial(app: &AppState, key: &TaskKey) -> Result<bool, ApiError> {
    let notices: Vec<_> = app
        .store
        .open_notices()
        .await?
        .into_iter()
        .filter(|n| n.task_key == key.as_str() && n.kind == KIND_PARTIAL)
        .collect();
    if notices.is_empty() {
        return Ok(false);
    }
    let opts = GitOptions {
        allow_file_protocol: app.config.allow_file_protocol,
    };
    let root = match app.store.get_task(key).await?.and_then(|t| t.project) {
        Some(project) => app.store.project(&project).await?.path,
        None => {
            return Err(ApiError::conflict(format!(
                "{key} has no project to find its branches in"
            )));
        }
    };
    let mut resolved = false;
    for notice in notices {
        let detail: ArchivePartial = notice
            .detail
            .clone()
            .and_then(|d| serde_json::from_value(d).ok())
            .ok_or_else(|| {
                ApiError::conflict(format!(
                    "cannot read what {key} left behind: {}",
                    notice.text
                ))
            })?;
        let (root, opts) = (root.clone(), opts.clone());
        tokio::task::spawn_blocking(move || {
            for b in &detail.branches {
                let dir = Workspace::home_of(&root, &b.repo)
                    .ok_or_else(|| format!("{} is not a repo of the project any more", b.repo))?;
                delilah_git::delete_branch(&dir, &b.branch, Some(&b.sha), &opts)
                    .map_err(|e| e.to_string())?;
            }
            Ok::<(), String>(())
        })
        .await
        .map_err(|e| ApiError::internal(e.to_string()))?
        .map_err(ApiError::conflict)?;
        resolved |= app.store.resolve_notice(notice.id).await?;
    }
    Ok(resolved)
}
