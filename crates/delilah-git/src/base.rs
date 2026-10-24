//! The commit a new task branch starts from: `origin`'s default branch, freshly fetched.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use crate::command::{Deadline, GitError, GitOptions, git_with_timeout};

/// A fetch that finished less ago than this is reused by the next caller.
const FRESH_FOR: Duration = Duration::from_secs(5);

/// The limit of each local read (no network). Its own, not the start's deadline: a
/// fetch that used that up must still find the last known `origin`.
fn local() -> Deadline {
    Deadline::after(Duration::from_secs(5))
}

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
    /// `None` when no commit could be resolved (an empty repo); `refname` is then `HEAD`.
    pub sha: Option<String>,
    pub fetch: FetchOutcome,
}

impl RepoBase {
    /// What a new branch starts at: the commit, else the ref.
    pub fn start(&self) -> &str {
        self.sha.as_deref().unwrap_or(&self.refname)
    }
}

/// When the repo last fetched, and how that went.
type RepoLock = Arc<Mutex<Option<(Instant, Result<(), String>)>>>;

/// One lock per repo, process-wide; the value is its last fetch. Keyed on the
/// repo's common git dir, so a worktree and its main checkout (which share `origin`'s
/// refs) share it too. The map holds one small entry per repo ever fetched.
fn repo_lock(dir: &Path, opts: &GitOptions) -> RepoLock {
    static FETCHING: OnceLock<Mutex<HashMap<PathBuf, RepoLock>>> = OnceLock::new();
    let common = git_with_timeout(
        dir,
        &["rev-parse", "--path-format=absolute", "--git-common-dir"],
        opts,
        local(),
    )
    .map(PathBuf::from)
    .unwrap_or_else(|_| dir.to_path_buf());
    let key = common.canonicalize().unwrap_or(common);
    let mut map = FETCHING
        .get_or_init(Default::default)
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    map.entry(key).or_default().clone()
}

fn has_origin(dir: &Path, opts: &GitOptions) -> bool {
    git_with_timeout(dir, &["remote", "get-url", "origin"], opts, local()).is_ok()
}

fn verify(dir: &Path, refname: &str, opts: &GitOptions) -> Option<String> {
    git_with_timeout(
        dir,
        &[
            "rev-parse",
            "--verify",
            "--quiet",
            &format!("{refname}^{{commit}}"),
        ],
        opts,
        local(),
    )
    .ok()
    .filter(|s| !s.is_empty())
}

