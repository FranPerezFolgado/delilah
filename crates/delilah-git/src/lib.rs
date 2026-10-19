//! Workspaces: parent worktrees, selective submodule init and branches.

mod changes;
mod command;
mod workspace;

pub use changes::{ChangeKind, Changes, FileChange, base_commit, changes, file_diff};
pub use command::{GitError, GitOptions};
pub use workspace::{
    Submodule, Workspace, WorkspaceSpec, archive_workspace, changed_paths, create_workspace,
    list_submodules, repo_root,
};
