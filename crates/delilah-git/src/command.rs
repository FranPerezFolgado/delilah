use std::path::{Path, PathBuf};
use std::process::Command;

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
}

fn command(dir: &Path, args: &[&str], opts: &GitOptions) -> Command {
    let mut cmd = Command::new("git");
    if opts.allow_file_protocol {
        cmd.args(["-c", "protocol.file.allow=always"]);
    }
    cmd.arg("-C").arg(dir).args(args);
    cmd
}

pub(crate) fn git(dir: &Path, args: &[&str], opts: &GitOptions) -> Result<String, GitError> {
    let output = command(dir, args, opts).output()?;
    if !output.status.success() {
        return Err(GitError::Command {
            dir: dir.to_path_buf(),
            args: args.join(" "),
            stderr: String::from_utf8_lossy(&output.stderr).trim().to_string(),
        });
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// Like [`git`], but returns the exit code instead of failing on a non-zero one
/// (a signal-killed process reads as -1). Fails only if git could not run.
pub(crate) fn git_code(dir: &Path, args: &[&str], opts: &GitOptions) -> Result<i32, GitError> {
    let status = command(dir, args, opts).output()?.status;
    Ok(status.code().unwrap_or(-1))
}
