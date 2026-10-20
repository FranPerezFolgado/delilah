---
description: "Task list for Task backlog and deleting tasks"
---

# Tasks: Task backlog and deleting tasks

**Input**: Design documents from `specs/002-task-backlog-delete/`

**Prerequisites**: plan.md, spec.md, research.md, data-model.md, contracts/api.md, quickstart.md

**Tests**: included. The plan asks for tests first, bottom-up, in the inline `#[cfg(test)]` style of each crate (in-memory `Store`, fixture repos from `scripts/make-fixture-parent.sh`).

**Organization**: grouped by user story (US1 save to backlog, US2 start a backlog task, US3 delete a task).

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies on incomplete tasks)
- **[Story]**: US1, US2, US3 (spec.md)

## Path Conventions

- Core: `crates/delilah-core/src/task.rs`
- Git: `crates/delilah-git/src/workspace.rs`, `crates/delilah-git/tests/workspace.rs`
- Daemon: `crates/delilah-daemon/src/{store.rs,api.rs,run.rs,actions.rs,asks.rs,phases.rs}`, `crates/delilah-daemon/migrations/`
- CLI: `crates/delilah-cli/src/main.rs`
- App: `app/src/api.ts`, `app/src/phases.ts`, `app/src/components/*.tsx`, `app/src/i18n/en.ts`, `app/src/styles.css`

Try everything in a preview (`scripts/preview.sh`), never against the daemon on 7420.

---

## Phase 1: Setup

- [X] T001 Run `cargo test --workspace` and `cd app && npx tsc --noEmit && npx vitest run`; note any pre-existing failure before starting (no files changed)

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: notes on the task, needed by US1 (save them) and US2 (pass them to the agent).

- [X] T002 Add `#[serde(default)] pub notes: Option<String>` to `Task` (doc comment: free text the user keeps with the task; reaches the agent on start), `None` in `Task::new`, in crates/delilah-core/src/task.rs
- [X] T003 Create crates/delilah-daemon/migrations/0009_task_notes.sql with a comment line and `ALTER TABLE tasks ADD COLUMN notes TEXT;`
- [X] T004 Read and write `notes` in `create_task`, `get_task`, `list_tasks`, `apply_event`'s SELECT and `task_from_row`; extend `keeps_tasks_and_rejects_duplicates` to round-trip a note, in crates/delilah-daemon/src/store.rs
- [X] T005 [P] Add `notes?: string | null` to the `Task` type and make `statusLabel` return "Backlog" for `queued`, in app/src/api.ts (+ "Backlog" in app/src/i18n/en.ts)

**Checkpoint**: `cargo test --workspace` green; existing tasks load with `notes: null`.

---

## Phase 3: User Story 1 - Save a task for later (Priority: P1) 🎯 MVP

**Goal**: "Guardar en backlog" stores a task with project and notes, no workspace, no agent; it shows in a folded "Backlog (n)" per project and in a "Backlog" board column.

**Independent Test**: in a preview, save a free task to the backlog; it shows in the sidebar section and board column, `git branch` in the fixture shows no new branch, no session exists, and it survives a preview restart.

### Tests for User Story 1

- [X] T006 [P] [US1] Store tests: `next_free_number` returns `max(setting, max existing LIBRE-n) + 1` and stores it, so deleting the newest `LIBRE-n` row doesn't hand its number out again, in crates/delilah-daemon/src/store_settings.rs (or store.rs)
- [X] T007 [P] [US1] API tests: `POST /api/v1/tasks` with `project`, `notes`, `free: true` → 201, key `LIBRE-1`, `status queued`, notes kept, no workspace row, no session; unknown project → 404; missing project → 400; workflow defaults to the project's, in crates/delilah-daemon/src/api.rs (next to `creates_and_lists_tasks`)

### Implementation for User Story 1

