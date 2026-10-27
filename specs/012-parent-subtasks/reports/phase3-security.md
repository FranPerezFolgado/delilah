# Phase 3 security review (163c0ea..HEAD)

Verdict: no CRITICAL or HIGH findings.

Checked: push-branch and subtasks endpoints, create with parent_task/kind, valid_ref_name, merge_refs, prepare_base_on, push_branch.

- push-branch: takes only the task key; branch and path come from the stored workspace, not the request. Parent-task-only (404 otherwise). `check_branch` runs before `git push`. No project-boundary bypass.
- create with parent_task: the project is taken from the parent; a different `project` gives 409 project-mismatch. Parent kind, archived and nesting rules are checked before anything is allocated.
- valid_ref_name: rejects leading `-`, `..`, `@{`, control characters, whitespace and the `~^:?*[\` characters. Applied to the base branch in prepare_base_on and to every ref in merge_refs. merge_refs also passes `--`, so no option injection.

[LOW] crates/delilah-git/src/workspace.rs:769
Problem: push_branch relies on check_branch, a different validator from valid_ref_name. Not exploitable here.
Fix: optional, unify the validators.

[LOW] crates/delilah-daemon/src/api_hierarchy.rs:~88
Problem: push failures return the git error text (`e.to_string()`) in a 409, which may include local paths. The API is local-only.
Fix: optional, trim the message.
