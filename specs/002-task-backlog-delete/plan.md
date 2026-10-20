# Implementation Plan: Task backlog and deleting tasks

**Branch**: `feature/FEAT-10` | **Date**: 2026-10-20 | **Spec**: [spec.md](spec.md)

**Input**: Feature specification from `specs/002-task-backlog-delete/spec.md`

## Summary

(1) **Backlog**: `TaskStatus::Queued` is the backlog. `POST /tasks` gains a project,
notes and free keys; the dialog gets "Guardar en backlog"; the sidebar gets a folded
"Backlog (n)" section per project and the board a "Backlog" column. `POST /runs`
with the key of a queued task starts that task, and a failed start puts it back
(`Event::StartAborted`). (2) **Delete**: `DELETE /tasks/{key}[?force]` runs
archive's unpushed check first (nothing is stopped if it fails), stops the agents,
releases the workspace with archive's helper, and deletes the row (FK cascade does
the rest), then the in-memory state and the task dir. Free-task numbers never
repeat. See [research.md](research.md).

## Technical Context

**Language/Version**: Rust (edition 2024) + TypeScript/React (Tauri app)

**Primary Dependencies**: axum, sqlx (SQLite), tokio, clap (CLI), React

**Storage**: SQLite (`~/.delilah`): migration `0009_task_notes.sql` (`tasks.notes`); setting `free_key_last`

**Testing**: `cargo test` (core state machine, store, daemon API with in-memory store and fixture repos, git), `tsc --noEmit`, vitest

**Target Platform**: macOS (daemon + Tauri app)

**Project Type**: desktop-app + local daemon + CLI

**Performance Goals**: delete and create as fast as archive and start today; views update through the existing SSE topics

**Constraints**: nothing can be lost without `force`; a delete is complete or doesn't happen; old `queued` rows stay valid (no status migration)

**Scale/Scope**: single user, tens of tasks per project

## Constitution Check

`.specify/memory/constitution.md` is still the unfilled template; the project's
ground rules (`CLAUDE.md`) serve as gates instead:

| Gate | Status |
|---|---|
| Nothing from the work repos; fixtures only | ✅ tests use `make-fixture-parent.sh` repos and the in-memory store |
| Agent hooks/settings outside the repo, clean env | ✅ unchanged (`launch_session` is reused as is) |
| Try changes only in a preview, never 7420 / pkill | ✅ quickstart uses `scripts/preview.sh` |
| No remote operations without confirming the account | ✅ delete never touches the remote; no push in this plan |
| MVP-0 scope | ✅ extends the existing task lifecycle |

Post-design re-check: no change.

## Project Structure

### Documentation (this feature)

```text
specs/002-task-backlog-delete/
├── spec.md
├── plan.md
├── research.md
├── data-model.md
├── quickstart.md
├── contracts/api.md
└── checklists/{requirements,lifecycle}.md
```

### Source Code