/// The first usable of `origin/HEAD` (by the name it points to), `origin/main`,
/// `origin/master`, `main`, `master` and `HEAD`.
fn resolve_base(dir: &Path, opts: &GitOptions) -> Option<(String, String)> {
    let head = git_with_timeout(
        dir,
        &["symbolic-ref", "--short", "refs/remotes/origin/HEAD"],
        opts,
        local(),
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

/// Fetches `origin` under the repo's lock, bounded by `deadline` (waiting for the lock
/// included). A fetch that finished less than `reuse` ago is not repeated: its result
/// is returned, failure included, so starts queued on an unreachable remote don't
/// each wait for it again. The error is git's stderr, or why it did not run.
pub(crate) fn locked_fetch(
    dir: &Path,
    opts: &GitOptions,
    deadline: Deadline,
    reuse: Duration,
) -> Result<(), String> {
    let lock = repo_lock(dir, opts);
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
    if let Some((at, result)) = last.as_ref()
        && at.elapsed() < reuse
    {
        return result.clone();
    }
    let result = git_with_timeout(
        dir,
        &["fetch", "--quiet", "--no-tags", "origin"],
        opts,
        deadline,
    )
    .map(|_| ())
    .map_err(|e| match e {
        GitError::Command { stderr, .. } if !stderr.is_empty() => stderr,
        e => e.to_string(),
    });
    *last = Some((Instant::now(), result.clone()));
    result?;
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
        match locked_fetch(dir, opts, deadline, FRESH_FOR) {
            Ok(()) => FetchOutcome::Fetched,
            Err(reason) => FetchOutcome::Failed(reason),
        }
    };
    let (refname, sha) = match resolve_base(dir, opts) {
        Some((refname, sha)) => (refname, Some(sha)),
        None => ("HEAD".into(), None),
    };
    RepoBase {
        repo: label.to_string(),
        refname,
        sha,
        fetch,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command::git;

    #[test]
    fn a_worktree_shares_its_main_checkout_lock() {
        let tmp = tempfile::TempDir::new().unwrap();
        let main = tmp.path().join("main");
        let o = GitOptions::default();
        std::fs::create_dir(&main).unwrap();
        git(&main, &["init", "-q", "-b", "main"], &o).unwrap();
        git(
            &main,
            &[
                "-c",
                "user.name=t",
                "-c",
                "user.email=t@t",
                "commit",
                "-q",
                "--allow-empty",
                "-m",
                "x",
            ],
            &o,
        )
        .unwrap();
        let wt = tmp.path().join("wt");
        git(
            &main,
            &["worktree", "add", "-q", "-b", "b", wt.to_str().unwrap()],
            &o,
        )
        .unwrap();
        let other = tmp.path().join("other");
        std::fs::create_dir(&other).unwrap();
        git(&other, &["init", "-q"], &o).unwrap();

        assert!(Arc::ptr_eq(&repo_lock(&main, &o), &repo_lock(&wt, &o)));
        assert!(!Arc::ptr_eq(&repo_lock(&main, &o), &repo_lock(&other, &o)));
    }

    #[test]
    fn a_hung_fetch_gives_up_at_the_deadline_and_releases_the_lock() {
        let tmp = tempfile::TempDir::new().unwrap();
        let o = GitOptions::default();
        git(tmp.path(), &["init", "-q"], &o).unwrap();
        git(
            tmp.path(),
            &["remote", "add", "origin", "ssh://example.invalid/x"],
            &o,
        )
        .unwrap();
        git(tmp.path(), &["config", "core.sshCommand", "sleep 30 #"], &o).unwrap();

        let start = Instant::now();
        let err = locked_fetch(
            tmp.path(),
            &o,
            Deadline::after(Duration::from_secs(1)),
            Duration::ZERO,
        )
        .unwrap_err();
        assert!(start.elapsed() < Duration::from_secs(5), "{err}");
        assert!(repo_lock(tmp.path(), &o).try_lock().is_ok());
    }

    fn repo_with_origin(origin: &str) -> tempfile::TempDir {
        let tmp = tempfile::TempDir::new().unwrap();
        let o = GitOptions::default();
        git(tmp.path(), &["init", "-q"], &o).unwrap();
        git(tmp.path(), &["remote", "add", "origin", origin], &o).unwrap();
        tmp
    }

    #[test]
    fn a_fetch_is_reused_within_the_window_failure_included() {
        let o = GitOptions::default();
        let remote = tempfile::TempDir::new().unwrap();
        git(remote.path(), &["init", "-q", "--bare"], &o).unwrap();
        let repo = repo_with_origin(remote.path().to_str().unwrap());
        let dl = || Deadline::after(Duration::from_secs(10));

        locked_fetch(repo.path(), &o, dl(), FRESH_FOR).unwrap();
        git(
            repo.path(),
            &["remote", "set-url", "origin", "/nonexistent"],
            &o,
        )
        .unwrap();
        // Reused: the broken URL is not tried yet.
        locked_fetch(repo.path(), &o, dl(), FRESH_FOR).unwrap();
        // No reuse: it is.
        let err = locked_fetch(repo.path(), &o, dl(), Duration::ZERO).unwrap_err();
        // And that failure is what the next caller in the window gets.
        assert_eq!(locked_fetch(repo.path(), &o, dl(), FRESH_FOR), Err(err));
    }

    #[test]
    fn waiting_for_the_lock_gives_up_at_the_deadline() {
        let o = GitOptions::default();
        let repo = repo_with_origin("ssh://example.invalid/x");
        git(
            repo.path(),
            &["config", "core.sshCommand", "sleep 30 #"],
            &o,
        )
        .unwrap();
        let dir = repo.path().to_path_buf();
        let o2 = o.clone();
        let holder = std::thread::spawn(move || {
            locked_fetch(
                &dir,
                &o2,
                Deadline::after(Duration::from_secs(3)),
                Duration::ZERO,
            )
        });
        std::thread::sleep(Duration::from_millis(500));

        let err = locked_fetch(
            repo.path(),
            &o,
            Deadline::after(Duration::from_millis(500)),
            FRESH_FOR,
        )
        .unwrap_err();
        assert!(err.contains("still running"), "{err}");
        assert!(holder.join().unwrap().is_err());
    }
}
