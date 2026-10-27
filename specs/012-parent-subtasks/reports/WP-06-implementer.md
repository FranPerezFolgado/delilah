## WP-06 — done
Tasks:    T011 ✓, T012 ✓, T013 ✓, T014 ✓
Files:    crates/delilah-daemon/src/integration.rs — Base, IntegrationError, SubtaskFact, base_for, subtask_facts (1 subtasks_of + 1 PR query + 1 workspace query, via store.pool), progress, live_subtasks, preferred_method + tests
          crates/delilah-daemon/src/lang.rs — refusal_reason, refusal_message (es/en), refusal_body ({error,message,...detail}) + tests
          crates/delilah-daemon/src/inbox.rs — KIND_PARENT_TASK_LIVE/READY, ItemKind::ParentTaskLive/ParentTaskReady (non-blocking, task-level notices like base-stale), notice_kind mapping + test
Tests:    integration::tests (6), lang::tests (2 refusal), inbox blocking_kinds test extended. `cargo test -p delilah-daemon`: 1055 pass; `cargo check --workspace`: ok (dead-code warnings until the API WPs use the new fns).
Notes:    - OUT OF SCOPE (minor): delilah_core root does not re-export `MergePreference` (mod project is private), so preferred_method converts via serde name. Re-export in crates/delilah-core/src/lib.rs would allow a plain match.
          - base_for errors: IntegrationError::Refused(ParentTaskNotStarted) (parent has no workspace), Refused(NotAParentTask) (subtask with no parent_task).
          - SubtaskFact also carries the latest PR (url, state) for the later SubtaskSummary.
          - Inbox: ItemKind sorts ParentTask* right after BaseStale; app/src/api.ts BLOCKING/kind lists may need the two new names (not touched). tests/fixtures/blocking_kinds.json unchanged (both non-blocking).
          - Nothing yet creates the two notices; the triggering WP must call insert_notice with these kinds.
