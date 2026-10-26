use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Default)]
pub struct GitOptions {
    /// Allows submodules over `file://`. Only the local fixtures need it: git blocks it
    /// by default since 2.38 for security reasons.
    pub allow_file_protocol: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum GitError {
    #[error("`git {args}` failed in {dir}: {stderr}")]
    Command {
        dir: PathBuf,
        args: String,
        stderr: String,
    },
    #[error("could not run git: {0}")]
    Spawn(#[from] std::io::Error),
    #[error("`{0}` is not a submodule of the parent repo")]
    UnknownSubmodule(String),
    #[error("`{0}` is not a valid branch name")]
    InvalidBranch(String),
    #[error("the workspace folder already exists: {0}")]
    WorkspaceExists(PathBuf),
    #[error("{location} has uncommitted changes; commit or discard them before archiving")]
    Dirty { location: String },
    #[error("{location} has {count} commit(s) that are on no remote; push them before archiving")]
    Unpushed { location: String, count: u32 },
    #[error(
        "{location} has commits that are not in the merged PR; push or discard them before archiving"
    )]
    NotMerged { location: String },
    #[error("a `repo` project has no submodules; drop --submodules")]
    SubmodulesInRepo,
    #[error(
        "`{0}` is not initialised in your checkout: run `git submodule update --init {0}` or use a worktree"
    )]
    NotInitialised(String),
    #[error("`git {args}` in {dir} took too long")]
    Timeout { dir: PathBuf, args: String },
}

/// How long the network part of a task's start (or of a merge check) may take, all
/// repos together.
pub const NETWORK_LIMIT: Duration = Duration::from_secs(30);

/// A point in time by which a series of git calls must be done.
#[derive(Debug, Clone, Copy)]
pub struct Deadline(Instant);

impl Deadline {
    pub fn after(d: Duration) -> Self {
        let now = Instant::now();
        // A duration too large to add means "no limit worth counting": a year will do.
        Self(
            now.checked_add(d)
                .unwrap_or_else(|| now + Duration::from_secs(365 * 24 * 3600)),
        )
    }

    /// Time left; zero once past.
    pub fn remaining(&self) -> Duration {
        self.0.saturating_duration_since(Instant::now())
    }

    pub fn expired(&self) -> bool {
        self.remaining().is_zero()
    }
}

/// Every git call goes through here: no hook or fsmonitor command of the repo runs
/// (agents write those repos), and nothing asks for input: no terminal prompt, no
/// askpass dialog (git's or ssh's), no interactive credential helper. Credentials that
/// need no one (a keychain helper, an ssh agent) still work.
fn command(dir: &Path, args: &[&str], opts: &GitOptions) -> Command {
    let mut cmd = Command::new("git");
    cmd.args([
        "-c",
        "core.hooksPath=/dev/null",
        "-c",
        "core.fsmonitor=false",
        "-c",
        "credential.interactive=never",
    ]);
    if opts.allow_file_protocol {
        cmd.args(["-c", "protocol.file.allow=always"]);
    }
    cmd.arg("-C").arg(dir).args(args);
    cmd.env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_ASKPASS", "true")
        .env("SSH_ASKPASS_REQUIRE", "never")
        .stdin(Stdio::null());
    cmd
}

/// Git's output as a result: stdout on success, stderr in the error otherwise.
fn checked(
    dir: &Path,
    args: &[&str],
    status: std::process::ExitStatus,
    stdout: &[u8],
    stderr: &[u8],
) -> Result<String, GitError> {
    if !status.success() {
        return Err(GitError::Command {
            dir: dir.to_path_buf(),
            args: args.join(" "),
            stderr: String::from_utf8_lossy(stderr).trim().to_string(),
        });
    }
    Ok(String::from_utf8_lossy(stdout).trim().to_string())
}

/// Runs git to the end, however long it takes. For calls with a time limit, see
/// [`git_with_timeout`].
pub(crate) fn git(dir: &Path, args: &[&str], opts: &GitOptions) -> Result<String, GitError> {
    let output = command(dir, args, opts).output()?;
    checked(dir, args, output.status, &output.stdout, &output.stderr)
}

