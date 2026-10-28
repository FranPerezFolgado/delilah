## WP-07 — done
Tasks:    T015 ✓, T016 ✓, T017 ✓, T020 ✓, T021 ✓, T022 ✓, T023 ✓
Files:
- crates/delilah-daemon/src/jira_import.rs — `preview`, `apply_jira_import`, `get_preview`/`post_import` handlers, `ImportRequest`/`ImportResult`/`PreviewBody`; `#![allow(dead_code)]` removed; `plan_import` gets a trailing `project: Option<&str>` (the hypothetical parent takes it when the ticket has no task); handler/planner tests.
- crates/delilah-daemon/src/jira.rs — `write_ticket` writes `- Parent: KEY title` / `- Padre: ...` (key only when no summary), after the epic line; test; `client()` made `pub(crate)`.
- crates/delilah-daemon/src/api.rs — route `GET`/`POST /api/v1/jira/issues/{key}/import` next to the subtasks route.
- crates/delilah-daemon/src/store.rs — only `insert_task` made `pub(crate)`.
- crates/delilah-daemon/tests/parent_tasks.rs — `an_imported_subtask_takes_the_parent_task_submodules` (T017).
Tests:    jira_import: the_preview_lists_the_fixture_subtasks, the_preview_answers_for_bad_keys_and_missing_jira, the_import_creates_the_parent_task_and_the_chosen_subtasks, bad_requests_write_nothing, a_normal_ticket_task_converts_and_a_backlog_task_attaches, a_refused_parent_is_a_409_and_a_stale_plan_writes_nothing, a_started_or_taken_task_is_left_out_not_failed, the_chosen_project_decides_whether_a_task_attaches; jira: the_ticket_names_its_parent_when_it_is_a_subtask; integration as above. `cargo test -p delilah-daemon`: 1116 + 8 pass, no flakiness seen. Clippy: only pre-existing warnings.
Notes:
- Error bodies for the new 400/409s are `{ error: <code>, message }` (via `ApiError.refusal`, like the hierarchy refusals): `bad-key`, `project-required`, `not-a-subtask`, `stale`, and the planner's refusal codes (`nesting`, `open-pr`, `running-session`, `archived`, ...). Jira not-connected/not-found/unreachable on POST gives 409 with `reason`.
- `GET` accepts an optional `?project=` so the dialog can preview attach/conflicts for the project it chose (review note 1); POST uses the body's project. The `is_issue_key` check is a local copy (the integrations one is private).
- The transaction re-checks with raw SQL on the tx connection (`live()`), because the in-memory test pool has one connection and pool reads inside the tx would deadlock. A key in both `add` and `skip` is skipped, not added.
- `left_out` reasons: `taken`, `started`, `other-project`, `in-checkout`, `linked`, or the planner's conflict reason. Convert/Existing/Create no longer valid in the tx -> 409, rolled back.
- `Topic::Tasks` is emitted once after commit, only if something was created/converted/attached.
- Subtasks of an existing parent task go through `resolve_new_task` with `parent_task` set (so `check_new_subtask` runs); for create/convert they use the normal resolve and override kind/parent.
- Attach also sets the task's project to the parent task's (as `/attach` does).
