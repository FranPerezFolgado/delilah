# Migration review: 0026_jira_subtask_skips.sql (329499b..HEAD)

Engine: SQLite via sqlx. The migration only creates a new table, so there is no rewrite, no lock on existing tables and no backfill.

[LOW] crates/delilah-daemon/migrations/0026_jira_subtask_skips.sql:1
Problem: The migration has no down script and no stated rollback. The project's migrations are forward-only, so this is consistent. Rollback is `DROP TABLE jira_subtask_skips`, and old code ignores the table.
Fix: None needed. Optionally note in the PR that the migration is additive.

[LOW] crates/delilah-daemon/migrations/0026_jira_subtask_skips.sql:5
Problem: There is no FK, so the spec says rows must be removed when the parent task is deleted. Nothing enforces that in the schema. That cleanup belongs to a later package, and the store stub (store/jira_skips.rs) is only 5 lines. Without it, orphan rows would build up and a re-created task with the same key would inherit old skips.
Fix: Make sure the task-delete path in store.rs calls the skip cleanup, and add a test for it.

Checks that passed:
- Ordering: 0026 follows 0025_task_hierarchy.sql, and no shipped migration was edited.
- Columns and PK match data-model.md exactly (parent_task, jira_key, created_at, all TEXT NOT NULL, PK (parent_task, jira_key)).
- The table is new and empty, so NOT NULL needs no default.
- Deploy order: old code ignores the new table, so rolling back the binary is safe.
- The `created_at` format (RFC 3339) must be written by the code. That is not yet implemented, so check it in the store package.
- Not verified: I did not run the migrations. There is no migration test in this diff. The existing sqlx test harness (opening a fresh store) should cover a fresh DB.

VERDICT: ✅ SAFE
