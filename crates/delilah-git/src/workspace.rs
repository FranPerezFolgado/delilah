use std::collections::HashMap;
use std::path::{Path, PathBuf};

use delilah_core::ProjectKind;
use serde::{Deserialize, Serialize};

use crate::base::{FetchOutcome, RepoBase, prepare_base};
use crate::command::{Deadline, GitError, GitOptions, NETWORK_LIMIT, git, git_code};

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
    /// `None`: no branch yet (the workspace starts detached at `HEAD`).
    pub branch: Option<String>,
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

    /// The folder that holds the task's branch once the workspace is gone, for the repo
    /// `label` (see [`Workspace::repos`]) of the project at `root`: its own checkout, or
    /// the submodule there when it is set up. `None` when the folder is not a repo, or
    /// when the label would lead out of `root`.
    pub fn home_of(root: &Path, label: &str) -> Option<PathBuf> {
        let root = root.canonicalize().ok()?;
        if label == "." {
            return Some(root);
        }
        let dir = root.join(label).canonicalize().ok()?;
        // An uninitialised submodule folder would make git answer for the parent.
        (dir.starts_with(&root) && dir != root && dir.join(".git").exists()).then_some(dir)
    }

    /// Whether a parent workspace is on the task's branch in the parent's own repo too (a
    /// descriptive branch made after the start; older tasks never were, and a branch of
    /// the same name that the parent happens to have is not theirs).
    pub fn parent_has_branch(&self, opts: &GitOptions) -> bool {
        self.kind == ProjectKind::Parent
            && self.has_branch()
            && current_branch(&self.path, opts).ok().flatten().as_deref()
                == Some(self.branch.as_str())
    }

    /// Whether the task has a branch yet: an empty name means the first analysis has
    /// not created it.
    pub fn has_branch(&self) -> bool {
        !self.branch.is_empty()
    }

    /// The repos (as in [`Workspace::repos`]) with uncommitted changes. Errors when a
    /// folder cannot be read by git.
    pub fn uncommitted(&self, opts: &GitOptions) -> Result<Vec<String>, GitError> {
        let mut dirty = Vec::new();
        for (label, dir) in self.repos() {
            if !git(&dir, &["status", "--porcelain"], opts)?.is_empty() {
                dirty.push(label);
            }
        }
        Ok(dirty)
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

/// Names of the folders in `root/specs/` that hold a `spec.md`, sorted. Empty when
/// there is no `specs/` folder.
pub fn spec_dirs(root: &Path) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(root.join("specs")) else {
        return Vec::new();
    };
    let mut names: Vec<String> = entries
        .filter_map(Result::ok)
        .filter(|e| e.path().join("spec.md").is_file())
        .filter_map(|e| e.file_name().into_string().ok())
        .collect();
    names.sort();
    names
}

/// The branch checked out in `dir`, or `None` on a detached HEAD.
pub fn current_branch(dir: &Path, opts: &GitOptions) -> Result<Option<String>, GitError> {
    let name = git(dir, &["branch", "--show-current"], opts)?;
    Ok((!name.is_empty()).then_some(name))
}

/// Starts the branch `name` at the current `HEAD` of `dir` and switches to it, keeping
/// the uncommitted files. Fails if the branch already exists.
pub fn apply_branch(dir: &Path, name: &str, opts: &GitOptions) -> Result<(), GitError> {
    if git(dir, &["check-ref-format", "--branch", name], opts).is_err() {
        return Err(GitError::InvalidBranch(name.to_string()));
    }
    git(dir, &["checkout", "-q", "-b", name], opts)?;
    Ok(())
}