```text
crates/delilah-core/src/task.rs          # Task.notes, Event::StartAborted (+ name), next_status, tests
crates/delilah-git/src/workspace.rs      # archive_workspace → check_workspace + remove_workspace (archive keeps its API)
crates/delilah-git/tests/workspace.rs    # check without removing
crates/delilah-daemon/migrations/0009_task_notes.sql
crates/delilah-daemon/src/store.rs       # notes in create/get/list/task_from_row, update_backlog_task, delete_task,
│                                        # apply_event with BEGIN IMMEDIATE
crates/delilah-daemon/src/api.rs         # NewTask (+project, notes, free), PATCH /tasks/{key}, DELETE /tasks/{key}
crates/delilah-daemon/src/run.rs         # start_run: existing queued key → start it; launch_task(); StartAborted;
│                                        # notes.md + prompt pointer; next_free_key with free_key_last
crates/delilah-daemon/src/actions.rs     # release_workspace() shared by archive and delete; delete()
crates/delilah-daemon/src/asks.rs, phases.rs, sessions.rs   # forget(key) for the in-memory state
crates/delilah-cli/src/main.rs           # new --project/--notes/--free, rm/delete [--force], ls "backlog"
app/src/api.ts                           # Task.notes, createTask, updateTask, deleteTask; statusLabel queued → "Backlog"
app/src/components/NewTaskDialog.tsx     # "Notas" field, "Guardar en backlog", `existing` mode (key fixed, prefilled) for "Empezar"
app/src/components/Sidebar.tsx           # queued out of ORDER; folded "Backlog (n)" per project; row menu "Eliminar"
app/src/components/BoardView.tsx, app/src/phases.ts   # "backlog" column first; columnOf(queued) = backlog
app/src/components/TaskView.tsx          # backlog task: edit title/notes/project, "Empezar"; "Eliminar" + confirm + force
app/src/components/DeleteTaskDialog.tsx  # new: confirmation per state, "Eliminar igualmente"
app/src/App.tsx (or TaskScreen.tsx)      # leave the view of a deleted task
app/src/i18n/en.ts, app/src/styles.css
```

**Structure Decision**: the existing layout; one new UI component (the delete dialog).

## Design (by layer)

### 1. Core (`delilah-core/src/task.rs`)

- `Task { …, #[serde(default)] notes: Option<String> }`; `Task::new` sets `None`.
- `Event::StartAborted { reason: String }`, name `start-aborted`;
  `next_status(PreparingWorkspace, StartAborted) = Queued`; invalid from any other state.
- Tests: aborted start goes back to queued; StartAborted from running is invalid; a
  task can be started again after an aborted start.

### 2. Git (`delilah-git/src/workspace.rs`)

- `check_workspace(ws, opts) -> Result<(), GitError>`: the dirty/unpushed loop
  that is in `archive_workspace` today.
- `remove_workspace(root, ws, opts)`: `worktree remove --force` + `prune`.
- `archive_workspace(root, ws, force, opts)` = `if !force { check }` + `remove` (same behaviour).

### 3. Store

- Migration `0009`: `ALTER TABLE tasks ADD COLUMN notes TEXT;`
- `create_task`, `get_task`, `list_tasks`, `apply_event` read/write `notes`.
- `apply_event`: `self.pool.begin_with("BEGIN IMMEDIATE")` (R4).
- `update_backlog_task(key, title?, notes?, project?)`:
  `UPDATE … WHERE key = ? AND status = '{"type":"queued"}'`; 0 rows → `Conflict`/`NotFound`.
- `delete_task(key)`: `DELETE FROM tasks WHERE key = ?` (the cascade does the rest); test that
  events, workspace and sessions are gone.
- `next_free_number()` / `set_free_number(n)` over `settings`.

### 4. Daemon

- **`api.rs`**
  - `NewTask { key: Option, title, project, notes, free, workflow: Option }` → checks the
    project exists, defaults the workflow, `next_free_key` when `free`. Emits `Tasks`.
  - `PATCH /api/v1/tasks/{key}` → `update_backlog_task`, emits `Tasks`.
  - `DELETE /api/v1/tasks/{key}` → `actions::delete`.
- **`run.rs`**
  - `start_run`: after resolving the key, `store.get_task(key)`:
    - `Some(queued)` → `existing = true`; title from the task if the body has none.
    - `Some(other)` → 409 (as `create_task` does today).
    - `None` → `create_task` as today.
    Then the same checks, `Event::Start`, and `launch_task(&app, &key, req, existing)`.
  - On workspace failure: `existing` → `StartAborted`, else `Fail` (today).
  - Notes: if the task has notes, write `notes.md` to the task dir and append the
    pointer to the prompt (both branches of the `match (&req.prompt, issue)`).
  - `next_free_key`: `max(setting, max existing) + 1`, saves the setting.
