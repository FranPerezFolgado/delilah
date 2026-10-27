## WP-02 — done (workspace check blocked by one out-of-scope literal)
Tasks:    T002 ✓, T003 ✓, T004 ✓, T005 ✓
Files:    crates/delilah-core/src/hierarchy.rs — TaskKind, ChildFacts, Progress, progress, is_live, Refusal, 4 check_* + tests
          crates/delilah-core/src/task.rs — Task.kind / Task.parent_task (serde default, skipped when normal/None); Task::new sets them; old-JSON test
          crates/delilah-core/src/project.rs — MergePreference, ProjectSettings.parent_task_merge; old-settings test; two existing round-trip tests now expect "parent_task_merge":"merge"
Tests:    hierarchy::tests (progress x3, is_live, kind serde, 4 refusal suites), task old-JSON test, project old-settings test. `cargo test -p delilah-core`: 89 pass.
          `cargo check --workspace`: FAILS on crates/delilah-daemon/src/store.rs:684 (Task literal in task_from_row lacks `kind`, `parent_task`) — OUT OF SCOPE; the storage package must add them (T006/T007).
Notes:    Signature deviations from plan (needed by contract): check_new_subtask(parent, parent_archived, explicit_project: Option<&str>) for project-mismatch;
          check_attach(task, parent, parent_archived, started) for parent-task-archived.
          Refusal has all 13 contract reasons as unit variants (kebab-case serde), including daemon-decided ones (branch-missing, live-subtasks, no-commits, empty-commit-not-allowed).
          Choices: make-parent-task on a ParentTask is Ok (idempotent), on a Subtask Nesting; attach of a non-normal task is Nesting; subtask as "parent" is NotAParentTask. Project mismatch only when both projects are Some.
          Check order in make_parent_task: Nesting, InCheckout, HasOpenPr, HasSession.