/// Like [`git`], but gives up at `deadline`, killing git and what it spawned.
pub(crate) fn git_with_timeout(
    dir: &Path,
    args: &[&str],
    opts: &GitOptions,
    deadline: Deadline,
) -> Result<String, GitError> {
    let timeout = || GitError::Timeout {
        dir: dir.to_path_buf(),
        args: args.join(" "),
    };
    if deadline.expired() {
        return Err(timeout());
    }
    let mut cmd = command(dir, args, opts);
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
    #[cfg(unix)]
    {
        // Own process group, so a timeout can take ssh and friends down with git.
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
    }
    let mut child = cmd.spawn()?;
    // Drain the pipes meanwhile, so a chatty git never blocks on a full one. Each
    // reader hands its bytes over a channel, so a descendant that keeps a pipe open
    // (ssh ControlMaster, a `setsid` child) can't make us wait past the deadline.
    let out = drain(child.stdout.take());
    let err = drain(child.stderr.take());
    // Polls often at first, so a quick local read costs little more than with `git`.
    let mut pause = Duration::from_millis(1);
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if !deadline.expired() => {
                std::thread::sleep(pause.min(deadline.remaining()));
                pause = (pause * 2).min(Duration::from_millis(50));
            }
            waited => {
                kill_group(&mut child);
                waited?;
                return Err(timeout());
            }
        }
    };
    // Git is gone; whatever it wrote is already in the pipes.
    let wait_for = deadline.remaining().max(PIPE_GRACE);
    let (Ok(stdout), Ok(stderr)) = (out.recv_timeout(wait_for), err.recv_timeout(wait_for)) else {
        return Err(timeout());
    };
    checked(dir, args, status, &stdout, &stderr)
}

/// How long to wait for a pipe's end after git has exited, when the deadline has passed.
const PIPE_GRACE: Duration = Duration::from_secs(1);

/// Reads `pipe` to its end on a thread of its own; the bytes arrive on the channel.
fn drain<R: std::io::Read + Send + 'static>(pipe: Option<R>) -> mpsc::Receiver<Vec<u8>> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let mut buf = Vec::new();
        if let Some(mut p) = pipe {
            let _ = p.read_to_end(&mut buf);
        }
        let _ = tx.send(buf);
    });
    rx
}

/// Kills git and its process group (ssh and friends), and reaps git. The group is
/// signalled before the reap, while git's pid still names it.
fn kill_group(child: &mut std::process::Child) {
    #[cfg(unix)]
    let _ = Command::new("kill")
        .args(["-KILL", "--", &format!("-{}", child.id())])
        .output();
    let _ = child.kill();
    let _ = child.wait();
}