- [X] T008 [US1] Add `free_key_last` read/write helpers over `settings` in crates/delilah-daemon/src/store_settings.rs, and make `next_free_key` use `max(stored, max existing) + 1` and save it, in crates/delilah-daemon/src/run.rs (make it `pub(crate)`)
- [X] T009 [US1] Change `NewTask` to `{ key: Option<String>, title, project, notes: Option<String>, free: bool, workflow: Option<String> }`; check the project exists, default the workflow to the project's, call `next_free_key` when `free`, set `task.project` and `task.notes`, emit `Topic::Tasks`, in crates/delilah-daemon/src/api.rs
- [X] T010 [US1] Add `PATCH /api/v1/tasks/{key}` (`{title?, notes?, project?}`) backed by a new `Store::update_backlog_task` that updates only while the status is queued (409 otherwise, 404 for an unknown project) + a test, in crates/delilah-daemon/src/store.rs and crates/delilah-daemon/src/api.rs
- [X] T011 [P] [US1] CLI: `New { key: Option<String>, --title, --project (required), --notes, --free }` posting the new body and printing the key; `Ls` shows `backlog` for `queued`, in crates/delilah-cli/src/main.rs
- [X] T012 [P] [US1] Add `createTask({ key?, title, project, notes?, free })` and `updateTask(key, change)` (PATCH) to `api` in app/src/api.ts
- [X] T013 [US1] NewTaskDialog: optional "Notas" textarea; a secondary "Guardar en backlog" button (same disabled rules as start) calling `api.createTask`, closing the dialog and selecting nothing, in app/src/components/NewTaskDialog.tsx
- [X] T014 [US1] Sidebar: drop `queued` from `ORDER` and from the active list; per project a `<details>` "Backlog (n)" (closed by default, hidden when n = 0) listing its queued tasks in `list_tasks` order; queued tasks with no project go under "Sin proyecto", in app/src/components/Sidebar.tsx (+ styles in app/src/styles.css)
- [X] T015 [P] [US1] Board: add a first `backlog` column to `COLUMNS` and return it from `columnOf` for `queued` tasks whatever their spec, in app/src/phases.ts (check app/src/components/BoardView.tsx needs no change)
- [X] T016 [US1] TaskView for a queued task: editable title, notes and project (save with `api.updateTask`), no terminal/changes panes, in app/src/components/TaskView.tsx (and app/src/components/TaskScreen.tsx if that's where the panes are chosen)
- [X] T017 [P] [US1] New strings in app/src/i18n/en.ts: "Guardar en backlog", "Notas", "Backlog ({n})", "Sin proyecto"

**Checkpoint**: US1 by hand in a preview (quickstart §1); the existing start flow is unchanged.

---

## Phase 4: User Story 2 - Start a backlog task (Priority: P1)

**Goal**: "Empezar" (and `delilah run KEY`, and "Analizar con Claude" on a ticket in the backlog) starts the existing queued task; its notes reach the agent; a failed start leaves it in the backlog.

**Independent Test**: start a backlog task in a preview; it leaves the backlog, gets a workspace and an agent, and `notes.md` exists in its task dir. Starting one whose branch already exists fails and leaves it in the backlog.

### Tests for User Story 2

- [X] T018 [P] [US2] Core tests: `PreparingWorkspace --StartAborted--> Queued`; `StartAborted` from `Running`/`Queued` is invalid; a task can `Start` again after an aborted start, in crates/delilah-core/src/task.rs
- [X] T019 [P] [US2] API tests with a fixture repo: `POST /runs` on an existing queued key → 201, same key, title from the task, `running`; on a non-queued key → 409; with a branch that already exists → 422 and the task is `queued` again with a `start-aborted` event; with notes → `<task_dir>/notes.md` written and its path in the session's first prompt; two concurrent `POST /runs` on the same queued key → one 201, one 409, in crates/delilah-daemon/src/run.rs (tests module)

### Implementation for User Story 2

- [X] T020 [US2] Add `Event::StartAborted { reason: String }` (name `start-aborted`, doc comment) and the `PreparingWorkspace → Queued` arm in `next_status`, in crates/delilah-core/src/task.rs
- [X] T021 [US2] Open `apply_event`'s transaction with `BEGIN IMMEDIATE` (`pool.begin_with`) and make sure the pool has a non-zero `busy_timeout`, in crates/delilah-daemon/src/store.rs
- [X] T022 [US2] `start_run`: look the key up first; an existing queued task is started (`existing = true`, title from the task unless the body has one), any other state → 409, none → `create_task` as today; move everything from `Event::Start` on into `launch_task(&app, &key, &req, existing)`; on workspace failure apply `StartAborted` when `existing`, `Fail` otherwise, in crates/delilah-daemon/src/run.rs
- [X] T023 [US2] Notes to the agent: when the task has non-empty notes, write `<task_dir>/notes.md` and append the pointer ("tus notas sobre la tarea están en <path>" / "your notes on the task are at <path>", via `app.lang().pick`) to the prompt, for both the default and a custom prompt, in crates/delilah-daemon/src/run.rs
- [X] T024 [P] [US2] CLI: `Run`'s `--title` becomes optional (the daemon fills it in for an existing task; it still answers 400 for a new one), in crates/delilah-cli/src/main.rs
- [X] T025 [US2] NewTaskDialog `existing?: Task` mode: origin and key fixed, title/notes/project prefilled, main button "Empezar" → save changed notes with `updateTask`, then `api.run({ key: existing.key, … })`; on error the dialog stays open with the message, in app/src/components/NewTaskDialog.tsx
- [X] T026 [US2] "Empezar" button on a queued task opening the dialog in `existing` mode; after a start the task moves out of the backlog via the `tasks` refresh, in app/src/components/TaskView.tsx
- [X] T027 [P] [US2] Strings in app/src/i18n/en.ts: "Empezar"

**Checkpoint**: US2 by hand (quickstart §2–3); starting a brand-new task directly still marks it failed on a workspace error (as before).

---

## Phase 5: User Story 3 - Delete a task (Priority: P1)

**Goal**: any task can be deleted after a confirmation; unpushed work in a worktree is never lost without forcing; the task disappears everywhere and the delete is complete or doesn't happen.

**Independent Test**: in a preview, delete a backlog task, a running task with an unpushed commit (decline, then force) and a checkout task; none appear in any view or in `delilah ls`, also after restarting the preview; the branch is still there; the checkout is back on its previous branch.

### Tests for User Story 3

- [X] T028 [P] [US3] Git test: `check_workspace` reports `Dirty`/`Unpushed` and leaves the worktree in place; `archive_workspace` behaves as before, in crates/delilah-git/tests/workspace.rs
- [X] T029 [P] [US3] Store test: `delete_task` removes the task and, through the cascade, its events, workspace and sessions (with attention); unknown key → `NotFound`, in crates/delilah-daemon/src/store.rs
- [X] T030 [P] [US3] API tests: delete a backlog task → 204 and gone from `GET /tasks`; `preparing-workspace` → 409; a worktree task with an unpushed commit → 409 with `"unpushed": true` and the task, workspace and session untouched; same with `?force=true` → 204, worktree gone, branch still in the repo, task dir gone; a checkout task → checkout back on its previous branch; the deleted Jira key can be created again, in crates/delilah-daemon/src/actions.rs (tests module) or crates/delilah-daemon/src/api.rs

### Implementation for User Story 3

- [X] T031 [US3] Split `archive_workspace` into `pub fn check_workspace(ws, opts)` (the dirty/unpushed loop) and `pub fn remove_workspace(root, ws, opts)` (worktree remove + prune); keep `archive_workspace(root, ws, force, opts)` as `check` (unless force) + `remove`, in crates/delilah-git/src/workspace.rs (export in crates/delilah-git/src/lib.rs)
- [X] T032 [US3] Add `Store::delete_task(key)`: one `DELETE FROM tasks WHERE key = ?`, `NotFound` when no row, in crates/delilah-daemon/src/store.rs
- [X] T033 [US3] Extract `release_workspace(app, key, force)` from `archive` (kill + wait the task's live sessions, `leave_checkout` or `archive_workspace`, `delete_workspace`) and make `archive` call it, in crates/delilah-daemon/src/actions.rs
- [X] T034 [US3] Add `forget(key)` for the in-memory per-task state (`Asks` and the review hints), and make sure live session entries of a killed task are dropped from `app.sessions`, in crates/delilah-daemon/src/asks.rs, crates/delilah-daemon/src/phases.rs, crates/delilah-daemon/src/sessions.rs
- [X] T035 [US3] Add `actions::delete` (`DELETE /api/v1/tasks/{key}`, `?force`), following research R6: 409 while preparing; `check_workspace` first for a worktree unless forced (409 + `unpushed: true`, an `ApiError` variant or field for it); `release_workspace(force: true)`; `delete_task`; `forget`; remove the task dir (warn on failure); emit `Tasks`, `Sessions`, `Inbox`, `Specs`; route it in crates/delilah-daemon/src/api.rs
- [X] T036 [P] [US3] CLI: `Rm { key, --force }` with `#[command(alias = "delete")]`; prints "deleted KEY"; on `unpushed` prints the error + "use --force to delete it anyway" and exits 1, in crates/delilah-cli/src/main.rs
- [X] T037 [P] [US3] Add `deleteTask(key, force = false)` (method DELETE, 204 → null) to `api`, and expose the `unpushed` flag on `ApiError`, in app/src/api.ts
- [X] T038 [US3] New DeleteTaskDialog: "¿Eliminar {key}?" plus "Se parará el agente." when it has a live agent; "Eliminar" → `deleteTask`; on 409 with `unpushed` shows the message and "Eliminar igualmente (se pierde lo no subido)" → `deleteTask(key, true)`; Escape/backdrop cancel, in app/src/components/DeleteTaskDialog.tsx
- [X] T039 [US3] "Eliminar" in TaskView (every state, next to "Archivar workspace") and in a row menu (right-click / "⋯") in the Sidebar, both opening DeleteTaskDialog, in app/src/components/TaskView.tsx and app/src/components/Sidebar.tsx
- [X] T040 [US3] When the selected task disappears after a `tasks` refresh, go back to its project (or the board) without an error, in app/src/components/TaskScreen.tsx (or app/src/App.tsx, wherever the selection lives)
- [X] T041 [P] [US3] Strings in app/src/i18n/en.ts: "Eliminar", "¿Eliminar {key}?", "Se parará el agente.", "Eliminar igualmente (se pierde lo no subido)", "Cancelar" if missing

**Checkpoint**: US3 by hand (quickstart §4); archive still works as before.

---

## Phase 6: Polish & Cross-Cutting Concerns

- [X] T042 [P] Document `delilah new --project/--notes/--free`, `delilah rm` and the backlog in README.md (CLI section)
- [X] T043 Run `cargo fmt --all`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, `cd app && npx tsc --noEmit && npx vitest run`
- [ ] T044 (API + CLI parts done in preview 1; UI by hand still pending) Run all of quickstart.md in a preview (`scripts/preview.sh --fresh`), including §5 (free keys not reused) and a preview restart after deleting

---

## Dependencies & Execution Order

- **Setup (T001)** → **Foundational (T002–T005)** → user stories.
- **US1 (T006–T017)**: needs Foundational only. MVP.
- **US2 (T018–T027)**: needs Foundational; its UI (T025–T026) builds on US1's dialog changes (T013) and backlog TaskView (T016).
- **US3 (T028–T041)**: needs Foundational only on the daemon side (independent of US1/US2); T039 places "Eliminar" in the Sidebar section added by T014.
- Within each story: tests → core/git → store → daemon → CLI → app.
- Same-file sequences: `run.rs` T008 → T022 → T023; `api.rs` T009 → T010 → T035; `NewTaskDialog.tsx` T013 → T025; `TaskView.tsx` T016 → T026 → T039; `Sidebar.tsx` T014 → T039; `main.rs` T011 → T024 → T036; `en.ts` T017 → T027 → T041.

## Parallel Examples

- Foundational: T005 (app) alongside T002–T004 (Rust).
- US1: T006 + T007 (tests), then T011 (CLI) + T012 (api.ts) + T015 (phases.ts) while T008–T010 land.
- US2: T018 + T019 together; T024 (CLI) alongside T020–T023.
- US3: T028 + T029 + T030 together; T036 (CLI) + T037 (api.ts) alongside T031–T035.

## Implementation Strategy

1. **MVP = US1**: the backlog replaces the notes app even before it can be started (`delilah run KEY` can't start a backlog task yet, so ship US2 right after).
2. **US2**: closes the loop: backlog → work.
3. **US3**: independent on the daemon side; can be developed in parallel with US1/US2 by another agent in its own worktree.
4. Polish and the full quickstart in a preview before opening the PR.
