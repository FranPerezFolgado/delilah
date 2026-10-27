## Spec compliance — Phase 6 (US4, WP-16)
Covered: FR-026 (actions.rs refuse_live_subtasks in archive and delete, force does not override; test a_parent_task_with_a_live_subtask_cannot_be_archived_or_deleted_even_forced; UI TaskView.tsx:100,130 + vitest; CLI ErrorBody test), FR-027 (pr_loop/merge.rs archive_merged: Done, workspace kept, parent-task-live notice once; test a_merged_parent_task_with_live_subtasks_is_done_but_not_archived), FR-028 (unchanged path), US4 sc.1-3, sc.4 refresh (Topic::Tasks on archive/delete, test deleting_a_subtask_tells_the_tasks_topic), delete clears parent_task links (test).
Missing: none.
Extra: rustfmt-only reformatting in delilah-cli/main.rs and an unrelated `let mut put` -> `let put` in merge.rs tests (harmless).
Deviates: T047 names TaskHeader.tsx; change landed in TaskView.tsx (same behaviour, minor).
Untested: dropped/leaves-count display (US4 sc.4) relies on earlier-phase progress code; not re-tested here. Archive of a parent with only archived subtasks is not tested explicitly (delete is).

VERDICT: ✅ COMPLIANT (no CRITICAL/HIGH findings)
