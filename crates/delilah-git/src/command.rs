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
    #[error("{submodule} has uncommitted changes; commit or discard them before archiving")]
    Dirty { submodule: String },
    #[error("{submodule} has {count} commit(s) that are on no remote; push them before archiving")]
    Unpushed { submodule: String, count: u32 },
}

pub(crate) fn git(dir: &Path, args: &[&str], opts: &GitOptions) -> Result<String, GitError> {
    let mut cmd = Command::new("git");
    if opts.allow_file_protocol {
        cmd.args(["-c", "protocol.file.allow=always"]);
    }
    let output = cmd.arg("-C").arg(dir).args(args).output()?;
    if !output.status.success() {
        return Err(GitError::Command {
            dir: dir.to_path_buf(),
            args: args.join(" "),
            stderr: String::from_utf8_lossy(&output.stderr).trim().to_string(),
        });
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}
