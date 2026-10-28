# Implementation Plan: Jira sub-tasks become Delilah subtasks

**Branch**: `013-jira-subtasks-import` | **Date**: 2026-10-28 | **Spec**: [spec.md](spec.md)
**Input**: Feature specification from `specs/013-jira-subtasks-import/spec.md`

## Summary

The Jira adapter for spec 012's parent tasks. A pure planner in a new daemon module joins a
Jira ticket's sub-tasks with the Delilah tasks that have those keys and the user's skips, and
decides for the ticket (create / convert / existing / refused) and for each sub-task (create /
attach / linked / conflict, plus the default tick). One preview endpoint feeds the Jira view's
"Create parent task" dialog, the parent task screen's "N new Jira sub-tasks" offer and the
CLI; one write endpoint applies the user's choice in a single SQLite transaction. Skips live in
a new small table. Sync is pull-on-open only; Delilah never writes to Jira.

## Technical Context

**Language/Version**: Rust 2021 (workspace), TypeScript + React (Vite) in `app/`
**Primary Dependencies**: axum, sqlx (SQLite), serde, reqwest (Jira client); React, vitest
**Storage**: SQLite: new table `jira_subtask_skips` (migration 0026); `tasks` unchanged
**Testing**: `cargo test -p delilah-core -p delilah-integrations -p delilah-daemon -p delilah-cli`;
`cd app && npx tsc --noEmit && npx vitest run`; the sample Jira client stands in for Jira
**Target Platform**: macOS (daemon + Tauri app); nothing native changes
**Project Type**: desktop app with a local daemon (HTTP API) and CLI
**Performance Goals**: preview = 3 Jira calls (issue, sub-tasks, "mine" JQL) + 2 queries
(tasks by keys, skips); the parent task screen never waits for it (SC-005)
**Constraints**: additive API; never write to Jira (FR-013); all-or-nothing writes (FR-006);
hierarchy rules only from `delilah_core::hierarchy` (no second copy)
**Scale/Scope**: 1 migration, 1 new daemon module, 1 client method, 2 endpoints, 1 CLI
command, 1 new UI dialog, 2 UI components touched; ≤ 50 sub-tasks per ticket

## Constitution Check

`.specify/memory/constitution.md` is the unfilled template; the project's rules are in
`CLAUDE.md` and `docs/decisions.md`. Gates:

| Gate | Status |
|---|---|
| Never edit a shipped migration | ✅ new `0026_jira_subtask_skips.sql` |
| API ↔ `app/src/api.ts` in sync, additive | ✅ [contracts/api.md](contracts/api.md) |
| Two levels, spec 012's rules (decision 102) | ✅ reused checks (R3); a Jira sub-task ticket is refused |
| No private-repo data; fixtures only | ✅ sample Jira data + fixture repos |
| Try in a preview, never on 7420 | ✅ [quickstart.md](quickstart.md) |
| Jira key safety in JQL | ✅ `is_issue_key` before the "mine" JQL (R4) |
| Docs updated (status, usage, decisions, architecture) | ✅ last phase |
| No version bump | ✅ none planned |

Post-design re-check: no violations.

## Design

See [research.md](research.md) (R1–R10), [data-model.md](data-model.md),
[contracts/api.md](contracts/api.md), [quickstart.md](quickstart.md).

### Daemon

