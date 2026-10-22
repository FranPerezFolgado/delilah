//! Workspaces: parent worktrees, selective submodule init and branches.

mod changes;
mod command;
mod share;
mod workspace;

pub use changes::{ChangeKind, Changes, FileChange, base_commit, changes, file_diff, repo_files};
pub use command::{GitError, GitOptions};
pub use share::{ShareNote, share_entries};
pub use workspace::{
    Submodule, Workspace, WorkspaceSpec, archive_workspace, changed_paths, check_workspace,
    create_workspace, leave_checkout, list_submodules, remove_workspace, repo_root, use_checkout,
};
