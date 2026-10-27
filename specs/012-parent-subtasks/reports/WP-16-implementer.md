## WP-16 — done except the UI part (OUT OF SCOPE)
Tasks:    T044 ✓, T045 ✓, T046 ✓, T047 partial (CLI ✓; UI archive needs TaskView.tsx, see Notes)
Files:
- crates/delilah-daemon/src/actions.rs — `refuse_live_subtasks` (409 live-subtasks, force does not override) in archive and delete; delete of a parent task calls `clear_parent_task_links`; tests
- crates/delilah-daemon/src/pr_loop/merge.rs — `archive_merged`: a parent task with live subtasks stays Done, workspace kept, `parent-task-live` notice (deduped); test
- crates/delilah-cli/src/main.rs — test only: the refusal body prints its message naming the subtasks (CLI already printed `message`; archive/rm show it unchanged)
Tests:    actions: a_parent_task_with_a_live_subtask_cannot_be_archived_or_deleted_even_forced, deleting_a_parent_task_with_only_archived_subtasks_clears_their_link, deleting_a_subtask_tells_the_tasks_topic; merge: a_merged_parent_task_with_live_subtasks_is_done_but_not_archived; cli: a_live_subtasks_refusal_prints_its_message_naming_them. cargo test -p delilah-daemon (1089 ok), cargo test -p delilah-cli (45 ok), cargo check --workspace ok. Web checks not run (no web change).
Notes:
- Archive/delete of a subtask already emit Topic::Tasks (global topic), so the parent's progress refreshes; only a test was added.
- The live check in archive runs before the can_archive status check.
- OUT OF SCOPE: app/src/components/TaskView.tsx (archive/delete actions live there, not in TaskHeader.tsx). In `archive`, a 409 with !force calls setKeeps(e.message) and offers "force archive", which is wrong for live-subtasks. Change needed: `if (e instanceof ApiError && e.status === 409 && !force && e.reason !== "live-subtasks") setKeeps(e.message); else throw e;` (the thrown error then shows via humanError, naming the subtasks). Delete already shows the message via setError(humanError(e)). Same check may be needed in InboxView.tsx archive.

## Follow-up (UI, now in scope)
- app/src/components/TaskView.tsx: archive and delete show `e.message` (names the subtasks) for reason `live-subtasks`; no "force archive" offer.
- InboxView.tsx: no change needed (its archive is already forced and errors surface through its own act handler).
- TaskView.test.tsx: "shows a live-subtasks refusal naming them, with no force offer". tsc clean; vitest related TaskView: 46 pass. node_modules symlink removed.
