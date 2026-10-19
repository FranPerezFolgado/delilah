use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::command::{GitError, GitOptions, git};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Submodule {
    pub name: String,
    /// Path relative to the parent repo's root.
    pub path: String,
}

/// What to build: a worktree of the parent with only `submodules` initialised,
/// each on a new `branch`.
#[derive(Debug, Clone)]
pub struct WorkspaceSpec {
    /// Folder of the new workspace, e.g. `~/.delilah/workspaces/PROJ-1234`.
    pub path: PathBuf,
    pub branch: String,
    /// Submodule paths, as listed by [`list_submodules`].
    pub submodules: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Workspace {
    pub path: PathBuf,
    pub branch: String,
    pub submodules: Vec<String>,
}

pub fn list_submodules(parent: &Path) -> Result<Vec<Submodule>, GitError> {
    let opts = GitOptions::default();
    let out = match git(
        parent,
        &[
            "config",
            "-f",
            ".gitmodules",
            "--get-regexp",
            r"^submodule\..*\.path$",
        ],
        &opts,
    ) {
        Ok(out) => out,
        // `git config` exits with 1 when nothing matches: a parent without submodules.
        Err(GitError::Command { stderr, .. }) if stderr.is_empty() => return Ok(Vec::new()),
        Err(e) => return Err(e),
    };
    Ok(out
        .lines()
        .filter_map(|line| {
            let (key, path) = line.split_once(' ')?;
            let name = key.strip_prefix("submodule.")?.strip_suffix(".path")?;
            Some(Submodule {
                name: name.to_string(),
                path: path.to_string(),
            })
        })
        .collect())
}

/// Creates the workspace. The parent worktree is detached at the parent's `HEAD`:
/// branches and PRs live in the submodules, not in the parent.
pub fn create_workspace(
    parent: &Path,
    spec: &WorkspaceSpec,
    opts: &GitOptions,
) -> Result<Workspace, GitError> {
    if spec.path.exists() {
        return Err(GitError::WorkspaceExists(spec.path.clone()));
    }
    if git(
        parent,
        &["check-ref-format", "--branch", &spec.branch],
        opts,
    )
    .is_err()
    {
        return Err(GitError::InvalidBranch(spec.branch.clone()));
    }
    let known = list_submodules(parent)?;
    if let Some(unknown) = spec
        .submodules
        .iter()
        .find(|s| !known.iter().any(|k| &k.path == *s))
    {
        return Err(GitError::UnknownSubmodule(unknown.clone()));
    }

    let ws_path = path_str(&spec.path);
    git(
        parent,
        &["worktree", "add", "--detach", &ws_path, "HEAD"],
        opts,
    )?;

    let result = init_submodules(&spec.path, spec, opts);
    if let Err(e) = result {
        // Leave nothing half-built behind.
        let _ = git(parent, &["worktree", "remove", "--force", &ws_path], opts);
        return Err(e);
    }

    Ok(Workspace {
        path: spec.path.clone(),
        branch: spec.branch.clone(),
        submodules: spec.submodules.clone(),
    })
}

fn init_submodules(ws: &Path, spec: &WorkspaceSpec, opts: &GitOptions) -> Result<(), GitError> {
    if spec.submodules.is_empty() {
        return Ok(());
    }
    let mut args = vec!["submodule", "update", "--init", "--"];
    args.extend(spec.submodules.iter().map(String::as_str));
    git(ws, &args, opts)?;

    for sub in &spec.submodules {
        git(&ws.join(sub), &["checkout", "-b", &spec.branch], opts)?;
    }
    Ok(())
}

/// Removes the workspace. Refuses while any submodule has uncommitted changes or
/// commits on no remote: removing the worktree also deletes the submodules' git data.
pub fn archive_workspace(parent: &Path, ws: &Workspace, opts: &GitOptions) -> Result<(), GitError> {
    for sub in &ws.submodules {
        let dir = ws.path.join(sub);
        if !git(&dir, &["status", "--porcelain"], opts)?.is_empty() {
            return Err(GitError::Dirty {
                submodule: sub.clone(),
            });
        }
        let ahead = git(
            &dir,
            &["rev-list", "--count", "HEAD", "--not", "--remotes"],
            opts,
        )?;
        let count = ahead.parse().unwrap_or(0);
        if count > 0 {
            return Err(GitError::Unpushed {
                submodule: sub.clone(),
                count,
            });
        }
    }
    // `--force` is required by git for worktrees with submodules; the checks above
    // are what keep it safe.
    git(
        parent,
        &["worktree", "remove", "--force", &path_str(&ws.path)],
        opts,
    )?;
    git(parent, &["worktree", "prune"], opts)?;
    Ok(())
}

fn path_str(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}
