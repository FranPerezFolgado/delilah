# Phase 2 migration review (0025_task_hierarchy.sql + store.rs)

Engine: SQLite via sqlx. Range 0ecfef3..HEAD.

Verified:
- Ordering: 0025 follows 0024; no shipped migration edited.
- Existing data: `kind TEXT NOT NULL DEFAULT 'normal'` is a constant default, so SQLite ADD COLUMN is metadata-only (no table rewrite) and existing rows read as 'normal'. `parent_task` is nullable. Index creation is cheap at this table size.
- Code consistency: all three SELECTs that feed `task_from_row` (get_task, list_tasks, the BEGIN IMMEDIATE one) and `insert_task` include kind/parent_task; bind order and placeholder count (24) match. Other `FROM tasks` queries select named columns only.
- Deploy order: old code ignores the new columns (INSERT omits them, default applies). Rollback of the binary is safe, except that an old binary does not know subtask rows.
- Test: `migration_0025_makes_existing_tasks_normal` runs 0001..0024, inserts a row, then upgrades. Fresh DB is covered by `in_memory`.

[LOW] crates/delilah-daemon/migrations/0025_task_hierarchy.sql:5
Problem: `parent_task` has no FOREIGN KEY or CHECK on `kind`. This looks intentional (clear_parent_task_links handles reused keys, archived subtasks outlive parents), and an unknown kind is surfaced as StoreError::Corrupt on read, which makes one bad row fail the whole `list_tasks`.
Fix: Optionally add `CHECK (kind IN ('normal','parent_task','subtask'))` at the ADD COLUMN, or make the row mapper degrade gracefully instead of failing the list.

[LOW] crates/delilah-daemon/src/store.rs:~496
Problem: The doc comment of `delete_task` ("Removes a task for good...") now sits above `subtasks_of`, because the new methods were inserted between the comment and its function.
Fix: Move the new methods above that comment.

[LOW] crates/delilah-daemon/src/store.rs (subtasks_of)
Problem: It loads every task, then filters in Rust, so the new `tasks_parent_task` index is unused by it.
Fix: Add a `WHERE t.parent_task = ?` query if the task count grows.

[LOW] crates/delilah-daemon/src/store.rs (set_kind, clear_parent_task_links)
Problem: No transaction around related updates. The callers' multi-step flows (e.g. parent delete, then clear links) are not atomic here, and there is no down migration (the project convention is forward-only, so this is fine).
Fix: None required. Make sure integration.rs calls these in a sensible order.

No CRITICAL or HIGH findings.

VERDICT: ✅ SAFE