- **`actions.rs`**
  - `release_workspace(app, key, force) -> Result<(), ApiError>`: what `archive` does after
    its terminal check (kill sessions, `leave_checkout` / `archive_workspace`, `delete_workspace`).
    `archive` calls it.
  - `delete(key, force)`: R6 steps 1–8. Step 2 calls `check_workspace` in `spawn_blocking`
    only for a worktree; the 409 carries `"unpushed": true` so the UI can offer
    "Eliminar igualmente". The release in step 4 passes `force: true` (already checked).
  - `forget(key)` helpers: `app.asks.forget(key)`, `app.review_hints.forget(key)`;
    `app.sessions` entries are dropped when their PTY exits (check; otherwise remove).

### 5. CLI

- `New { key: Option, --title, --project (required), --notes, --free }`.
- `Rm { key, --force }` (`#[command(alias = "delete")]`): `DELETE`; on `unpushed` prints
  the error + "use --force to delete it anyway", exit 1.
- `Ls`: `queued` shown as `backlog`.
- `Run`: `--title` optional (the daemon fills it in for an existing task; 400 for a new one).

### 6. App

- `api.ts`: `Task.notes?: string | null`; `createTask`, `updateTask`, `deleteTask(key, force)`
  (`request` with `method: "DELETE"`); `statusLabel(queued)` → "Backlog".
- `NewTaskDialog`: a "Notas" textarea (optional); a secondary "Guardar en backlog" button
  (disabled with no project/title, like the main one) → `api.createTask`;
  prop `existing?: Task` → key and origin fixed, title/notes prefilled, the main button
  "Empezar" → `api.run({ key: existing.key, … })`. The notes are saved with
  `updateTask` before running if they changed.
- `Sidebar`: `queued` out of `ORDER` and out of the active list; per project a
  `<details>` "Backlog (n)" (closed by default, hidden when n = 0) with its rows in
  `list_tasks` order; a row menu (right-click / "⋯") with "Eliminar".
- `BoardView`/`phases.ts`: `Column` gains `backlog` (first); `columnOf` returns it for `queued`.
- `TaskView`: for `queued`: editable title/notes/project, "Empezar" (opens the dialog in
  `existing` mode), "Eliminar". For other states, "Eliminar" next to "Archivar workspace".
- `DeleteTaskDialog`: the text depends on the state (backlog: just the confirmation; live agent:
  "Se parará el agente."); on 409 with `unpushed`, shows the error and "Eliminar igualmente
  (se pierde lo no subido)" → `deleteTask(key, true)`.
- After a delete of the selected task, the screen goes back to the project/board (the
  `tasks` SSE refresh no longer has the key → clear the selection).
- Inbox/badge: they come from session `attention`; deleted with the rows, refreshed by `inbox`.
  Backlog tasks have no sessions, so they never count.

## Testing strategy

Tests first, bottom-up: core transitions → git `check_workspace` → store (notes, update only
while queued, delete cascade, free numbers) → daemon API (create backlog task, PATCH
conflict, `/runs` on an existing queued key with a fixture repo, aborted start goes back
to queued, two concurrent starts → one 201 + one 409, delete backlog/running/unpushed
without force/with force, deleted key reusable, `LIBRE-n` not reused) → CLI by hand →
UI by hand in a preview ([quickstart.md](quickstart.md)).

## Risks / limitations

- `BEGIN IMMEDIATE` for every event briefly serializes writers; events are tiny and rare,
  but the `busy_timeout` must be > 0 (check the pool options).
- The agent is stopped before the workspace is released: if git then fails, the task
  stays with no agent (shown as it was; the user can retry, force, or resume).
- Removing the task dir is best effort: a leftover dir is harmless, but a later task
  with the same Jira key would find old logs; logged as a warning.
- A task in the project's checkout with uncommitted changes: `leave_checkout` leaves
  the folders it can't switch on the task branch (as archive), logged; the task is still deleted.
- Tasks whose project was removed keep `project = NULL`; "Empezar" then needs a project
  (the dialog's project picker already covers it).

## Complexity Tracking

No violations.
