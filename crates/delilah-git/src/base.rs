//! The commit a new task branch starts from: `origin`'s default branch, freshly fetched.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use crate::command::{Deadline, GitError, GitOptions, git, git_with_timeout};

/// A fetch that finished less ago than this is reused by the next caller.
const FRESH_FOR: Duration = Duration::from_secs(5);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FetchOutcome {
    Fetched,
    /// The repo has no `origin`: nothing to fetch, nothing to warn about.
    NoRemote,
    /// The fetch failed or timed out; the base is the last one known.
    Failed(String),
    /// The branch already exists, so it was not re-based.
    Existing,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepoBase {
    pub repo: String,
    pub refname: String,
    pub sha: String,
    pub fetch: FetchOutcome,
}

type RepoLock = Arc<Mutex<Option<Instant>>>;

/// One lock per repo, process-wide; the value is when it last fetched.
pub(crate) fn repo_lock(dir: &Path) -> RepoLock {
    static FETCHING: OnceLock<Mutex<HashMap<PathBuf, RepoLock>>> = OnceLock::new();
    let key = dir.canonicalize().unwrap_or_else(|_| dir.to_path_buf());
    let mut map = FETCHING
        .get_or_init(Default::default)
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    map.entry(key).or_default().clone()
}

fn has_origin(dir: &Path, opts: &GitOptions) -> bool {
    git(dir, &["remote", "get-url", "origin"], opts).is_ok()
}

fn verify(dir: &Path, refname: &str, opts: &GitOptions) -> Option<String> {
    git(
        dir,
        &[
            "rev-parse",
            "--verify",
            "--quiet",
            &format!("{refname}^{{commit}}"),
        ],
        opts,
    )
    .ok()
    .filter(|s| !s.is_empty())
}

/// The first usable of `origin/HEAD` (by the name it points to), `origin/main`,
/// `origin/master`, `main`, `master` and `HEAD`.
fn resolve_base(dir: &Path, opts: &GitOptions) -> Option<(String, String)> {
    let head = git(
        dir,
        &["symbolic-ref", "--short", "refs/remotes/origin/HEAD"],
        opts,
    )
    .ok()
    .filter(|s| !s.is_empty());
    head.into_iter()
        .chain(
            ["origin/main", "origin/master", "main", "master", "HEAD"]
                .into_iter()
                .map(String::from),
        )
        .find_map(|r| verify(dir, &r, opts).map(|sha| (r, sha)))
}

/// Fetches `origin` under the repo's lock. `Ok(())` also when a fetch just finished.
fn locked_fetch(dir: &Path, opts: &GitOptions, deadline: Deadline) -> Result<(), String> {
    let lock = repo_lock(dir);
    let mut last = loop {
        match lock.try_lock() {
            Ok(g) => break g,
            Err(std::sync::TryLockError::Poisoned(e)) => break e.into_inner(),
            Err(std::sync::TryLockError::WouldBlock) => {
                if deadline.expired() {
                    return Err("another fetch of this repo is still running".into());
                }
                std::thread::sleep(Duration::from_millis(50).min(deadline.remaining()));
            }
        }
    };
    if last.is_some_and(|t| t.elapsed() < FRESH_FOR) {
        return Ok(());
    }
    git_with_timeout(
        dir,
        &["fetch", "--quiet", "--no-tags", "origin"],
        opts,
        deadline,
    )
    .map_err(|e| match e {
        GitError::Command { stderr, .. } if !stderr.is_empty() => stderr,
        e => e.to_string(),
    })?;
    *last = Some(Instant::now());
    if verify(dir, "refs/remotes/origin/HEAD", opts).is_none() {
        let _ = git_with_timeout(
            dir,
            &["remote", "set-head", "origin", "--auto"],
            opts,
            deadline,
        );
    }
    Ok(())
}

/// Fetches `origin` (bounded by `deadline`) and says which commit a new branch should
/// start from. Only fetches and reads: no local branch is moved or created.
pub fn prepare_base(dir: &Path, label: &str, opts: &GitOptions, deadline: Deadline) -> RepoBase {
    let fetch = if !has_origin(dir, opts) {
        FetchOutcome::NoRemote
    } else {
        match locked_fetch(dir, opts, deadline) {
            Ok(()) => FetchOutcome::Fetched,
            Err(reason) => FetchOutcome::Failed(reason),
        }
    };
    let (refname, sha) = resolve_base(dir, opts).unwrap_or_else(|| ("HEAD".into(), String::new()));
    RepoBase {
        repo: label.to_string(),
        refname,
        sha,
        fetch,
    }
}
