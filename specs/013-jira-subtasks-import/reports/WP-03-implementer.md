## WP-03 — done
Tasks:    T005 ✓, T006 ✓, T007 ✓
Files:    crates/delilah-daemon/src/store.rs — free fns `set_kind_tx`, `become_parent_task` (pub(crate), generic Executor); `Store::begin_immediate`, `Store::tasks_by_keys`; `set_kind` delegates to `set_kind_tx`; test `store_primitives_for_the_jira_import`
          crates/delilah-daemon/src/api_hierarchy.rs — `make_parent_task` calls `store::become_parent_task` then re-reads the task
Tests:    store::tests::store_primitives_for_the_jira_import passes. `cargo test -p delilah-daemon`: 1104 passed, 2 failed (run::tests::a_free_task_with_a_template_starts_on_the_templates_branch, a_jira_task_keeps_its_key_in_the_branch): both in run.rs, outside my files; they get a Spanish branch slug ("nombres-rama" vs "task-branch-names"), a language/environment issue, unrelated.
Notes:    become_parent_task does the reset and the kind change in one UPDATE (also sets parent_task NULL, updated_at) and returns NotFound on 0 rows. It is a free function, not a Store method. `begin_immediate`/`tasks_by_keys` give dead-code warnings until WP-06 uses them. Make_parent_task's existing tests pass.
