## WP-04 — Jira skips store (T008–T010)

**Tasks**:  
T008 — Write tests for `add_jira_skips`, `remove_jira_skips`, `jira_skips_of`, `delete_jira_skips`  
T009 — Implement the four functions using executor-taking writes (`INSERT OR IGNORE`)  
T010 — Call `delete_jira_skips` in actions.rs on parent task deletion; test delete vs. archive

**Files** (in order of work):

- `crates/delilah-daemon/src/store/jira_skips.rs:1–5` — empty module with `impl Store {}` stub (T008, T009)
- `crates/delilah-daemon/migrations/0026_jira_subtask_skips.sql:1–9` — table schema, PK `(parent_task, jira_key)`, no FK, comment says "stops import from recreating"
- `crates/delilah-daemon/src/store.rs:533–539` — `clear_parent_task_links` (line 533: doc comment, execute, return Ok); reference for cleanup pattern and async fn signature
- `crates/delilah-daemon/src/store.rs:812–856` — test module entry; reference for `tokio::test` macros, `Store::in_memory()`, assert patterns

**New functions signature & patterns**:

- `pub(crate) async fn add_jira_skips<'e, E: Executor<'e, Database = Sqlite>>(e: E, parent: &TaskKey, keys: HashSet<String>) -> Result<(), StoreError>` — `INSERT OR IGNORE`, idempotent; executor param like `become_parent_task` (T006 pattern)
- `pub(crate) async fn remove_jira_skips<'e, E: Executor<'e, Database = Sqlite>>(e: E, parent: &TaskKey, keys: HashSet<String>) -> Result<(), StoreError>` — loop `DELETE WHERE parent_task = ? AND jira_key = ?`
- `pub(crate) async fn jira_skips_of(parent: &TaskKey) -> Result<HashSet<String>, StoreError>` — `SELECT jira_key FROM jira_subtask_skips WHERE parent_task = ? ORDER BY jira_key`
- `pub(crate) async fn delete_jira_skips(&self, parent: &TaskKey) -> Result<(), StoreError>` — `DELETE FROM jira_subtask_skips WHERE parent_task = ?` (no executor param; called from actions.rs)

**Tests**:

- Location: `crates/delilah-daemon/src/store/jira_skips.rs` after the impl block, `#[cfg(test)] mod tests { ... }` pattern
- T008 tests in same file: `add_idempotent`, `remove`, `jira_skips_sorted`, `delete_single_parent_only` (each `#[tokio::test]`)
- T010 test in `crates/delilah-daemon/src/actions.rs` near line 484 (after `deleting_a_parent_task_with_only_archived_subtasks_clears_their_link`): 
  - Create parent task + add skips to it
  - Delete: skips gone, verify `store.jira_skips_of(parent)` is empty
  - Archive a parent task: skips persist

**Callers & impact**:

- `crates/delilah-daemon/src/actions.rs:179` — add `app.store.delete_jira_skips(&key).await?;` after `clear_parent_task_links`, only if `task.kind == TaskKind::ParentTask` (already guarded by line 178)
- `crates/delilah-daemon/src/jira_import.rs` — will call `add_jira_skips` and `remove_jira_skips` on transaction in `apply_jira_import` (T036, US3) and `jira_skips_of` in `preview` (T020, US1), but T008–T010 tests can pass without it
- No other files reference jira_skips in this codebase (new table, new fns)

**Watch out**:

- `delete_jira_skips` is not on a transaction executor param (unlike the others) — it's `&self` like `clear_parent_task_links`; only called from actions.rs on already-running transaction context (delete_task path is sync with archive-workspace)
- Test isolation: each test gets a fresh `Store::in_memory()`, so no cleanup needed between tests
- Task deletion flow (actions.rs:160–196): queued tasks go via `delete_backlog_task`, started tasks via `release_workspace` + `delete_task`; both paths reach line 179; test both with and without skips
- Executor trait bounds: `<'e, E: Executor<'e, Database = Sqlite>>` — used in T006 too; check `sqlx` documentation if unsure (also see `become_parent_task` once T006 lands)

**Patterns in sibling store functions**:

- Async simple updates (no transaction needed): call `sqlx::query(...)`, `.bind(...)`, `.execute(&self.pool)`, check `.rows_affected()` if deletion, return `Ok(())` or `StoreError::NotFound`
- Cleanup on deletion: line 533 pattern — `UPDATE`, no read-after-write; `delete_*` patterns use `DELETE FROM ...` once
- Tests: `#[tokio::test]`, `let store = Store::in_memory().await.unwrap()`, create tasks with `create_task`, assert with `store.list_tasks()` or explicit re-fetch
