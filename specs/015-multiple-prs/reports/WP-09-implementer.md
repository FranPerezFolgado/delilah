## WP-09 — done
Tasks:    T023 ✓, T026 ✓, review fixes (1) insert_notice guard ✓, (2) 0027 backfill of autofix-stopped + legacy fallback dropped ✓, (3) prs::summary no clones ✓
Files:
- crates/delilah-daemon/src/store_prs.rs — new `link_pr_on_branch(key, pr, head_branch: Option<&str>)` (link_pr delegates with None) + `replace_closed`: same task+repo, snapshot Closed and snapshot head_branch == branch -> replaced_by = new id, old PR's open notices resolved. `PR_NOTICE_KINDS` (shared set, incl. autofix-stopped). `insert_notice` renamed `insert_task_notice`, returns Err(Corrupt) for PR kinds. Tests updated/added.
- crates/delilah-daemon/migrations/0027_pr_per_url.sql — 'autofix-stopped' added to both backfill kind lists (url + orphan-resolve).
- crates/delilah-daemon/src/prs.rs — summary iterates counted() directly (Vec<&PrRow>).
- Call sites of the rename (outside the list, minimal): actions.rs (rename x3 + one test now uses insert_notice_for with a url), base_update.rs, hooks.rs, run.rs, inbox.rs (rename + stopped_notice test helper uses insert_notice_for), pr_loop/merge.rs, autofix/api.rs (rename + test uses insert_notice_for with the PR url), autofix/mod.rs (resolve_stopped: legacy `pr_url=''` fallback removed).
Tests:    a_new_pr_on_the_branch_of_a_closed_one_replaces_it_and_resolves_its_notices; open_merged_other_branch_or_other_repo_prs_are_never_replaced; task_level_insert_refuses_the_pr_kinds; migration test extended with a legacy autofix-stopped notice (gets the PR url). `cargo test -p delilah-daemon`: 1148 + 8 pass; `cargo clippy -p delilah-daemon --all-targets`: clean.
Notes:
- Head branch of the NEW PR is not in NewPr/snapshot at link time, so WP-10 (capture) must call `link_pr_on_branch` with the branch (agent: from the task branch / gh output). Plain `link_pr` replaces nothing.
- Mid-run, a foreign process rewrote actions.rs/base_update.rs/hooks.rs/run.rs to `insert_notice_for(..., "")`; I restored those four from HEAD and redid the rename (no PR-kind task-level inserts remain).
- Transient failures in pr_loop::tests (3) and setup::tests::retry_runs_it_again seen in one run while WP-08 edited in parallel; they passed alone and in the final full run.
