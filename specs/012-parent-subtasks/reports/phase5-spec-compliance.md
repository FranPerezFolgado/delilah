## Spec compliance — phase 5 (US3 daemon, T034–T041; UI T042/T043 ignored)
Covered: FR-014 (open.rs open_with: draft forced unless progress.ready; empty_commit commits `chore: start <KEY> <title>` + push; tests a_parent_task_pr_is_a_draft_unless_progress_is_ready, empty_commit_starts_a_parent_task_pr_and_pushes, empty_commit_on_another_kind_is_refused), FR-016/FR-016a (base_update.rs RepoState::pending, subtask_commits; tests count/update/tracked-changes/second-merge-conflict undo), FR-019 (merge.rs confirm_live_subtasks, `live-subtasks` + keys; test), FR-020 (pr_ready only from POST .../ready, ready.rs; test asserts 0 calls before), FR-023 daemon side (pr_loop/merge.rs parent_ready, dedupe via INSERT OR IGNORE on task+kind+cause `pr <url>`; test ...ready_once), Topic::Tasks on subtask merge (on_subtask_merged). T034–T041 all implemented with tests.
Missing: none in daemon scope.
Extra: none.
Deviates: none.
Untested / minor (LOW):
- FR-014 "ready ⇒ user chooses" branch (parent with all subtasks merged and draft:false opens non-draft) has no test; only the forced-draft case is tested.
- Update-branch conflict in the FIRST ref (own branch) not tested; only second-ref conflict.
- FR-019 "naming how many": response carries keys list (count derivable); fine.

Findings: CRITICAL 0, HIGH 0, MEDIUM 0, LOW 3 (above).
VERDICT: ✅ COMPLIANT