- **`crates/delilah-daemon/src/jira_import.rs`** (new):
  - `plan_import(...) -> ImportPlan`: pure decision table (data-model), calling
    `hierarchy::check_make_parent_task`, `check_new_subtask`, `check_attach`. Unit-tested
    exhaustively (every row of both tables).
  - `preview(app, key)`: short-circuits without calling Jira when the key fails
    `is_issue_key` or Jira is not configured (`linked: false`). Otherwise Jira client (`issue`,
    `subtasks`, `my_subtask_keys`) + store (`tasks_by_keys`, `jira_skips_of`, the parent task's
    subtasks via `integration::subtask_facts` for `linked` rows and `gone`, session/open-PR
    facts as `api_hierarchy::make_parent_task` gets them) → `plan_import`. "Not found" and
    Jira errors/timeouts become `linked: false` with `reason` (`not-found`, `unreachable`), so
    the parent task screen stays quiet; the dialog shows `unreachable` with retry.
  - handlers `GET`/`POST /api/v1/jira/issues/{key}/import`; `POST` re-runs `preview` (Jira is
    read outside the transaction), validates the requested keys against it, resolves the
    project and builds the `Task` values (below), then calls `apply_jira_import`. An empty
    `add` is valid (skip/unskip only, or a parent task with no subtasks yet). Emits
    `Topic::Tasks` only when a task was created, converted or attached.
  - `apply_jira_import` runs one `BEGIN IMMEDIATE` transaction and **re-checks inside it** the
    facts that can change between preview and write (decision for review finding 3): the
    ticket task's kind, open PR and live session for a convert; for each attach, that the task
    is still normal, never started and in the project; a key that is now taken → `left_out`
    (`taken`). A convert refused at this point aborts everything (409). Then:
    `insert_task` for a new parent task or `store::become_parent_task(tx, key)` for a convert,
    `insert_task` per created subtask, `store::set_kind_tx(tx, Subtask, parent)` per attach,
    and the skip writes.
- **`store.rs`**: executor-taking primitives only, no Jira policy —
  `become_parent_task(executor, key)`: the `UPDATE tasks SET workflow='none', saving_mode=0,
  autopilot=NULL, ci_autofix=0, handoff=NULL` that `api_hierarchy::make_parent_task` runs today
  (`api_hierarchy.rs:131`) plus the kind change, in one function; `make_parent_task` is changed
  to call it, so both paths keep spec 012's invariant (no agent settings on a parent task).
  `set_kind_tx(executor, …)` (with `set_kind` delegating to it), `tasks_by_keys`,
  `has_live_session`/open-PR reads usable on the transaction.
- **`store/jira_skips.rs`** (new, `impl Store`): `jira_skips_of(parent)`, `add_jira_skips`,
  `remove_jira_skips` (executor-taking), `delete_jira_skips(parent)`.
- **`actions.rs`**: deleting a parent task also calls `delete_jira_skips` (next to
  `clear_parent_task_links`, its only call site).
- **`api.rs`**: two routes. `build_task_with` is split in two: `resolve_new_task` (async: project
  lookup, free key, parent task lookup and `check_new_subtask`) and a pure
  `new_task(key, title, kind, parent_task, project, defaults) -> Task` that sets the workflow,
  saving-mode, CI autofix and model defaults exactly as `POST /tasks` does. `POST /tasks` calls
  both; the import resolves the project once, outside the transaction, and calls only
  `new_task`. Subtasks of a parent task created in the same call skip `check_new_subtask`
  (the parent does not exist yet; same project and two levels hold by construction);
  subtasks added to an existing parent task run it.
- **`jira.rs` (daemon)**: `write_ticket` adds `Parent: KEY title` (R9).
- **`migrations/0026_jira_subtask_skips.sql`** (R5).

### Jira client (`crates/delilah-integrations/src/jira.rs`)

- `my_subtask_keys(parent) -> Result<HashSet<String>>` (R4): `is_issue_key` check, JQL
  `parent = KEY AND assignee = currentUser()`, keys only; sample: assignee `SAMPLE_ME`.
- Sample data: make sure one ticket has an open unassigned, an open mine, an open
  someone-else's and a done sub-task (add sub-tasks to an existing sample ticket if needed).

### CLI (`crates/delilah-cli/src/main.rs`)

- `delilah jira import` as in the contract: preview table by default, `--defaults`, `--add`,
  `--skip`, `--unskip`, `--project` (no `--all`/`--dry-run`: the preview is the dry run, and
  `--add` covers the rest).

### UI (`app/`)

- `api.ts`: `JiraImportPreview`, `JiraImportRequest`, `JiraImportResult`; `jiraImportPreview(key)`,
  `jiraImport(key, req)`.
- `components/JiraImportDialog.tsx` (new) + `i18n/en/JiraImportDialog.ts` (+ es): two modes —
  *create* (project picker; parent action line; checklist of sub-tasks with key, title, Jira
  status, assignee, "done in Jira"/"assigned to X"/"already a task" tags, defaults from
  `selected`, "Skip" per row) and *add* (same list, only `create`/`attach` rows, plus the
  skipped ones with "Undo"). Shows `left_out` after confirming.
