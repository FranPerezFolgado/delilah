//! Workspaces: parent worktrees, selective submodule init and branches.

mod command;
mod workspace;

pub use command::{GitError, GitOptions};
pub use workspace::{
    Submodule, Workspace, WorkspaceSpec, archive_workspace, changed_paths, create_workspace,
    list_submodules, repo_root,
};
