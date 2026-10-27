## WP-11 — done (one OUT OF SCOPE item)
Tasks:    T028 ✓, T029 ✓, T030 ✓, T031 ✓, T032 ✓ (list endpoint wiring: see Notes)
Files:
- crates/delilah-daemon/src/pr_api/open.rs — `base_of(app, key, dir)` now goes through `integration::base_for`; used by proposal and `create_and_link`; subtask with no `origin/<parent branch>` => refusal `parent-task-branch-missing`. Tests added.
- crates/delilah-daemon/src/base_update.rs — subtask base = `origin/<parent branch>` in `inspect`; merge via `delilah_git::merge_refs`; on failure every repo already merged is reset to its start; conflict files from `git merge-tree`; update refuses `parent-task-branch-missing`. Tests added.
- crates/delilah-daemon/src/pr_api/merge.rs — default method from `integration::preferred_method` (fallback `allowed[0]`). Tests added.
- crates/delilah-daemon/src/pr_api/dto.rs — PrView gains `method`, `base_branch`, `expected_base`; new `TaskContext` (kind, settings, expected base) and `PrView::with(row, ctx)`.
- crates/delilah-daemon/src/integration.rs — `api_error` helper (IntegrationError -> ApiError); `preferred_method` converts MergePreference explicitly.
- crates/delilah-core/src/lib.rs — re-export `MergePreference`.
Tests:    open: a_subtasks_pr_targets_the_parent_task_branch_on_every_path, opening_a_subtask_pr_needs_the_parent_task_branch_on_the_remote; base_update: a_subtask_is_behind_its_parent_task_branch_not_main, update_brings_in_the_parent_task_branch_only, a_conflict_with_the_parent_task_branch_leaves_the_repo_as_it_was, a_missing_parent_task_branch_is_refused_on_update; merge: a_subtask_merges_by_squash..., a_subtask_falls_back_to_the_first_allowed_method_when_squash_is_not, the_pr_view_carries_the_method_and_the_bases. `cargo test -p delilah-daemon` pass (1072 + integration); `cargo check --workspace` pass.
Notes:
- OUT OF SCOPE: crates/delilah-daemon/src/pr_api/mod.rs lines ~123 (list) and ~174 (link) build `PrView::from(&row)`, so `GET /tasks/{key}/prs` and the link response give `method: null`, `expected_base: null` (base_branch is filled). Change needed: `let ctx = dto::TaskContext::of(&app, &key).await?;` then `PrView::with(r, &ctx)` in both places (merge and open responses already do it).
- base-status already had `base`; for a subtask it is `origin/<parent branch>` (null when the remote lacks it; update then refuses).
- Conflict message in update now names files via `git merge-tree --write-tree` (git >= 2.38), since merge_refs aborts the merge itself.
- The multi-repo rollback (parent with submodules) is implemented but only covered by the single-repo conflict test.
- Did not move integration.rs raw SQL into Store (optional).
