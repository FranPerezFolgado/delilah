# WP-03 Code Map: Store Primitives

## Tasks
- **T005**: Write tests in `crates/delilah-daemon/src/store.rs` test module for three functions
- **T006**: Implement three functions in `crates/delilah-daemon/src/store.rs`
- **T007**: Refactor `make_parent_task` in `crates/delilah-daemon/src/api_hierarchy.rs` (~L107–144) to call new `become_parent_task`

---

## Files & Functions

| File | Location | What | Why |
|------|----------|------|-----|
| `crates/delilah-daemon/src/store.rs` | L812+ | `#[cfg(test)] mod tests` | Where T005 tests go; uses `Store::in_memory()` fixture pattern (L818) |
| `crates/delilah-daemon/src/store.rs` | L90–111 | `create_task_from_conversation` | Pattern: generic executor + BEGIN IMMEDIATE transaction (L97), insert_task call, commit (L109) |
| `crates/delilah-daemon/src/store.rs` | L97 | `begin_with("BEGIN IMMEDIATE")` | Helper pattern for immediate transactions; T006 should expose a `begin_immediate()` variant |
| `crates/delilah-daemon/src/store.rs` | L641–682 | `insert_task<'e, E: Executor>` | Generic executor pattern; takes `impl sqlx::Executor<'e, Database = sqlx::Sqlite>` to work on pool or tx |
| `crates/delilah-daemon/src/store.rs` | L509–529 | `set_kind(&self, key, kind, parent_task)` | Async method doing UPDATE kind/parent_task; T006 must refactor this to `set_kind_tx` for use in transactions |
| `crates/delilah-daemon/src/store.rs` | L1041, L1050 | Test calls to `set_kind` | Example of test pattern; will have equivalent T005 tests |
| `crates/delilah-daemon/src/api_hierarchy.rs` | L107–144 | `make_parent_task` handler | Currently inline UPDATE (L130–137) + `set_kind` call (L140); T007 replaces with call to `store::become_parent_task` |
| `crates/delilah-daemon/src/api_hierarchy.rs` | L130–137 | Inline UPDATE statement | `UPDATE tasks SET workflow='none', saving_mode=0, autopilot=NULL, ci_autofix=0, handoff=NULL WHERE key=?`; this becomes the body of `become_parent_task` |
| `crates/delilah-daemon/src/store/jira_skips.rs` | L1–5 | Empty impl block | Skeleton from T001; not in scope T005–T007 (T008–T010 implements the four skip functions) |

---

## Patterns

**Generic Executor on Pool or Transaction**  
Functions taking `impl sqlx::Executor<'e, Database = sqlx::Sqlite>` work unchanged whether given `&pool` or `&mut *tx`. See `insert_task` at L641–682 and its use in `create_task_from_conversation` (L98).

**Transaction & Commit**  
```rust
let mut tx = self.pool.begin_with("BEGIN IMMEDIATE").await?;
// ... do work using &mut *tx as executor ...
tx.commit().await?;
```
Pattern from L97–109; atomically apply multiple updates or fail all.

**Query Binding & NULL**  
Keys as `task.key.as_str()`, booleans/ints as-is, JSON via `serde_json::to_string(value)?`, NULL as `Option::<T>::None`. See L646–675.

**Error on No Rows**  
After UPDATE/DELETE, check `rows_affected() == 0` → `Err(StoreError::NotFound(key.clone()))`. See L523–525 in `set_kind`.

**Test Factory**  
Helper `task(key: &str) -> Task { Task::new(TaskKey::new(key), title, project) }` at L834; creates blank tasks for tests.

---

## Tests

**Location**: `crates/delilah-daemon/src/store.rs:812+`  
**Pattern**: `#[tokio::test] async fn test_name() { let store = Store::in_memory().await.unwrap(); ... }`  
**Fixture**: `Store::in_memory()` per test isolates state (single connection, in-memory SQLite).

**T005 Must Test**:
1. `become_parent_task(&pool, key)` sets `kind = parent_task`, clears `workflow = 'none'`, `saving_mode = 0`, `autopilot/ci_autofix/handoff = NULL`
2. `set_kind_tx` on a transaction rolls back with the transaction (test roll-back on conflict/constraint)
3. `tasks_by_keys(&[..])` returns only existing tasks, any kind, archived included

**Example Existing Tests**:
- L816–832: Basic create + get with asserts
- L838–858: `set_ci_autofix` (update assertion pattern)
- L1041–1050: `set_kind` tests showing kind/parent_task changes

---

## Watch Out

1. **`set_kind` Refactor**: The async `set_kind` at L509 does UPDATE on the pool directly. T006 must add a `set_kind_tx` variant that takes an executor; the async version can then delegate to it.

2. **Inline UPDATE in `make_parent_task`**: The UPDATE at L130–137 in `api_hierarchy.rs` is currently hardcoded there. T007 will move it into `become_parent_task` and call that instead. The statement must be identical (workflow, saving_mode, autopilot, ci_autofix, handoff, kind, WHERE key).

3. **Generic Executor Trait**: All three new functions must work with both `&SqlitePool` (for non-transactional callers) and `&mut *SqliteTransaction` (for batched writes). `insert_task` at L641 shows the pattern.

4. **Caller**: `make_parent_task` in `api_hierarchy.rs:140` will change from calling `app.store.set_kind(...)` to a new `store::become_parent_task` that does both the UPDATE and set_kind in one call.

5. **`begin_immediate()` Helper**: T006 should add a private or `pub(crate)` helper that wraps `self.pool.begin_with("BEGIN IMMEDIATE")` and returns a transaction, following the pattern at L97. This simplifies callers like the import logic that will use it.

---

## Related Symbols to Know

- `TaskKey` (`delilah_core`): Key type, converts to/from string via `as_str()`
- `TaskKind` (`delilah_core::hierarchy`): Enum with variants `ParentTask`, `Subtask`, etc.
- `StoreError`: Error enum; `NotFound(TaskKey)`, `Db(sqlx::Error)` variants
- `Task`: Full task struct with all fields; serialized/deserialized via JSON in DB
- `kind_name(TaskKind) -> &'static str`: Converter for storage; see use at L670
