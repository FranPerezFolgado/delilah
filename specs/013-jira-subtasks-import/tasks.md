# Tasks: Jira sub-tasks become Delilah subtasks

**Input**: Design documents from `specs/013-jira-subtasks-import/`
**Prerequisites**: plan.md, spec.md, research.md (R1–R10), data-model.md, contracts/api.md, quickstart.md

**Tests**: included. Implementation is orchestrated test-first; each test task is written
before the code it covers and must fail first.

**Naming**: always "parent task" in code and copy (`TaskKind::ParentTask`, `parent_task`), never
bare "parent" (that is a parent repository, `ProjectKind::Parent`). A Jira "sub-task" is
Jira's issue; a "subtask" is the Delilah task.

**Checks**: `cargo test -p <crate>`; UI `cd app && npx tsc --noEmit && npx vitest related --run <files>`.
Try changes only with `scripts/preview.sh` (never port 7420). Never write to Jira (FR-013).

## Format: `[ID] [P?] [Story] Description`

- **[P]**: can run in parallel (different files, no dependency on an unfinished task)
- **[Story]**: US1–US4 from spec.md

---

## Phase 1: Setup

- [X] T001 Add the empty module `crates/delilah-daemon/src/jira_import.rs` and declare it where the daemon declares modules (`crates/delilah-daemon/src/lib.rs` or `main.rs`); add the empty submodule `crates/delilah-daemon/src/store/jira_skips.rs` declared from `crates/delilah-daemon/src/store.rs` (`mod jira_skips;`, an `impl Store` block)
- [X] T002 Create `crates/delilah-daemon/migrations/0026_jira_subtask_skips.sql`: table `jira_subtask_skips(parent_task TEXT NOT NULL, jira_key TEXT NOT NULL, created_at TEXT NOT NULL, PRIMARY KEY (parent_task, jira_key))`, no foreign key, with a comment above as in 0025 (data-model §Stored)

---

## Phase 2: Foundational (blocks every story)

### Jira client

- [X] T003 [P] Write tests in `crates/delilah-integrations/src/jira.rs` (`#[cfg(test)]`): `my_subtask_keys` on the sample client returns exactly the sub-tasks of a ticket assigned to `SAMPLE_ME`; a bad key (`"X; DROP"`) is `JiraError::BadKey`; the sample data has one ticket with an open unassigned, an open `SAMPLE_ME`, an open someone-else's and a done sub-task (assert by key)
- [X] T004 Implement `JiraClient::my_subtask_keys(parent) -> Result<HashSet<String>, JiraError>` in `crates/delilah-integrations/src/jira.rs` (research R4): `is_issue_key` check as `subtasks()` (~L507); Remote: JQL `parent = KEY AND assignee = currentUser()` with only the `key` field; Sample: filter by `parent.key == KEY && assignee == SAMPLE_ME`. Extend the sample data (add sub-tasks to an existing sample ticket) so T003's fixture ticket exists, until T003 passes

### Store primitives (no Jira policy)

- [X] T005 [P] Write tests in `crates/delilah-daemon/src/store.rs` test module: `become_parent_task(&pool, key)` sets `kind = parent_task` and clears `workflow` (→ `none`), `saving_mode`, `autopilot`, `ci_autofix`, `handoff`; `set_kind_tx` on a transaction is rolled back with it; `tasks_by_keys(&[..])` returns only existing tasks, any kind, archived included
- [X] T006 Implement in `crates/delilah-daemon/src/store.rs`: `pub(crate) async fn become_parent_task<'e, E: Executor<'e, Database = Sqlite>>(e, key)` holding the `UPDATE tasks SET workflow = 'none', saving_mode = 0, autopilot = NULL, ci_autofix = 0, handoff = NULL …` now in `api_hierarchy.rs:131` plus the kind change; `set_kind_tx(e, key, kind, parent_task)` with `set_kind` (~L506) delegating to it; `tasks_by_keys(keys) -> Vec<Task>`; a `begin_immediate()` helper returning the transaction (pattern of `create_task_from_conversation`, ~L86), until T005 passes
- [X] T007 Change `make_parent_task` in `crates/delilah-daemon/src/api_hierarchy.rs` (~L107–135) to call `store::become_parent_task` instead of its inline `UPDATE` + `set_kind`; its existing tests must pass unchanged
- [X] T008 [P] Write tests in `crates/delilah-daemon/src/store/jira_skips.rs`: `add_jira_skips(e, parent, keys)` is idempotent, `remove_jira_skips(e, parent, keys)`, `jira_skips_of(parent)` sorted, `delete_jira_skips(parent)` only touches that parent task
- [X] T009 Implement the four functions in `crates/delilah-daemon/src/store/jira_skips.rs` (`INSERT OR IGNORE`, executor-taking writes) until T008 passes
- [X] T010 In `crates/delilah-daemon/src/actions.rs` (~L179, next to `clear_parent_task_links`) call `delete_jira_skips` when a parent task is deleted; test there that deleting a parent task removes its skips and archiving keeps them