/// Switches `dir` to the existing branch `name`, keeping the uncommitted files. Fails if
/// git cannot carry them over, or the name is not a valid branch name.
pub fn switch_branch(dir: &Path, name: &str, opts: &GitOptions) -> Result<(), GitError> {
    if git(dir, &["check-ref-format", "--branch", name], opts).is_err() {
        return Err(GitError::InvalidBranch(name.to_string()));
    }
    git(dir, &["checkout", "-q", name], opts)?;
    Ok(())
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
/// - `parent`: a worktree detached at the parent's fresh `origin` default branch, with
///   only `submodules` initialised, each on a new `branch` that starts at its own
///   fresh `origin` default. Branches and PRs live in the submodules.
/// - `repo`: a worktree of the repo on a new `branch` that starts at `origin`'s default
///   branch, fetched first (bounded by `deadline`). No local branch moves and the new
///   branch has no upstream. A failed fetch falls back to the last known `origin`.
///
/// Also returns the base each repo started from (`.` is the parent for `parent`).
pub fn create_workspace(
    root: &Path,
    spec: &WorkspaceSpec,
    opts: &GitOptions,
    deadline: Deadline,
) -> Result<(Workspace, Vec<RepoBase>), GitError> {
    if spec.path.exists() {
        return Err(GitError::WorkspaceExists(spec.path.clone()));
    }
    if let Some(branch) = &spec.branch
        && git(root, &["check-ref-format", "--branch", branch], opts).is_err()
    {
        return Err(GitError::InvalidBranch(branch.clone()));
    }
    let ws_path = path_str(&spec.path);

    let mut bases = Vec::new();
    match spec.kind {
        ProjectKind::Repo => {
            if !spec.submodules.is_empty() {
                return Err(GitError::SubmodulesInRepo);
            }
            let base = prepare_base(root, ".", opts, deadline);
            let start = if base.sha.is_empty() {
                base.refname.clone()
            } else {
                base.sha.clone()
            };
            git(
                root,
                &[
                    "worktree",
                    "add",
                    "--no-track",
                    "-b",
                    &spec.branch,
                    &ws_path,
                    &start,
                ],
                opts,
            )?;
            bases.push(base);
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
            let base = prepare_base(root, ".", opts, deadline);
            let start = if base.sha.is_empty() {
                "HEAD".to_string()
            } else {
                base.sha.clone()
            };
            git(
                root,
                &["worktree", "add", "--detach", &ws_path, &start],
                opts,
            )?;
            bases.push(base);
            match init_submodules(&spec.path, spec, opts, deadline) {
                Ok(subs) => bases.extend(subs),
                Err(e) => {
                    // Leave nothing half-built behind.
                    let _ = git(root, &["worktree", "remove", "--force", &ws_path], opts);
                    return Err(e);
                }
            }
        }
    }

    let ws = Workspace {
        path: spec.path.clone(),
        branch: spec.branch.clone().unwrap_or_default(),
        kind: spec.kind,
        submodules: spec.submodules.clone(),
    };
    Ok((ws, bases))
}

fn init_submodules(
    ws: &Path,
    spec: &WorkspaceSpec,
    opts: &GitOptions,
    deadline: Deadline,
) -> Result<Vec<RepoBase>, GitError> {
    if spec.submodules.is_empty() {
        return Ok(Vec::new());
    }
    let mut args = vec!["submodule", "update", "--init", "--"];
    args.extend(spec.submodules.iter().map(String::as_str));
    git(ws, &args, opts)?;

    let mut bases = Vec::new();
    for sub in &spec.submodules {
        let dir = ws.join(sub);
        let base = prepare_base(&dir, sub, opts, deadline);
        let start = if base.sha.is_empty() {
            base.refname.clone()
        } else {
            base.sha.clone()
        };
        git(
            &dir,
            &["checkout", "-q", "--no-track", "-b", &spec.branch, &start],
            opts,
        )?;
        bases.push(base);
    }
    Ok(bases)
}

/// Removes the workspace. Refuses while the task's branch has uncommitted changes or
/// commits on no remote: in a `parent` workspace, removing the worktree also deletes
/// the submodules' git data. A `repo` workspace keeps its branch in the repo.
/// `force` skips those checks: the user accepted losing that work.
pub fn archive_workspace(
    root: &Path,
    ws: &Workspace,
    force: bool,
    opts: &GitOptions,
) -> Result<(), GitError> {
    if !force {
        check_workspace(ws, opts)?;
    }
    remove_workspace(root, ws, opts)
}

/// Fails, touching nothing, if removing the workspace would lose work: uncommitted
/// changes, or commits of the task's branch on no remote.
pub fn check_workspace(ws: &Workspace, opts: &GitOptions) -> Result<(), GitError> {
    // Removed by hand: nothing left in it to lose, and the branch stays in the repo.
    if !ws.path.exists() {
        return Ok(());
    }
    for (location, dir) in ws.branch_dirs() {
        check_dir(&location, &dir, &ws.branch, false, opts)?;
    }
    if ws.parent_has_branch(opts) {
        check_dir("the parent", &ws.path, &ws.branch, true, opts)?;
    }
    Ok(())
}

/// [`check_workspace`] for only the repos of `ws` whose label (see [`Workspace::repos`])
/// is in `labels`: for the repos an archive after a merge has no merged PR to vouch for.
pub fn check_repos(ws: &Workspace, labels: &[String], opts: &GitOptions) -> Result<(), GitError> {
    if !ws.path.exists() {
        return Ok(());
    }
    for (label, dir) in ws.repos() {
        if labels.contains(&label) {
            let location = if label == "." {
                "the workspace".to_string()
            } else {
                label
            };
            check_dir(&location, &dir, &ws.branch, false, opts)?;
        }
    }
    Ok(())
}

/// `ignore_submodules`: for a parent's own repo, whose submodules are checked on their own.
fn check_dir(
    location: &str,
    dir: &Path,
    branch: &str,
    ignore_submodules: bool,
    opts: &GitOptions,
) -> Result<(), GitError> {
    let location = location.to_string();
    let mut status = vec!["status", "--porcelain"];
    if ignore_submodules {
        status.push("--ignore-submodules=all");
    }
    if !git(dir, &status, opts)?.is_empty() {
        return Err(GitError::Dirty { location });
    }
    // Commits of this branch that are neither pushed nor on another local branch
    // (so a local, unpushed `main` it started from does not count).
    let only_this_branch = format!("--exclude={branch}");
    let ahead = git(
        dir,
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
    Ok(())
}

/// Removes the workspace's worktree, whatever is in it (see [`check_workspace`]).
/// The branch stays.
pub fn remove_workspace(root: &Path, ws: &Workspace, opts: &GitOptions) -> Result<(), GitError> {
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

/// Per folder of a checkout-mode workspace, the branch (or commit) it was on.
pub type Previous = Vec<(String, String)>;

/// Puts the task's branch in the project's own checkout instead of a worktree:
/// creates it (or switches to it) in the repo, or in each chosen submodule. Refuses
/// while there are uncommitted changes (before any fetch). A new branch starts at the
/// fresh `origin` default branch (see [`prepare_base`]) with no upstream; one that
/// already exists is switched to as it is, never moved. Returns the workspace, per
/// folder the branch (or commit) it was on, to go back there when the task is archived,
/// and the base each folder's branch started from.
pub fn use_checkout(
    root: &Path,
    spec: &WorkspaceSpec,
    opts: &GitOptions,
    deadline: Deadline,
) -> Result<(Workspace, Previous, Vec<RepoBase>), GitError> {
    if git(root, &["check-ref-format", "--branch", &spec.branch], opts).is_err() {
        return Err(GitError::InvalidBranch(spec.branch.clone()));
    }
    let mut dirs = checkout_dirs(root, spec)?;
    for (location, dir) in &dirs {
        if !git(dir, &["status", "--porcelain"], opts)?.is_empty() {
            return Err(GitError::Dirty {
                location: location.clone(),
            });
        }
    }
    let mut previous: Previous = Vec::new();
    let mut bases: Vec<RepoBase> = Vec::new();
    for (location, dir) in &dirs {
        let was = git(dir, &["symbolic-ref", "--short", "-q", "HEAD"], opts)
            .ok()
            .filter(|b| !b.is_empty())
            .map_or_else(|| git(dir, &["rev-parse", "HEAD"], opts), Ok)?;
        let Some(branch) = &spec.branch else {
            previous.push((location.clone(), was));
            continue;
        };
        let exists = git(
            dir,
            &[
                "rev-parse",
                "--verify",
                "--quiet",
                &format!("refs/heads/{branch}"),
            ],
            opts,
        )
        .is_ok();
        let switched = if exists {
            git(dir, &["checkout", "-q", &spec.branch], opts)
                .and_then(|_| git(dir, &["rev-parse", "HEAD"], opts))
                .map(|sha| RepoBase {
                    repo: location.clone(),
                    refname: spec.branch.clone(),
                    sha,
                    fetch: FetchOutcome::Existing,
                })
        } else {
            let base = prepare_base(dir, location, opts, deadline);
            let start = if base.sha.is_empty() {
                base.refname.clone()
            } else {
                base.sha.clone()
            };
            git(
                dir,
                &["checkout", "-q", "--no-track", "-b", &spec.branch, &start],
                opts,
            )
            .map(|_| base)
        };
        let base = match switched {
            Ok(base) => base,
            Err(e) => {
                // Leave nothing half-switched behind, and no branch this call made.
                for ((loc, back), base) in previous.iter().zip(&bases) {
                    let d = if loc == "." {
                        root.to_path_buf()
                    } else {
                        root.join(loc)
                    };
                    if git(&d, &["checkout", "-q", back], opts).is_ok()
                        && base.fetch != FetchOutcome::Existing
                    {
                        let _ = git(&d, &["branch", "-D", &spec.branch], opts);
                    }
                }
                return Err(e);
            }
        };
        bases.push(base);
        previous.push((location.clone(), was));
    }
    let ws = Workspace {
        path: root.to_path_buf(),
        branch: spec.branch.clone().unwrap_or_default(),
        kind: spec.kind,
        submodules: spec.submodules.clone(),
    };
    Ok((ws, previous, bases))
}

/// Ends a task that worked in the project's checkout: goes back to the branches it
/// was on. The task's branch stays (nothing is deleted), so unpushed commits are kept,
/// and uncommitted files stay in the checkout (git carries them over).
///
/// Never refuses: the checkout is the user's own, and a task done with it must let go
/// of it. Returns, for each folder that could not go back, why (e.g. a change git
/// would overwrite); those stay on the task's branch for the user to sort out.
pub fn leave_checkout(
    root: &Path,
    previous: &[(String, String)],
    opts: &GitOptions,
) -> Vec<String> {
    let dir = |loc: &str| {
        if loc == "." {
            root.to_path_buf()
        } else {
            root.join(loc)
        }
    };
    let mut stuck = Vec::new();
    for (location, back) in previous {
        let dir = dir(location);
        let current = git(&dir, &["branch", "--show-current"], opts).unwrap_or_default();
        if current == *back {
            continue;
        }
        if let Err(e) = git(&dir, &["checkout", "-q", back], opts) {
            stuck.push(format!("{location}: {e}"));
        }
    }
    stuck
}

fn check_branch(branch: &str) -> Result<(), GitError> {
    if branch.is_empty() || branch.starts_with('-') {
        return Err(GitError::InvalidBranch(branch.to_string()));
    }
    Ok(())
}

/// Subjects of the commits on `branch` that are not in `base`, oldest first. Prefers
/// `origin/<base>` over the local `<base>` when it exists.
pub fn commit_subjects(
    dir: &Path,
    base: &str,
    branch: &str,
    opts: &GitOptions,
) -> Result<Vec<String>, GitError> {
    check_branch(branch)?;
    check_branch(base)?;
    let remote_base = format!("origin/{base}");
    let base_ref = if git(
        dir,
        &["rev-parse", "--verify", "--quiet", &remote_base],
        opts,
    )
    .is_ok()
    {
        remote_base
    } else {
        base.to_string()
    };
    let range = format!("{base_ref}..{branch}");
    let out = git(dir, &["log", "--reverse", "--format=%s", &range], opts)?;
    Ok(out.lines().map(str::to_string).collect())
}

/// True when `branch` has no upstream or has commits its upstream lacks.
pub fn needs_push(dir: &Path, branch: &str, opts: &GitOptions) -> Result<bool, GitError> {
    check_branch(branch)?;
    let upstream = format!("{branch}@{{upstream}}");
    if git(dir, &["rev-parse", "--abbrev-ref", &upstream], opts).is_err() {
        return Ok(true);
    }
    let range = format!("{upstream}..{branch}");
    let count = git(dir, &["rev-list", "--count", &range], opts)?;
    Ok(count.trim().parse::<u32>().unwrap_or(1) > 0)
}

/// `git push -u origin <branch>`.
pub fn push_branch(dir: &Path, branch: &str, opts: &GitOptions) -> Result<(), GitError> {
    check_branch(branch)?;
    git(dir, &["push", "-u", "origin", branch], opts).map(|_| ())
}

/// True when `name` is a local branch, or a remote-tracking branch of any remote, in `dir`.
pub fn branch_taken(dir: &Path, name: &str, opts: &GitOptions) -> Result<bool, GitError> {
    let out = git(
        dir,
        &[
            "for-each-ref",
            "--format=%(refname)",
            "refs/heads",
            "refs/remotes",
        ],
        opts,
    )?;
    // Remote names may contain `/`, so strip each known remote instead of splitting.
    let remotes = git(dir, &["remote"], opts)?;
    let local = format!("refs/heads/{name}");
    Ok(out.lines().any(|r| {
        r == local
            || remotes.lines().any(|remote| {
                r.strip_prefix("refs/remotes/")
                    .and_then(|rest| rest.strip_prefix(remote))
                    .and_then(|rest| rest.strip_prefix('/'))
                    == Some(name)
            })
    }))
}

/// `base` if no repo in `dirs` has that branch, else the first free `base-2`..`base-99`,
/// else `base` with the unix time.
pub fn free_branch(dirs: &[PathBuf], base: &str, opts: &GitOptions) -> Result<String, GitError> {
    let taken = |name: &str| -> Result<bool, GitError> {
        for dir in dirs {
            if branch_taken(dir, name, opts)? {
                return Ok(true);
            }
        }
        Ok(false)
    };
    if !taken(base)? {
        return Ok(base.to_string());
    }
    for n in 2..=99 {
        let name = format!("{base}-{n}");
        if !taken(&name)? {
            return Ok(name);
        }
    }
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    Ok(format!("{base}-{secs}"))
}

/// Fetches `origin`, giving up at `deadline`. The error is git's stderr, or why it
/// did not run.
pub fn fetch(dir: &Path, opts: &GitOptions, deadline: Deadline) -> Result<(), String> {
    // The same lock and limits as `prepare_base`, so they never fetch at once and a
    // hung remote holds neither for long. Never reuses an earlier fetch: a merge check
    // must see the remote as it is now.
    crate::base::locked_fetch(dir, opts, deadline, std::time::Duration::ZERO)
}

/// Fails, touching nothing, if archiving after a merged PR would lose work: uncommitted
/// changes, or local commits of the task's branch beyond the PR's head. `heads` maps
/// each repo label (see [`Workspace::repos`]) to the head the merged PR had; repos
/// without an entry are not checked. A squash merge leaves the PR head out of the
/// base branch, so the check is against that head, not against the base.
pub fn check_merged(
    ws: &Workspace,
    heads: &HashMap<String, String>,
    opts: &GitOptions,
) -> Result<(), GitError> {
    // Removed by hand: nothing left in it to lose.
    if !ws.path.exists() {
        return Ok(());
    }
    if ws.kind == ProjectKind::Parent
        && !git(
            &ws.path,
            &["status", "--porcelain", "--ignore-submodules=all"],
            opts,
        )?
        .is_empty()
    {
        return Err(GitError::Dirty {
            location: "the parent".into(),
        });
    }
    for (location, dir) in ws.repos() {
        let Some(head) = heads.get(&location) else {
            continue;
        };
        if !git(&dir, &["status", "--porcelain"], opts)?.is_empty() {
            return Err(GitError::Dirty { location });
        }
        let args = ["merge-base", "--is-ancestor", ws.branch.as_str(), head];
        let mut code = git_code(&dir, &args, opts)?;
        if code > 1 {
            // Probably a head this clone has not fetched yet.
            fetch(&dir, opts, Deadline::after(NETWORK_LIMIT)).map_err(|stderr| {
                GitError::Command {
                    dir: dir.clone(),
                    args: "fetch origin".into(),
                    stderr,
                }
            })?;
            code = git_code(&dir, &args, opts)?;
        }
        match code {
            0 => {}
            1 => return Err(GitError::NotMerged { location }),
            _ => {
                // Surface git's own message.
                git(&dir, &args, opts)?;
                return Err(GitError::NotMerged { location });
            }
        }
    }
    Ok(())
}

/// The commit the local `branch` points at, or `None` when there is no such branch.
pub fn branch_tip(dir: &Path, branch: &str, opts: &GitOptions) -> Result<Option<String>, GitError> {
    check_branch(branch)?;
    let reference = format!("refs/heads/{branch}");
    if git_code(dir, &["show-ref", "--verify", "--quiet", &reference], opts)? != 0 {
        return Ok(None);
    }
    git(dir, &["rev-parse", &reference], opts).map(Some)
}

/// A full commit id: what is safe to hand to git as one.
pub fn is_sha(s: &str) -> bool {
    (40..=64).contains(&s.len()) && s.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

/// Deletes the local `branch`. Fine if it is already gone; refuses the checked-out one.
/// With `expected` (a commit id) it deletes only if the branch still points there, so
/// a commit made since is never lost.
pub fn delete_branch(
    dir: &Path,
    branch: &str,
    expected: Option<&str>,
    opts: &GitOptions,
) -> Result<(), GitError> {
    check_branch(branch)?;
    if expected.is_some_and(|sha| !is_sha(sha)) {
        return Err(GitError::InvalidBranch(format!(
            "{branch}@{}",
            expected.unwrap_or_default()
        )));
    }
    let reference = format!("refs/heads/{branch}");
    if git_code(dir, &["show-ref", "--verify", "--quiet", &reference], opts)? != 0 {
        return Ok(());
    }
    if git(dir, &["symbolic-ref", "--short", "HEAD"], opts).is_ok_and(|cur| cur == branch) {
        return Err(GitError::Command {
            dir: dir.to_path_buf(),
            args: format!("branch -D {branch}"),
            stderr: "it is the checked-out branch".into(),
        });
    }
    match expected {
        Some(sha) => git(dir, &["update-ref", "-d", &reference, sha], opts).map(|_| ()),
        None => git(dir, &["branch", "-D", branch], opts).map(|_| ()),
    }
}
