## WP-06 — done
Tasks:    T016 ✓, T017 ✓, shim cleanup ✓
Files:
- pr_api/dto.rs — PrView adds replaced_by, head_branch (from snapshot); test each_pr_carries_replaced_by_and_head_branch_and_comes_ordered.
- pr_api/mod.rs — task_prs sorts rows with prs::sort_for_display.
- prs.rs — sort_for_display (`.`, submodules, outside repos `host/owner/name`, then id); summary works over counted PRs (count = counted; only replaced = None). Tests: summary_counts_only_prs_that_are_not_replaced, prs_are_ordered_by_repo_then_id.
- pr_loop/merge.rs — all_merged and the release heads map use counted PRs (a closed PR replaced by a merged one no longer blocks archive); test a_closed_pr_replaced_by_a_merged_one_does_not_block_all_merged.
- store_prs.rs — removed the insert_notice inference and PR_KINDS: insert_notice now = task-level (pr_url ''), PR notices use insert_notice_for. Tests moved to insert_notice_for.
- autofix/mod.rs — removed unused `retry` wrapper; test uses retry_pr; clippy slice::from_ref fixes in WP-05 tests.
- inbox.rs, pr_api/forward.rs — test helpers use insert_notice_for for PR kinds.
- specs/015-multiple-prs/contracts/api.md — documents InboxItem.pr_url.
Tests:    cargo test -p delilah-daemon: 1142 pass + 8 integration, 0 fail (the earlier autopilot finish failure passes). cargo clippy -p delilah-daemon --all-targets: clean.
Notes:
- Sweep result: remaining repo lookups are deliberate: autofix retry_pr (repo alone = first counted PR, per contract), pr_loop/mod.rs:203 has_row (detection, WP-08), store.rs ~1447 and jira_import ~1841 are single-row test fixtures that stay valid (left as is).
- resolve_notices_of (repo-wide) kept: used for task-level base-stale; cancel() repo-wide in autofix kept (still used).
- app/ untouched.
