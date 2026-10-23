//! Workspaces: parent worktrees, selective submodule init and branches.

mod changes;
mod command;
mod share;
mod workspace;

pub use changes::{ChangeKind, Changes, FileChange, base_commit, changes, file_diff, repo_files};
pub use command::{GitError, GitOptions};
pub use share::{ShareNote, share_entries};
pub use workspace::{
    Submodule, Workspace, WorkspaceSpec, archive_workspace, branch_taken, branch_tip,
    changed_paths, check_merged, check_repos, check_workspace, commit_subjects, create_workspace,
    delete_branch, fetch, free_branch, is_sha, leave_checkout, list_submodules, needs_push,
    push_branch, remove_workspace, repo_root, use_checkout,
};