### Task constructor shared with `POST /tasks`

- [X] T011 Write tests in `crates/delilah-daemon/src/api.rs` test module: a pure `new_task(NewTaskSpec { key, title, kind, parent_task, project }, &Defaults)` gives the same workflow, saving mode, CI autofix, model and in-checkout values as `POST /tasks` for a normal Jira task, a parent task and a subtask (compare with tasks created through the route)
- [X] T012 Split `build_task_with` in `crates/delilah-daemon/src/api.rs` (~L556–600) into async `resolve_new_task` (project lookup, free key, parent task lookup, `check_new_subtask`, project defaults) and pure `new_task`; `create_task` calls both; every existing `api.rs`/`api_hierarchy.rs` test passes; T011 passes

### Planner (pure)

- [X] T013 Write table tests in `crates/delilah-daemon/src/jira_import.rs` for `plan_import` (data-model §ImportPlan): parent `create` / `convert` / `existing` / `refused` (each spec 012 refusal: subtask, open PR, running session, archived) and a ticket that is a Jira sub-task → `refused(nesting)`; per sub-task `create`, `attach` (normal, never started, same project or none), `linked` (subtask of this parent task, live or archived), `conflict` (started, other project, other parent task, a parent task); `selected` for done / someone else's / mine / unassigned / skipped / conflict; `gone` = Delilah subtasks of the parent task whose key is not among the Jira sub-tasks
- [X] T014 Implement `ImportPlan`, `ParentAction`, `SubtaskRow`, `SubtaskAction` (serde as contracts/api.md, `snake_case` actions) and `plan_import(...)` in `crates/delilah-daemon/src/jira_import.rs`, calling `delilah_core::hierarchy::{check_make_parent_task, check_attach}` (no copy of the rules), until T013 passes

**Checkpoint**: client, store primitives, constructor and planner tested; nothing user-facing yet.

---

## Phase 3: User Story 1 — Turn a Jira ticket into a parent task with subtasks (P1) 🎯 MVP

**Goal**: From a Jira ticket with sub-tasks, create the parent task and the chosen subtasks in one action (API, UI, CLI).

**Independent Test**: With sample Jira data, run the import on the T003 fixture ticket with the default selection; the parent task and exactly the open mine-or-unassigned subtasks exist, keyed and titled as in Jira, in the backlog.

### Tests