- `components/JiraView.tsx`: "Create parent task" on a ticket with `subtask_count > 0` and not
  `subtask`; opens the dialog in create mode (or add mode when `parent.action == existing`).
- `components/SubtaskList.tsx`: loads the preview on mount without blocking; banner
  "N new Jira sub-tasks · Add" (N = selectable, non-skipped `create`/`attach` rows), Jira
  status next to each subtask, "not in Jira any more" for `gone`, "Skipped (n)" link; a quiet
  note when `linked: false` because Jira is not connected, nothing for `not-found`.

### Tests

- `jira_import.rs`: `plan_import` table tests (each parent and sub-task case, default
  selection, skipped, gone, ticket that is a sub-task); handler tests with the sample client:
  preview, create, convert, existing+add, refusal → nothing written, `left_out`, skips/unskip,
  second run creates nothing (SC-003), bad key, unknown key in `add`.
- `apply_jira_import`: rolls back on a failed insert after a convert (nothing converted,
  nothing created); a session started on the ticket task between preview and write → 409 and
  nothing written; an attach target started meanwhile → `left_out`.
- `store`: `become_parent_task` clears workflow/autopilot/CI autofix/saving mode;
  `make_parent_task` still passes its existing tests through it.
- `actions.rs`: deleting a parent task clears its skips.
- `integrations/jira.rs`: `my_subtask_keys` on sample data; bad key refused.
- `daemon/jira.rs`: `write_ticket` writes the `Parent:` line.
- CLI: argument parsing and preview printing (existing CLI test style).
- UI: `JiraImportDialog.test.tsx` (defaults, toggles, tags, result), `SubtaskList.test.tsx`
  (banner count, Jira status, gone, quiet when unlinked), `JiraView` button visibility.
- Submodules inheritance (FR-005, R8): one daemon test that a subtask created by the import
  starts with its parent task's submodules (covered by spec 012's path; assert, don't rebuild).

### Docs (last phase)

`docs/usage.md` ("Parent tasks and subtasks" and "Jira"), `docs/status.md` (drop "Not yet: the
Jira adapter", add the feature), `docs/decisions.md` (new decision: one-way, pull-on-open sync;
default selection rules; skips table; ruled out: background polling, writing to Jira, stored
Jira link), `docs/architecture.md` (`jira_import.rs`, migration 0026).

## Project Structure

### Documentation (this feature)

```text
specs/013-jira-subtasks-import/
├── plan.md
├── research.md
├── data-model.md
├── quickstart.md
├── contracts/api.md
├── checklists/requirements.md
└── tasks.md            # /speckit-tasks
```

### Source Code (repository root)

```text
crates/delilah-integrations/src/jira.rs        # my_subtask_keys, sample data
crates/delilah-daemon/migrations/0026_jira_subtask_skips.sql
crates/delilah-daemon/src/jira_import.rs       # new: planner, preview, handlers
crates/delilah-daemon/src/store.rs             # become_parent_task, set_kind_tx, tasks_by_keys
crates/delilah-daemon/src/store/jira_skips.rs  # new: skips queries
crates/delilah-daemon/src/api_hierarchy.rs     # make_parent_task calls become_parent_task
crates/delilah-daemon/src/actions.rs           # delete clears skips
crates/delilah-daemon/src/api.rs               # routes, shared new-task helper
crates/delilah-daemon/src/jira.rs              # write_ticket Parent line
crates/delilah-daemon/src/lib.rs (or main.rs)  # mod jira_import
crates/delilah-cli/src/main.rs                 # delilah jira import
app/src/api.ts
app/src/components/JiraImportDialog.tsx (+ test, i18n en/es)
app/src/components/JiraView.tsx
app/src/components/SubtaskList.tsx (+ test, i18n)
docs/usage.md, docs/status.md, docs/decisions.md, docs/architecture.md
```

**Structure Decision**: Jira-specific logic stays out of the generic hierarchy code: a new
daemon module next to `jira.rs` uses `delilah_core::hierarchy` and the store, and the generic
`/tasks` routes are untouched.

## Complexity Tracking

None.
