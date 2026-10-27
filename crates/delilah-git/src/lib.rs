//! Workspaces: parent worktrees, selective submodule init and branches.

mod base;
mod changes;
mod command;
mod share;
mod workspace;

pub use base::{FetchOutcome, RepoBase, prepare_base, prepare_base_on};
pub use changes::{ChangeKind, Changes, FileChange, base_commit, changes, file_diff, repo_files};
pub use command::{Deadline, GitError, GitOptions, NETWORK_LIMIT};
pub use share::{ShareNote, share_entries};
pub use workspace::{
    Submodule, Workspace, WorkspaceSpec, apply_branch, archive_workspace, branch_taken, branch_tip,
    changed_paths, check_merged, check_repos, check_workspace, commit_empty, commit_subjects,
    create_workspace, current_branch, delete_branch, fetch, free_branch, is_sha, leave_checkout,
    list_submodules, merge_refs, needs_push, push_branch, remove_workspace, repo_root, spec_dirs,
    switch_branch, use_checkout,
};
