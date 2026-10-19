use std::path::{Path, PathBuf};

use delilah_core::ProjectKind;
use serde::{Deserialize, Serialize};

use crate::command::{GitError, GitOptions, git};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Submodule {
    pub name: String,
    /// Path relative to the parent repo's root.
    pub path: String,
}

/// What to build for a task.
#[derive(Debug, Clone)]
pub struct WorkspaceSpec {
    /// Folder of the new workspace, e.g. `~/.delilah/workspaces/PROJ-1234`.
    pub path: PathBuf,
    pub branch: String,
    pub kind: ProjectKind,
    /// `parent` only: submodule paths, as listed by [`list_submodules`].
    pub submodules: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Workspace {
    pub path: PathBuf,
    pub branch: String,
    pub kind: ProjectKind,
    pub submodules: Vec<String>,
}

impl Workspace {
    /// Folders that hold the task's branch: each submodule, or the repo itself.
    fn branch_dirs(&self) -> Vec<(String, PathBuf)> {
        match self.kind {
            ProjectKind::Parent => self
                .submodules
                .iter()
                .map(|sub| (sub.clone(), self.path.join(sub)))
                .collect(),
            ProjectKind::Repo => vec![("the workspace".into(), self.path.clone())],
        }
    }
}

/// Top folder of the repo containing `path`.
pub fn repo_root(path: &Path) -> Result<PathBuf, GitError> {
    let root = git(
        path,
        &["rev-parse", "--show-toplevel"],
        &GitOptions::default(),
    )?;
    Ok(PathBuf::from(root))
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

/// Creates the workspace of `root` (the parent or the repo) described by `spec`.
///
/// - `parent`: a worktree detached at the parent's `HEAD`, with only `submodules`
///   initialised, each on `branch`. Branches and PRs live in the submodules.
/// - `repo`: a worktree of the repo on a new `branch`.
pub fn create_workspace(
    root: &Path,
    spec: &WorkspaceSpec,
    opts: &GitOptions,
) -> Result<Workspace, GitError> {
    if spec.path.exists() {
        return Err(GitError::WorkspaceExists(spec.path.clone()));
    }
    if git(root, &["check-ref-format", "--branch", &spec.branch], opts).is_err() {
        return Err(GitError::InvalidBranch(spec.branch.clone()));
    }
    let ws_path = path_str(&spec.path);

    match spec.kind {
        ProjectKind::Repo => {
            if !spec.submodules.is_empty() {
                return Err(GitError::SubmodulesInRepo);
            }
            git(
                root,
                &["worktree", "add", "-b", &spec.branch, &ws_path, "HEAD"],
                opts,
            )?;
        }
        ProjectKind::Parent => {
            let known = list_submodules(root)?;
            if let Some(unknown) = spec
                .submodules
                .iter()
                .find(|s| !known.iter().any(|k| &k.path == *s))
            {
                return Err(GitError::UnknownSubmodule(unknown.clone()));
            }
            git(
                root,
                &["worktree", "add", "--detach", &ws_path, "HEAD"],
                opts,
            )?;
            if let Err(e) = init_submodules(&spec.path, spec, opts) {
                // Leave nothing half-built behind.
                let _ = git(root, &["worktree", "remove", "--force", &ws_path], opts);
                return Err(e);
            }
        }
    }

    Ok(Workspace {
        path: spec.path.clone(),
        branch: spec.branch.clone(),
        kind: spec.kind,
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

/// Removes the workspace. Refuses while the task's branch has uncommitted changes or
/// commits on no remote: in a `parent` workspace, removing the worktree also deletes
/// the submodules' git data. A `repo` workspace keeps its branch in the repo.
pub fn archive_workspace(root: &Path, ws: &Workspace, opts: &GitOptions) -> Result<(), GitError> {
    let only_this_branch = format!("--exclude={}", ws.branch);
    for (location, dir) in ws.branch_dirs() {
        if !git(&dir, &["status", "--porcelain"], opts)?.is_empty() {
            return Err(GitError::Dirty { location });
        }
        // Commits of this branch that are neither pushed nor on another local branch
        // (so a local, unpushed `main` it started from does not count).
        let ahead = git(
            &dir,
            &[
                "rev-list",
                "--count",
                "HEAD",
                "--not",
                "--remotes",
                &only_this_branch,
                "--branches",
            ],
            opts,
        )?;
        let count = ahead.parse().unwrap_or(0);
        if count > 0 {
            return Err(GitError::Unpushed { location, count });
        }
    }
    // `--force` is required by git for worktrees with submodules; the checks above
    // are what keep it safe.
    git(
        root,
        &["worktree", "remove", "--force", &path_str(&ws.path)],
        opts,
    )?;
    git(root, &["worktree", "prune"], opts)?;
    Ok(())
}

fn path_str(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}
