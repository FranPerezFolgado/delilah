# Tasks: Saving mode (one fresh session per phase)

**Input**: Design documents from `specs/004-saving-mode/`
**Prerequisites**: plan.md, spec.md, research.md, data-model.md, contracts/api.md, quickstart.md

**Tests**: test-first. Write each test task before its implementation and see it fail.

**Checks**: `cargo test -p <crate>`; `cd app && npx tsc --noEmit && npx vitest related --run <files>`; at the end `cargo test --workspace`, `cargo test --manifest-path app/src-tauri/Cargo.toml` and `npx vitest run`.

## Format: `[ID] [P?] [Story] Description`

## Phase 1: Setup

None: no new dependencies.

---

## Phase 2: Foundational (blocks every story)

**Purpose**: the types, the storage and the one-primary invariant.

- [ ] T001 Write failing tests in `crates/delilah-core/src/task.rs`: `Step` (`plan`/`implement`/`review`, serde lowercase, `parse`, `next`), `Handoff { to, stage, error }` round-trips as `{"to":"implement","stage":"offered"}`, `HandoffStage` serde lowercase; `Event::StepStarted { step, session }` leaves every status unchanged (like `PhaseReached`); `Task` deserialises without `saving_mode`/`handoff` (defaults `false`/`None`).
- [ ] T002 Implement T001 in `crates/delilah-core/src/task.rs` (and its transition in the same crate's state machine): `Task.saving_mode: bool` (`#[serde(default)]`), `Task.handoff: Option<Handoff>`, `Task::new` defaults.
- [ ] T003 Add `crates/delilah-daemon/migrations/0011_saving_mode.sql` exactly as in data-model.md (`tasks.saving_mode`, `tasks.handoff`, `sessions.step`).
- [ ] T004 Write failing store tests in `crates/delilah-daemon/src/store.rs` (tests module): a created task reads back `saving_mode` and `handoff`; `set_saving_mode(key, on)` and `set_handoff(key, Option<&Handoff>)` persist; `StepStarted` is listed in the task's history.
- [ ] T005 Implement T004 in `crates/delilah-daemon/src/store.rs`: select/insert the new columns in every task query, `set_saving_mode`, `set_handoff` (JSON text or NULL).
- [ ] T006 Write failing tests in `crates/delilah-daemon/src/store_sessions.rs`: `SessionRecord.step` persists; `session_of_step(task, step)` finds it; `hand_over(task, from_id, new_record)` leaves exactly one `is_primary = 1` (the new one) and records `StepStarted`; calling it again for the same step returns the existing session and creates nothing; `undo_hand_over(task, from_id, new_id)` restores the old primary and marks the new record closed.
- [ ] T007 Implement T006 in `crates/delilah-daemon/src/store_sessions.rs` (both in one SQL transaction each) and add `step: None` to every existing `SessionRecord` literal (`run.rs`, `session_api.rs`, tests).

**Checkpoint**: `cargo test -p delilah-core && cargo test -p delilah-daemon` green; no behaviour change.

---

## Phase 3: User Story 1 — Implement in a fresh session after planning (P1) 🎯 MVP

**Goal**: a saving-mode task's first session stops after the tasks; confirming in the Bandeja starts "Implementación" as the main session; declining keeps working in place.

**Independent Test**: on a temp workspace with a fake Claude PTY (as in `session_api.rs`/`scheduler.rs` tests): create a saving-mode task, write `specs/<dir>/tasks.md` with unchecked items, send a primary `Stop` hook → handoff `offered` and an inbox `handoff` item; `POST /handoff {confirm}` (save-notes off) → a new primary session with `step = implement` launched with `/orchestrate specs/<dir>`, the old one stopped, not closed, not primary.

- [ ] T008 [US1] Write failing tests in `crates/delilah-daemon/src/run.rs` (tests): `RunRequest.saving_mode = true` stores it and the first session gets `step = plan`, label "Spec y plan" and a prompt ending with the saving-mode note (Spanish and English); absent → reads preference `saving-mode` (default false); false → label "Principal", `step = None`, prompt unchanged.
- [ ] T009 [US1] Implement T008 in `crates/delilah-daemon/src/run.rs` (and keep it for backlog tasks the way `in_checkout` is kept).
- [ ] T010 [US1] Write failing tests for `crates/delilah-daemon/src/handoff.rs` boundaries: `boundary(task, primary_step, spec)` → `Offer(implement)` for step plan with total > 0 and done == 0; `None` without saving mode, without spec dir, with `stage = skipped` for implement, or with a pending handoff; the furthest step when both boundaries are reached (all checked from plan → `Start(review)`).
- [ ] T011 [US1] Write failing tests for the handoff engine in `crates/delilah-daemon/src/handoff.rs`: `on_turn_ended` sets `offered` and emits the inbox topic; `confirm` with save-notes off → `starting` → new session launched via `launch_session` with `/orchestrate specs/<dir>` in the user's language, old PTY killed, handoff cleared; `decline` → handoff cleared and `saving_mode = false`; `confirm` twice → second answers 409 and no second session; a launch failure → `undo_hand_over`, `stage = failed` with the error, `retry` → starts again.
- [ ] T012 [US1] Implement `crates/delilah-daemon/src/handoff.rs` for T010–T011: `boundary`, `on_turn_ended(app, key)`, `confirm/decline/retry`, `start(app, key, step)` (hand_over → kill old → launch → or undo), the step prompts from research.md R4 (`lang.pick`); register `mod handoff` in `crates/delilah-daemon/src/lib.rs`.
- [ ] T013 [US1] Write failing tests in `crates/delilah-daemon/src/phases.rs`: a primary not-limited `TurnEnded` calls the handoff check; a code `Edit` from the session whose step is `plan` sets `{to: implement, stage: skipped}`; `POST /tasks/{key}/phase` forward to implement on a saving-mode task offers the handoff; backwards never touches sessions.
- [ ] T014 [US1] Implement T013 in `crates/delilah-daemon/src/phases.rs` (`on_hook`, `set`), calling into `handoff.rs`.
- [ ] T015 [US1] Write failing tests in `crates/delilah-daemon/src/api.rs` routes: `POST /api/v1/tasks/{key}/handoff {action}` (200/404/409, unknown action 400) and `POST /api/v1/tasks/{key}/saving-mode {on}` (off clears a pending handoff); then add both routes (handlers in `handoff.rs`).
- [ ] T016 [US1] Write failing tests in `crates/delilah-daemon/src/inbox.rs`: a task with `handoff.stage = offered` yields one `handoff` item (text from contracts/api.md, session = outgoing primary), ordered after `review` and before `suggest-review`; `failed` yields one with the error; none for `skipped` or without handoff. Then implement `ItemKind::Handoff`.
- [ ] T017 [US1] Write failing tests in `crates/delilah-daemon/src/scheduler.rs`: recovery on tick — `starting` with no live session for that step → starts it once; `starting` whose step session already exists → just clears the handoff; `saving` with the outgoing PTY gone → `offered` (implement). Then implement `handoff::recover` and call it from `tick`.
- [ ] T018 [P] [US1] CLI: `--saving-mode` / `--no-saving-mode` on `new` and `run` in `crates/delilah-cli/src/main.rs` (sends `saving_mode`), with a parse test.
- [ ] T019 [US1] `app/src/api.ts`: `Step`, `HandoffStage`, `Handoff`, `Task.saving_mode`, `Task.handoff`, `Session.step`, inbox kind `"handoff"`, `RunRequest.saving_mode`, `api.handoff(key, action)`, `api.setSavingMode(key, on)` (contracts/api.md).
- [ ] T020 [US1] Write failing tests then add the "Modo ahorro" checkbox to `app/src/components/NewTaskDialog.tsx` (preloaded from preference `saving-mode`, sent as `saving_mode`) and to `app/src/components/BacklogTaskView.tsx`; tests in a new `app/src/components/NewTaskDialog.test.tsx`.
- [ ] T021 [US1] Write failing tests then render the `handoff` item in `app/src/components/InboxView.tsx`: `offered` → "Implementar en sesión nueva" (confirm) and "Seguir aquí" (decline); `failed` → error + "Reintentar"; buttons disabled while the request runs; tests in `app/src/components/InboxView.test.tsx`.
- [ ] T022 [P] [US1] English strings for every new Spanish text in `app/src/i18n/en.ts`.

**Checkpoint**: US1 works end to end in a preview (quickstart steps 1–5 without the chain view).

---

## Phase 4: User Story 2 — See the chain of sessions (P1)

**Goal**: the task screen shows the chain and the saving-mode marker; after a handoff the new main session's tab is shown.

**Independent Test**: `chainOf` unit tests and `SessionChain`/`TaskScreen` component tests with tasks in each stage.

- [ ] T023 [P] [US2] Write failing tests in `app/src/chain.test.ts` for `chainOf(task, sessions)`: null without saving mode; three steps in order with labels; the primary's step `active`, earlier steps with a session `done`, later `pending`; a step with `handoff.to` and stage `offered`/`saving`/`starting` → `waiting`; `failed` → `waiting` with error; skipped implement → the plan step stays active.
- [ ] T024 [US2] Implement `app/src/chain.ts` for T023.
- [ ] T025 [US2] Write failing tests then implement `app/src/components/SessionChain.tsx` (+ `SessionChain.test.tsx`): ✓/●/○/⏳ per step with accessible labels ("Spec y plan, terminada"), done steps' "Abrir" selects their tab (or "Reabrir" when stopped), active step selects its tab.
- [ ] T026 [US2] In `app/src/components/TaskScreen.tsx`: render `SessionChain` above the session tabs and a "Modo ahorro" marker next to `PhaseBadge`; when the primary session changes to a new one, select its tab; extend `app/src/components/TaskScreen.test.tsx` (marker and chain only with saving mode).
- [ ] T027 [P] [US2] Styles for the chain and the marker in `app/src/styles.css`, readable at 1024 px wide.

**Checkpoint**: quickstart step 3 and 5 visible in a preview.

---

## Phase 5: User Story 3 — Keep the session notes before closing a session (P2)

**Goal**: with `save-notes` on, the outgoing session runs `/save-session` before the handoff.

**Independent Test**: engine tests with a fake PTY reading what was typed.

- [ ] T028 [US3] Write failing tests in `crates/delilah-daemon/src/handoff.rs`: with preference `save-notes` absent or true, `confirm` → `saving` and `/save-session` typed into the outgoing primary; its next not-limited `TurnEnded` → the new session starts; a limited turn end keeps `saving` (and the task pauses as today); a `TurnFailed` without limit → `failed`; with `save-notes = false` → straight to `starting`.
- [ ] T029 [US3] Implement T028 in `crates/delilah-daemon/src/handoff.rs` (uses `actions::type_prompt`).
- [ ] T030 [US3] Settings: "Modo ahorro por defecto" (`saving-mode`) and "Guardar notas antes de cambiar de sesión" (`save-notes`, default on) in `app/src/components/SettingsView.tsx`, with English strings in `app/src/i18n/en.ts`.

---

## Phase 6: User Story 4 — Review and commit in a third session (P2)

**Goal**: when every task is checked, "Revisión" starts on its own, announced to the user.

**Independent Test**: engine test: implement-step primary turn ends with all tasks checked → review session launched with the review prompt, notification recorded.

- [ ] T031 [US4] Write failing tests in `crates/delilah-daemon/src/handoff.rs`: implement step + all checked + `TurnEnded` → `Start(review)` without offer (through `saving` when save-notes is on); the review prompt asks to review, fix the confirmed findings, update stale docs and commit, and forbids push and PR (both languages); a notification is sent (`notify` recorded) and an inbox `handoff` item shows while `saving`/`starting`.
- [ ] T032 [US4] Implement T031 in `crates/delilah-daemon/src/handoff.rs` and `crates/delilah-daemon/src/inbox.rs`.

---

## Phase 7: Polish & cross-cutting

- [ ] T033 [P] `docs/architecture.md`: sessions section (step, chain, primary switch) and a "Saving mode" subsection (boundaries, stages, recovery); the hook→effect table row for `TurnEnded`.
- [ ] T034 [P] `docs/decisions.md`: new decision "Saving mode: a fresh session per phase" (why, measured numbers, what was ruled out: returning to the parent, cutting inside orchestrate for now), noting it relaxes decision 15.
- [ ] T035 [P] `docs/status.md` (built; pending: one session per orchestrate phase) and `README.md` (the option, the chain, the settings).
- [ ] T036 Run every check (`cargo test --workspace`, `cargo test --manifest-path app/src-tauri/Cargo.toml`, `cd app && npx tsc --noEmit && npx vitest run`) and walk quickstart.md in a preview.

---

## Dependencies & Execution Order

- Phase 2 blocks everything (T001→T002; T003→T004→T005; T005→T006→T007).
- US1 (T008–T022) needs Phase 2. Inside it: T010–T012 before T013–T017; T019 before T020–T021.
- US2 needs T019 (types) and T012 (the data it shows); independent of US3/US4.
- US3 and US4 need T012; they touch `handoff.rs`, so run them one after the other.
- Polish after the stories.

## Parallel Opportunities

- T018 (CLI) and T022 (strings) alongside the rest of US1.
- The UI of US1 (T019–T021) alongside the daemon part (T013–T017) once T012 is done.
- US2 (app only) alongside US3/US4 (daemon only).
- T033–T035 together.

## Implementation Strategy

1. **MVP**: Phase 2 + US1 — one cut (planning → implementation), ~37 % of the saving.
2. US2: the chain, which makes it understandable.
3. US3 + US4: notes before leaving and the second cut (~49 %).
4. Docs and the full check.