/// Like [`git`], but returns the exit code instead of failing on a non-zero one
/// (a signal-killed process reads as -1). Fails only if git could not run.
pub(crate) fn git_code(dir: &Path, args: &[&str], opts: &GitOptions) -> Result<i32, GitError> {
    let status = command(dir, args, opts).output()?.status;
    Ok(status.code().unwrap_or(-1))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_repo() -> tempfile::TempDir {
        let dir = tempfile::TempDir::new().unwrap();
        git(dir.path(), &["init", "-q"], &GitOptions::default()).unwrap();
        dir
    }

    #[test]
    fn deadline_saturates_at_zero() {
        let d = Deadline::after(Duration::ZERO);
        std::thread::sleep(Duration::from_millis(5));
        assert_eq!(d.remaining(), Duration::ZERO);
        assert!(d.expired());
        assert!(!Deadline::after(Duration::from_secs(5)).expired());
    }

    #[test]
    fn fast_command_returns_like_git() {
        let dir = temp_repo();
        let o = GitOptions::default();
        let a = git(dir.path(), &["rev-parse", "--show-toplevel"], &o).unwrap();
        let b = git_with_timeout(
            dir.path(),
            &["rev-parse", "--show-toplevel"],
            &o,
            Deadline::after(Duration::from_secs(10)),
        )
        .unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn hanging_fetch_times_out() {
        let dir = temp_repo();
        let o = GitOptions::default();
        git(
            dir.path(),
            &["remote", "add", "origin", "ssh://example.invalid/x"],
            &o,
        )
        .unwrap();
        git(dir.path(), &["config", "core.sshCommand", "sleep 30 #"], &o).unwrap();
        let start = Instant::now();
        let err = git_with_timeout(
            dir.path(),
            &["fetch", "origin"],
            &o,
            Deadline::after(Duration::from_secs(1)),
        )
        .unwrap_err();
        assert!(matches!(err, GitError::Timeout { .. }), "{err}");
        assert!(start.elapsed() < Duration::from_secs(5));
    }

    #[test]
    fn a_timeout_leaves_no_child_behind() {
        let dir = temp_repo();
        let o = GitOptions::default();
        git(
            dir.path(),
            &["remote", "add", "origin", "ssh://example.invalid/x"],
            &o,
        )
        .unwrap();
        // A sleep only this test starts, to look for it afterwards.
        git(
            dir.path(),
            &["config", "core.sshCommand", "sleep 41.37 #"],
            &o,
        )
        .unwrap();
        let err = git_with_timeout(
            dir.path(),
            &["fetch", "origin"],
            &o,
            Deadline::after(Duration::from_secs(1)),
        )
        .unwrap_err();
        assert!(matches!(err, GitError::Timeout { .. }), "{err}");
        // SIGKILL is sent, not awaited: give the orphan a moment to go.
        let gone = (0..20).any(|_| {
            std::thread::sleep(Duration::from_millis(100));
            !Command::new("pgrep")
                .args(["-f", "sleep 41.37"])
                .status()
                .unwrap()
                .success()
        });
        assert!(gone, "a child of git outlived the timeout");
    }

    #[test]
    fn expired_deadline_does_not_spawn() {
        let dir = temp_repo();
        let d = Deadline::after(Duration::ZERO);
        let err = git_with_timeout(dir.path(), &["status"], &GitOptions::default(), d).unwrap_err();
        assert!(matches!(err, GitError::Timeout { .. }));
    }

    #[test]
    fn a_descendant_holding_the_pipes_does_not_outlast_the_deadline() {
        // ssh fails at once but leaves a child of its own on git's stderr.
        let dir = temp_repo();
        let o = GitOptions::default();
        git(
            dir.path(),
            &["remote", "add", "origin", "ssh://example.invalid/x"],
            &o,
        )
        .unwrap();
        git(
            dir.path(),
            &["config", "core.sshCommand", "sh -c 'sleep 30 & exit 1' #"],
            &o,
        )
        .unwrap();
        let start = Instant::now();
        let err = git_with_timeout(
            dir.path(),
            &["fetch", "origin"],
            &o,
            Deadline::after(Duration::from_secs(1)),
        )
        .unwrap_err();
        assert!(start.elapsed() < Duration::from_secs(6), "{err}");
    }

    #[test]
    fn a_huge_deadline_does_not_overflow() {
        assert!(!Deadline::after(Duration::MAX).expired());
    }

    #[test]
    fn credentials_are_never_asked_for() {
        let dir = temp_repo();
        let o = GitOptions::default();
        let out = command(dir.path(), &["config", "credential.interactive"], &o)
            .output()
            .unwrap();
        assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "never");
        let cmd = command(dir.path(), &["status"], &o);
        let envs: Vec<_> = cmd.get_envs().collect();
        for (k, v) in [
            ("GIT_TERMINAL_PROMPT", "0"),
            ("GIT_ASKPASS", "true"),
            ("SSH_ASKPASS_REQUIRE", "never"),
        ] {
            assert!(
                envs.contains(&(k.as_ref(), Some(v.as_ref()))),
                "{k} in {envs:?}"
            );
        }
    }
}