- [ ] T015 [P] [US1] Handler tests in `crates/delilah-daemon/src/jira_import.rs` with the sample client (pattern of `crates/delilah-daemon/src/jira.rs` tests ~L349): `GET /api/v1/jira/issues/{key}/import` on the fixture ticket (rows, `selected`, `parent.action = create`); bad key → 400 `bad-key`; Jira not connected → 200 `linked:false, reason:not-connected` without a Jira call; `POST` with `project` + `add` creates the parent task (`kind parent_task`, workflow `none`) and the subtasks (`kind subtask`, `parent_task`, parent task's project, backlog, no workspace); missing `project` → 400 `project-required`; a key in `add` that is not a sub-task of the ticket → 400 `not-a-subtask` and nothing written; `Topic::Tasks` emitted once
- [ ] T016 [P] [US1] Test in `crates/delilah-daemon/src/jira.rs` that `write_ticket` writes `Parent: KEY title` for an issue with `parent` (title from `JiraParent.summary`, key only when absent) and no such line without one
- [ ] T017 [P] [US1] Daemon test (in `crates/delilah-daemon/tests/parent_tasks.rs`, fixture parent repo) that a subtask created by the import and started after its parent task gets the parent task's submodules (FR-005, research R8: assert existing behaviour, no new code expected)
- [X] T018 [P] [US1] `app/src/components/JiraImportDialog.test.tsx`: create mode lists rows with key, title, Jira status, assignee and the tags "done in Jira" / "assigned to X" / "already a task"; default ticks follow `selected`; toggling changes the request's `add`; project picker required; the result shows `left_out` with reasons; errors (`unreachable`) show retry
- [X] T019 [P] [US1] `app/src/components/JiraView.test.tsx` (create if missing): "Create parent task" shows only for a ticket with `subtask_count > 0` that is not `subtask`, and opens the dialog

### Implementation

- [ ] T020 [US1] Implement `preview(app, key)` in `crates/delilah-daemon/src/jira_import.rs`: `is_issue_key` and "Jira configured" short-circuits (→ `linked:false`), Jira `issue` + `subtasks` + `my_subtask_keys`, store `tasks_by_keys`, `jira_skips_of`, `integration::subtask_facts` of an existing parent task, session/open-PR facts as `api_hierarchy::make_parent_task` reads them → `plan_import`; Jira not-found/errors/timeouts → `linked:false` with `reason`
- [ ] T021 [US1] Implement `apply_jira_import(app, plan, request)` in `crates/delilah-daemon/src/jira_import.rs`: build `Task` values with `api::new_task` (project resolved once, before the transaction; subtasks of a parent task created in this call skip `check_new_subtask`), then in one `begin_immediate()` transaction `insert_task` the parent task, `insert_task` each created subtask, `set_kind_tx` each attach; commit; return `ImportResult` (data-model)
- [ ] T022 [US1] Add `GET`/`POST /api/v1/jira/issues/{key}/import` handlers in `crates/delilah-daemon/src/jira_import.rs` (POST re-runs `preview`, validates `project` and keys, calls `apply_jira_import`, emits `Topic::Tasks` only if something was created/converted/attached) and their routes in `crates/delilah-daemon/src/api.rs` next to `/api/v1/jira/issues/{key}/subtasks` (~L161), until T015 passes
- [ ] T023 [US1] Add the `Parent:` line to `write_ticket` in `crates/delilah-daemon/src/jira.rs` (~L218) until T016 passes
- [X] T024 [P] [US1] Add types `JiraImportPreview`, `JiraImportRow`, `JiraImportRequest`, `JiraImportResult` and functions `jiraImportPreview(key)`, `jiraImport(key, req)` to `app/src/api.ts` (contracts/api.md), with a case in `app/src/api.test.ts`
- [X] T025 [US1] Create `app/src/components/JiraImportDialog.tsx` in create mode plus `app/src/i18n/en/JiraImportDialog.ts` and its Spanish counterpart (follow `NewTaskDialog.tsx` and its i18n files; register in `app/src/i18n/en/en.ts` and the es index), until T018 passes
- [X] T026 [US1] Add "Create parent task" to the ticket in `app/src/components/JiraView.tsx` (near the sub-task listing ~L404–494) and its strings in `app/src/i18n/en/JiraView.ts` (+ es), until T019 passes
- [ ] T027 [US1] Add `delilah jira import <KEY> [--project] [--defaults | --add K,..]` to `crates/delilah-cli/src/main.rs` (contracts §CLI): preview table when no write flag, result lines and exit code 1 on refusal; parsing and printing tests in the CLI's existing test style

**Checkpoint**: US1 works end to end on sample data (quickstart steps 1–2, 5–6).

---

## Phase 4: User Story 2 — Reuse tasks that already exist (P1)

**Goal**: An existing normal task with the ticket key becomes the parent task; existing tasks with sub-task keys are attached or reported; all or nothing.

**Independent Test**: Normal backlog task with the ticket key + one with a sub-task key → import: the first is the parent task (workflow `none`), the second its subtask, the rest created.

### Tests

- [ ] T028 [P] [US2] Handler tests in `crates/delilah-daemon/src/jira_import.rs`: convert (normal task with the ticket key → parent task, keeps status, agent settings cleared); existing parent task → only adds; refused (ticket task is a subtask / has an open PR / a running session / archived) → 409 with the refusal code and nothing written; attach of a never-started normal task in the same project; `left_out` for started / other project / other parent task with reasons, the rest created; second run on the same ticket creates nothing (SC-003)
- [ ] T029 [P] [US2] Transaction tests in `crates/delilah-daemon/src/jira_import.rs`: a failing subtask insert after a convert rolls back the convert too (nothing converted, nothing created); a session started on the ticket task between preview and write → 409, nothing written; an attach target started between preview and write → `left_out` (`started`); a key taken meanwhile → `left_out` (`taken`)
- [X] T030 [P] [US2] `app/src/components/JiraImportDialog.test.tsx`: the parent line says "becomes the parent task" for `convert`, "adds to the parent task" for `existing`, and shows the refusal and disables confirm for `refused`; `attach` rows say "already a task, will be attached"; `conflict` rows are unticked and disabled with the reason

### Implementation

- [ ] T031 [US2] In `apply_jira_import` (`crates/delilah-daemon/src/jira_import.rs`) add the convert path (`store::become_parent_task` on the transaction) and the in-transaction re-checks (ticket task kind, open PR, live session; each attach target still normal, never started, same project; created keys still free → else `left_out`), until T028 and T029 pass
- [X] T032 [US2] Render `convert` / `existing` / `refused` and `attach` / `conflict` in `app/src/components/JiraImportDialog.tsx` (+ i18n en/es), until T030 passes

**Checkpoint**: US1 + US2 cover every first import.

---

## Phase 5: User Story 3 — Bring in sub-tasks added in Jira later (P2)

**Goal**: The parent task screen offers new Jira sub-tasks on open; the user adds or skips them; skips persist and can be undone. Same from the CLI.

**Independent Test**: After an import, add a sub-task to the sample data, open the parent task: "1 new Jira sub-task · Add"; add it; skip another, reopen: not offered, listed under "Skipped" with Undo.

### Tests

- [ ] T033 [P] [US3] Handler tests in `crates/delilah-daemon/src/jira_import.rs`: `POST` with `skip` writes skips and the next preview marks them `skipped`, `selected:false`; `unskip` reverses it; skip/unskip without an existing or created parent task → 400; an empty `add` is valid; skip-only writes emit no `Topic::Tasks`; a hand-made parent task whose key is a sample ticket gets the offer (US3-5); a `LIBRE-n` parent task → `linked:false, not-found` without a Jira call; a deleted subtask's Jira sub-task is offered again, an archived one is `linked`
- [X] T034 [P] [US3] `app/src/components/SubtaskList.test.tsx`: the banner "N new Jira sub-tasks · Add" counts selectable non-skipped `create`/`attach` rows, hides at 0; it renders the subtask list before the preview resolves (never waits); `not-connected` shows a quiet note, `not-found`/`unreachable` show nothing; "Skipped (n)" lists skips with Undo; "Add" opens `JiraImportDialog` in add mode
- [X] T035 [P] [US3] `app/src/components/JiraImportDialog.test.tsx`: add mode lists only `create`/`attach` rows plus the skipped ones with Undo; a per-row "Skip" goes to the request's `skip`

### Implementation

- [ ] T036 [US3] Add skip/unskip to `apply_jira_import` and the `POST` validation in `crates/delilah-daemon/src/jira_import.rs` (`add_jira_skips` / `remove_jira_skips` on the same transaction), until T033 passes
- [X] T037 [US3] Add add mode and per-row Skip/Undo to `app/src/components/JiraImportDialog.tsx` (+ i18n), until T035 passes
- [X] T038 [US3] Load `jiraImportPreview(parentTask.key)` on mount (and on a "Check Jira" action) in `app/src/components/SubtaskList.tsx` without blocking the list; banner, quiet note and "Skipped (n)" (+ `app/src/i18n/en/SubtaskList.ts` and es), until T034 passes
- [ ] T039 [US3] Add `--skip K,..` and `--unskip K,..` to `delilah jira import` in `crates/delilah-cli/src/main.rs`; on a parent task the preview shows only new and skipped rows; tests in the CLI's style

**Checkpoint**: both sides stay in step on demand.

---

## Phase 6: User Story 4 — See how the subtasks stand in Jira (P3)

**Goal**: Each subtask in the parent task's list shows its Jira status; subtasks no longer under the ticket are marked.

**Independent Test**: With a parent task from Jira, the list shows each subtask's Jira status; removing a sub-task from the sample data marks that subtask "not in Jira any more" and leaves it as it is.

- [ ] T040 [P] [US4] `app/src/components/SubtaskList.test.tsx`: Jira status next to each subtask whose key is in the preview's `subtasks` (`linked` rows); "not in Jira any more" for keys in `gone`; nothing when `linked:false`
- [ ] T041 [P] [US4] Handler test in `crates/delilah-daemon/src/jira_import.rs`: a subtask whose Jira sub-task moved to another ticket is in `gone` and is neither archived nor changed
- [ ] T042 [US4] Render the Jira status and the `gone` mark in `app/src/components/SubtaskList.tsx` (+ i18n), until T040 and T041 pass

---

## Phase 7: Polish & docs

- [ ] T043 [P] `docs/usage.md`: under "Parent tasks and subtasks" and "Jira", the "Create parent task" flow, default ticks (done / someone else's unticked), reuse of existing tasks, the offer on the parent task screen, skips, Jira status, one-way sync, and `delilah jira import`
- [ ] T044 [P] `docs/status.md`: drop "Not yet: the Jira adapter" from the parent tasks entry and add this feature (spec 013); add it under **Jira**
- [ ] T045 [P] `docs/decisions.md`: new decision (next number after 102) — Jira → Delilah only, pull on open, never background, never writes to Jira; default selection rules; reuse via convert/attach with in-transaction re-checks; skips table (0026); no stored Jira link (the key is the link); ruled out: background polling, writing to Jira, syncing titles, a stored link column
- [ ] T046 [P] `docs/architecture.md`: `jira_import.rs`, `store/jira_skips.rs`, migration 0026, the two routes
- [ ] T047 Run `cargo test --workspace`, `cargo test --manifest-path app/src-tauri/Cargo.toml`, `cd app && npx tsc --noEmit && npx vitest run`; walk through `specs/013-jira-subtasks-import/quickstart.md` in `scripts/preview.sh --fresh`

---

## Dependencies & execution order

- **Phase 1** → **Phase 2** → stories.
- In Phase 2: T003→T004; T005→T006→T007; T008→T009→T010; T011→T012; T013→T014 (T014 needs T006's types only for inputs, not the store). The five chains touch different files and run in parallel, except T007 and T012 which edit `api_hierarchy.rs` and `api.rs` respectively (no overlap).
- **US1** needs all of Phase 2. **US2** builds on US1's `apply_jira_import` (T021) and dialog (T025). **US3** builds on US1 (endpoint, dialog) and T009. **US4** needs only US1's preview (T020) and can run alongside US2/US3 except for `SubtaskList.tsx`, shared with US3 (do T042 after T038).
- Shared files: `jira_import.rs` (T015, T020–T022, T028–T029, T031, T033, T036, T041 — one writer at a time), `JiraImportDialog.tsx` (T025, T032, T037), `SubtaskList.tsx` (T038, T042), `crates/delilah-cli/src/main.rs` (T027, T039).

### Parallel examples

- Phase 2: T003, T005, T008, T011, T013 together.
- US1 tests: T015, T016, T017, T018, T019 together; then daemon (T020–T023) alongside UI (T024–T026) and CLI (T027).
- Polish: T043–T046 together.

## Implementation strategy

- **MVP**: Phases 1–3 (US1): a ticket becomes a parent task with subtasks on a clean slate.
- **Next**: US2 (the common case of an existing ticket task), then US3 (sync), then US4.
- Each checkpoint is shippable on its own; docs land with the last phase.
