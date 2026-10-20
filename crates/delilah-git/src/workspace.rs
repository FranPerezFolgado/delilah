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
    /// The repos where the task's branch lives, by path in the workspace: each
    /// submodule of a parent, or `.` for a plain repo.
    pub fn repos(&self) -> Vec<(String, PathBuf)> {
        match self.kind {
            ProjectKind::Parent => self.branch_dirs(),
            ProjectKind::Repo => vec![(".".into(), self.path.clone())],
        }
    }

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

/// Files under `pathspec` that the task's branch touched, newest first: uncommitted
/// changes, then those of the commits that are on no remote and no other local branch
/// (the same commits archiving refuses to lose). Paths are relative to `dir`.
///
/// Ignored files count as changes: a new worktree starts without them, so any there
/// were made in it (e.g. SpecKit's `specs/`, which some setups keep out of git).
pub fn changed_paths(
    dir: &Path,
    pathspec: &str,
    branch: &str,
    opts: &GitOptions,
) -> Result<Vec<String>, GitError> {
    let status = git(
        dir,
        &[
            "status",
            "--porcelain",
            "--untracked-files=all",
            "--ignored",
            "--",
            pathspec,
        ],
        opts,
    )?;
    let uncommitted = status.lines().filter_map(|line| {
        let path = line.get(3..)?;
        // A rename reads `old -> new`.
        Some(path.rsplit(" -> ").next().unwrap_or(path).trim_matches('"'))
    });
    let only_this_branch = format!("--exclude={branch}");
    // A repo with no commits yet has no history to read.
    let log = git(
        dir,
        &[
            "log",
            "--format=",
            "--name-only",
            "HEAD",
            "--not",
            "--remotes",
            &only_this_branch,
            "--branches",
            "--",
            pathspec,
        ],
        opts,
    )
    .unwrap_or_default();
    let committed = log.lines().filter(|l| !l.is_empty());
    let mut paths: Vec<String> = Vec::new();
    for path in uncommitted.chain(committed) {
        if !paths.iter().any(|p| p == path) {
            paths.push(path.to_string());
        }
    }
    Ok(paths)
}

/// The folders a task working in the project's own checkout puts its branch in:
/// the repo itself, or each chosen submodule (which must be initialised there).
fn checkout_dirs(root: &Path, spec: &WorkspaceSpec) -> Result<Vec<(String, PathBuf)>, GitError> {
    match spec.kind {
        ProjectKind::Repo if !spec.submodules.is_empty() => Err(GitError::SubmodulesInRepo),
        ProjectKind::Repo => Ok(vec![(".".into(), root.to_path_buf())]),
        ProjectKind::Parent => spec
            .submodules
            .iter()
            .map(|sub| {
                let dir = root.join(sub);
                if dir.join(".git").exists() {
                    Ok((sub.clone(), dir))
                } else {
                    Err(GitError::NotInitialised(sub.clone()))
                }
            })
            .collect(),
    }
}

/// Puts the task's branch in the project's own checkout instead of a worktree:
/// creates it (or switches to it) in the repo, or in each chosen submodule. Refuses
/// while there are uncommitted changes. Returns the workspace and, per folder, the
/// branch (or commit) it was on, to go back there when the task is archived.
pub fn use_checkout(
    root: &Path,
    spec: &WorkspaceSpec,
    opts: &GitOptions,
) -> Result<(Workspace, Vec<(String, String)>), GitError> {
    if git(root, &["check-ref-format", "--branch", &spec.branch], opts).is_err() {
        return Err(GitError::InvalidBranch(spec.branch.clone()));
    }
    let dirs = checkout_dirs(root, spec)?;
    for (location, dir) in &dirs {
        if !git(dir, &["status", "--porcelain"], opts)?.is_empty() {
            return Err(GitError::Dirty {
                location: location.clone(),
            });
        }
    }
    let mut previous: Vec<(String, String)> = Vec::new();
    for (location, dir) in &dirs {
        let was = git(dir, &["symbolic-ref", "--short", "-q", "HEAD"], opts)
            .ok()
            .filter(|b| !b.is_empty())
            .map_or_else(|| git(dir, &["rev-parse", "HEAD"], opts), Ok)?;
        let exists = git(
            dir,
            &[
                "rev-parse",
                "--verify",
                "--quiet",
                &format!("refs/heads/{}", spec.branch),
            ],
            opts,
        )
        .is_ok();
        let switched = if exists {
            git(dir, &["checkout", "-q", &spec.branch], opts)
        } else {
            git(dir, &["checkout", "-q", "-b", &spec.branch], opts)
        };
        if let Err(e) = switched {
            // Leave nothing half-switched behind.
            for (loc, back) in &previous {
                let d = if loc == "." {
                    root.to_path_buf()
                } else {
                    root.join(loc)
                };
                let _ = git(&d, &["checkout", "-q", back], opts);
            }
            return Err(e);
        }
        previous.push((location.clone(), was));
    }
    let ws = Workspace {
        path: root.to_path_buf(),
        branch: spec.branch.clone(),
        kind: spec.kind,
        submodules: spec.submodules.clone(),
    };
    Ok((ws, previous))
}

/// Ends a task that worked in the project's checkout: goes back to the branches it
/// was on. The task's branch stays (nothing is deleted), so unpushed commits are kept;
/// only uncommitted changes stop it.
pub fn leave_checkout(
    root: &Path,
    previous: &[(String, String)],
    opts: &GitOptions,
) -> Result<(), GitError> {
    let dir = |loc: &str| {
        if loc == "." {
            root.to_path_buf()
        } else {
            root.join(loc)
        }
    };
    for (location, _) in previous {
        if !git(&dir(location), &["status", "--porcelain"], opts)?.is_empty() {
            return Err(GitError::Dirty {
                location: location.clone(),
            });
        }
    }
    for (location, back) in previous {
        git(&dir(location), &["checkout", "-q", back], opts)?;
    }
    Ok(())
}
