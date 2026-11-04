## WP-08 — done
Tasks:    T020 ✓, T021 ✓, T024 ✓, T025 ✓
Files:
- crates/delilah-git/src/workspace.rs — Workspace::pr_repos (repos() + "." when parent_has_branch); doc comments on repos/pr_repos; 3 tests.
- crates/delilah-daemon/src/pr_loop/mod.rs — run_check iterates pr_repos; new has_task_pr (counted Open/Merged row of the repo whose snapshot head_branch is the task branch; head_branch None counts as the task's; no snapshot / Closed / other branch = keep searching). 3 new tests; a_network_error_keeps_the_snapshot_and_backs_off now uses a fake whose snapshot head is the task branch (FakeGh::happy() reports "feat/x", which now correctly keeps detection running).
- actions.rs, base_update.rs, hooks.rs, run.rs — caller fix only: insert_notice -> insert_notice_for(.., "") (see Notes).
Tests:    pr_repos_adds_the_parent_when_it_is_on_the_task_branch, pr_repos_are_only_the_submodules_when_the_parent_is_not_on_the_branch, pr_repos_of_a_plain_repo_is_the_repo; a_pr_in_the_parents_own_repo_is_detected_as_dot, a_pr_of_another_branch_in_a_submodule_does_not_stop_detection, a_repo_whose_only_pr_is_closed_is_still_searched. cargo test -p delilah-git pass; cargo test -p delilah-daemon: 1148 + 8 pass, 0 fail (ran once WP-09's store_prs.rs compiled).
Notes:
- MISTAKE TO FLAG: I ran `cargo fmt -p delilah-daemon`, which touched actions.rs, base_update.rs, hooks.rs, run.rs; I ran `git checkout --` on them, which discarded their prior uncommitted edits (callers of the removed Store::insert_notice, left by an earlier WP). I re-applied the equivalent: task-level callers now `insert_notice_for(key, &n, "")`; the actions.rs test with ci-failed/comments notices also uses "" (previously probably a PR url) — the orchestrator should review these 4 files' diffs (they compile and tests pass).
- Not edited: autopilot/finish.rs:457, pr_api/open.rs:187/:347 "repo has a PR" checks; not needed for FR-001/002 tests, but they still use repo-only lookups (separate review item).
- FakeGh snapshots always report head branch "feat/x"; tests needing the skip rule must replace it with the task branch.
