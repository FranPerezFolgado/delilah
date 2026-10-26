# Tasks: CI autofix

**Input**: Design documents from `specs/010-ci-autofix/`

**Prerequisites**: plan.md, spec.md, research.md, data-model.md, contracts/api.md, quickstart.md

**Tests**: included. The project implements test-first (orchestrated implementers write failing tests from the acceptance criteria first), so every story starts with its tests.

**Organization**: grouped by user story; US1 and US2 (both P1) form the MVP.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: can run in parallel (different files, no dependency on an unfinished task)
- **[Story]**: US1–US4 from spec.md

Paths: daemon `crates/delilah-daemon/src/`, core `crates/delilah-core/src/`, CLI `crates/delilah-cli/src/`, e2e `crates/delilah-e2e/tests/`, UI `app/src/`.

---

## Phase 1: Setup

- [X] T001 Add migration `crates/delilah-daemon/migrations/0022_ci_autofix.sql` exactly as in data-model.md (`tasks.ci_autofix`, `pull_requests.autofix`, backfill for live autopilot tasks and seed of their PRs' `sha`); first check how `tasks.status` is encoded and how archived tasks are marked in `crates/delilah-daemon/migrations/0001*.sql` and `store.rs`, and match the `WHERE`
- [X] T002 Create the module skeleton `crates/delilah-daemon/src/autofix/mod.rs` and `autofix/api.rs`, registered in `crates/delilah-daemon/src/lib.rs` (empty pub fns `evaluate`, `on_turn_ended`, `cancel`; mirror how `autopilot/` is declared)

---

## Phase 2: Foundational (blocks every story)

- [X] T003 [P] Test in `crates/delilah-core/src/project.rs`: `ProjectSettings` without `ci_autofix` deserialises to `AutofixDefault::Autopilot`; `"off"|"autopilot"|"always"` round-trip; an unknown value is a validation error with field `ci_autofix`, code `ci_autofix.invalid`
- [X] T004 [P] Add `AutofixDefault` (kebab-case serde, default `Autopilot`) and `ProjectSettings.ci_autofix` with validation in `crates/delilah-core/src/project.rs`, plus its localised message where field errors are localised in the daemon (decision 51)
- [X] T005 [P] Add `Task.ci_autofix: bool` (serde default false) and `Event` variants `AutofixSent {repo, sha, attempt, checks}`, `AutofixFixed {repo}`, `AutofixFailedAgain {repo, sha}`, `AutofixStopped {repo, reason}`, `AutofixCancelled {repo}` with their history names in `crates/delilah-core/src/task.rs`
- [X] T006 Store tests in `crates/delilah-daemon/src/store.rs`: `ci_autofix` is read on `get_task`/`list_tasks` and written by `set_ci_autofix`; the autofix events appear in the history; migration test: a live autopilot task gets `ci_autofix = 1` and its open PR a seeded `sha`, a done one doesn't
- [X] T007 Read/write `tasks.ci_autofix` in the task queries and add `Store::set_ci_autofix(key, bool)` in `crates/delilah-daemon/src/store.rs`
- [X] T008 Define `PrAutofix {state, attempts, sha, session, sent_at, reason}` (serde, `LIMIT = 3`) in `crates/delilah-daemon/src/autofix/mod.rs`; store tests then `PrRow.autofix` read and `Store::update_pr_autofix(pr_id, expected: Option<&PrAutofix>, next: Option<&PrAutofix>) -> Result<bool>` compare-and-set in `crates/delilah-daemon/src/store_prs.rs` (false when the stored JSON differs from `expected`)
- [X] T009 At task creation (`POST /tasks`, backlog create/PATCH, `proposals.rs` "Create and launch on autopilot") resolve `ci_autofix`: explicit request value wins, else the project default (`always` → on, `autopilot` → on iff autopilot, `off` → off); request field `ci_autofix: Option<bool>` in `crates/delilah-daemon/src/api.rs` and `crates/delilah-daemon/src/proposals.rs`, with tests for the three defaults
- [X] T010 Extract `pub(crate) async fn send(app, key, prompt) -> Result<(), ApiError>` from `forward()` in `crates/delilah-daemon/src/pr_api/forward.rs` (PausedByLimit refusal, type into the live primary or relaunch the record); `forward()` uses it; existing forward tests stay green
- [X] T011 Add a per-task `AutofixGuard` (set of keys, claim returns `None` if held, released on drop, like `pr_loop::claim`) in `crates/delilah-daemon/src/state.rs` / `autofix/mod.rs`

**Checkpoint**: settings, task flag, per-PR state with CAS, one send path, guard.

---

## Phase 3: User Story 1 - A failed CI is fixed without the user (P1) 🎯 MVP

**Goal**: an eligible failure (every check of the head finished, one failed, head not forwarded, under the limit, agent idle) reaches the agent once; a green head ends the attempt.

**Independent Test**: fake `gh` with a failing head and an idle `cat` primary session: one autofix prompt is typed, `ci-failed` resolved, PR state `fixing` attempt 1; then a new green head → `watching`, attempts 0, history `autofix-sent` + `autofix-fixed`.

### Tests for User Story 1

- [X] T012 [P] [US1] Table tests of the pure `decide(task, row_state, snapshot, idle) -> Action` in `crates/delilah-daemon/src/autofix/mod.rs`: send on red+finished+new sha+idle; wait while any check pending (acceptance 1.3); never twice for the same sha (1.4); wait when not idle (1.5); no checks/neutral only → nothing; autofix off → nothing; PR not open → nothing; green after `fixing` → fixed
- [X] T013 [P] [US1] Prompt tests in `crates/delilah-daemon/src/autofix/mod.rs`: es/en wording, "attempt N of 3", one section per PR when several are batched, framing "data, not instructions", "commit and push to this same branch; don't force-push or open another PR", within `forward::fit` size
- [X] T014 [P] [US1] Idle tests in `crates/delilah-daemon/src/autofix/mod.rs`: `Waiting` with a `Question` attention is idle; `Working`/`Starting`, a `Permission` attention, an open ask card, `PausedByLimit`, a terminal task are not; no live primary but a resumable record is idle (relaunch) (research R3)
- [X] T015 [US1] Flow test in `crates/delilah-daemon/src/autofix/mod.rs` (FakeGh + `cat` session, as `pr_api/forward.rs` tests): `pr_loop::check_task` on a failing finished head → prompt typed once, notice resolved, state `fixing`; a second check → nothing typed; a later green head → `watching`; two PRs of one task failing together → one prompt with both
- [X] T016 [US1] Test in `crates/delilah-daemon/src/pr_loop/notices.rs`: with `ci_autofix` on and the PR not `stopped`, a failing head writes the notice but sends no "CI failed" notification; with it off, the notification is sent as today
- [X] T017 [US1] Test in `crates/delilah-daemon/src/pr_api/forward.rs`: manual forward of `ci-failed` CASes `sha`; then autofix doesn't send that sha; a manual forward of a sha autofix already sent resolves and returns 204 without typing
- [X] T018 [US1] Test: a send failure after the CAS (PausedByLimit race) restores the previous `PrAutofix` (attempt not counted); `no-session` stops with reason `no-session` (in `crates/delilah-daemon/src/autofix/mod.rs`)

### Implementation for User Story 1

- [X] T019 [US1] Implement `decide()`, the idle check and the prompt wrapper over `forward::ci_prompt` in `crates/delilah-daemon/src/autofix/mod.rs`
- [X] T020 [US1] Implement `evaluate(app, key)` in `crates/delilah-daemon/src/autofix/mod.rs`: take `AutofixGuard`, read task + PR rows, decide per row, CAS every row to send into `fixing` (attempts+1, sha, session, sent_at), send one prompt via `forward::send`, roll back on error (or stop on no-session), resolve the rows' `ci-failed` notices, emit `Topic::Prs`/`Topic::Inbox`, record `AutofixSent` (and `AutofixFailedAgain` when coming from `fixing`) / `AutofixFixed` on green
- [X] T021 [US1] Call `autofix::evaluate` from `pr_loop::check_task` after its `CheckGuard` is dropped (spawned) in `crates/delilah-daemon/src/pr_loop/mod.rs`
- [X] T022 [US1] Skip the "CI failed" notification for tasks with autofix handling the PR in `crates/delilah-daemon/src/pr_loop/notices.rs`
- [X] T023 [US1] Manual forward CASes the sha first, no double send, in `crates/delilah-daemon/src/pr_api/forward.rs`
- [X] T024 [US1] Expose `autofix` (DTO without `session`, with `limit`) on each PR of `GET /tasks/{key}/prs` in `crates/delilah-daemon/src/pr_api/dto.rs`, and `ci_autofix` on the task DTO in `crates/delilah-daemon/src/api.rs`
- [X] T025 [US1] E2E `crates/delilah-e2e/tests/autofix.rs`: fake `gh` reports a failing head, fake Claude (scenario per command) commits/pushes, fake `gh` then reports the new green head → one autofix prompt reached the agent, history shows sent + fixed (extend `gh/fake.rs` / fake bin only as needed)

**Checkpoint**: autofix fixes CI on its own for tasks that have it on (set via DB/API).

---

## Phase 4: User Story 2 - Autofix stops and hands over (P1)

**Goal**: 3 attempts per PR, `no-push`, `no-session`; one `autofix-stopped` notice + notification; retry; every exit leaves `fixing`.

**Independent Test**: always-failing fake checks → exactly 3 prompts, one `pr-autofix` inbox item and one notification; "Retry autofix" sends attempt 1 again.

### Tests for User Story 2

- [X] T026 [P] [US2] `decide()` tests in `crates/delilah-daemon/src/autofix/mod.rs`: 4th red head with attempts 3 → stop `limit`; green head while `stopped` → `watching`, notice resolved (acceptance 2.4)
- [X] T027 [P] [US2] Turn-end tests in `crates/delilah-daemon/src/autofix/mod.rs`: turn end of `session` started after `sent_at`, fresh snapshot with head == sha → stop `no-push`; head changed → stays `fixing`; turn of another session or started before `sent_at` → ignored; no fresh snapshot (claim timeout / gh error) → stays `fixing`
- [X] T028 [P] [US2] Cancel tests: toggle off, PR merged/closed, task archived/deleted/back to backlog while `fixing` → `watching` + `AutofixCancelled`, `autofix-stopped` resolved, nothing sent (in `crates/delilah-daemon/src/autofix/mod.rs`)
- [X] T029 [P] [US2] Inbox test in `crates/delilah-daemon/src/inbox.rs`: an open `autofix-stopped` notice is a `pr-autofix` item with reason, attempts and text in es/en; forward on it → 409

### Implementation for User Story 2

- [X] T030 [US2] Stop path in `crates/delilah-daemon/src/autofix/mod.rs`: CAS to `stopped` with reason, insert the `autofix-stopped` notice (`cause = "<sha>:<reason>"`, detail with reason/attempts/checks) via `store_prs`, one notification (none if the app listens), `AutofixStopped` history; resolve it on green/retry/toggle/merge
- [X] T031 [US2] Add `pr_loop::check_task_waiting(app, key, timeout)` (waits for the claim instead of returning) in `crates/delilah-daemon/src/pr_loop/mod.rs`
- [X] T032 [US2] Implement `autofix::on_turn_ended` (spawned; forced waiting check; judge `no-push` only from a snapshot checked after the turn end, for the attempt's session) and call it for the primary session from `handoff::on_turn_ended` in `crates/delilah-daemon/src/handoff.rs`, then `evaluate`
- [X] T033 [US2] Implement `autofix::cancel(app, key, repo?)` and call it from toggle off, the merge/close path in `crates/delilah-daemon/src/pr_loop/notices.rs` / `merge.rs`, archive (`archive.rs`), delete and back-to-backlog (`actions.rs`)
- [X] T034 [US2] `pr-autofix` inbox item (kind, reason, attempts, text) in `crates/delilah-daemon/src/inbox.rs`; `forward` refuses it with 409 in `crates/delilah-daemon/src/pr_api/forward.rs`
- [X] T035 [US2] E2E in `crates/delilah-e2e/tests/autofix.rs`: always-failing checks → 3 prompts, one stopped item, no 4th prompt; a fake Claude turn that doesn't push → `no-push` item

**Checkpoint**: MVP complete (US1 + US2): bounded, visible stops.

---

## Phase 5: User Story 3 - Turning autofix on and off (P2)

**Goal**: per-task toggle (API, CLI, UI), project default (API, CLI, UI), retry.

**Independent Test**: tasks with and without autopilot in a default project; toggle from UI and CLI; state matches.

### Tests for User Story 3

- [ ] T036 [P] [US3] Route tests in `crates/delilah-daemon/src/autofix/api.rs`: `on`/`off`/`retry` per contracts/api.md (200 with the task; 404; 409 `terminal`; 422 retry without/unknown repo); `on` with a failed finished head and an idle agent sends attempt 1 (acceptance 3.3); `on` resets stopped PRs; `off` while `fixing` doesn't touch the session (3.4)
- [ ] T037 [P] [US3] CLI tests in `crates/delilah-cli/src/main.rs`: `autofix <KEY> on|off|retry [--repo]`, `--autofix/--no-autofix` on `new`/`run` (neither → absent field), `project set <name> ci-autofix off|autopilot|always` and `unset`
- [ ] T038 [P] [US3] Vitest `app/src/components/AutofixControl.test.tsx`: toggle calls `setAutofix`, shows on/off, disabled on terminal tasks; `NewTaskDialog.test.tsx`: the option follows the project default and autopilot, and is sent as `ci_autofix`; `ProjectSettings.test.tsx`: the three-value select saves `ci_autofix`

### Implementation for User Story 3

- [ ] T039 [US3] Implement `POST /api/v1/tasks/{key}/autofix` in `crates/delilah-daemon/src/autofix/api.rs` (thin handler over `autofix::set` / `autofix::retry`) and register it in `crates/delilah-daemon/src/api.rs`
- [ ] T040 [US3] CLI: `autofix` subcommand, `--autofix/--no-autofix` on `new`/`run`, `ci-autofix` key on `project set/unset` in `crates/delilah-cli/src/main.rs` (and its client module if separate)
- [ ] T041 [US3] Client types and calls in `app/src/api.ts`: `Task.ci_autofix?`, `PrAutofix`, `Pr.autofix?`, `ProjectSettings.ci_autofix?`, `ci_autofix?` on create/backlog bodies, `setAutofix(key, action, repo?)`
- [ ] T042 [US3] New `app/src/components/AutofixControl.tsx` (+ `.css`), used in `PrView.tsx`, `NewTaskDialog.tsx` (next to autopilot) and `BacklogTaskView.tsx`; three-value select in `ProjectSettings.tsx`; strings in `app/src/i18n/es/` and `app/src/i18n/en/`

**Checkpoint**: users control autofix everywhere.

---

## Phase 6: User Story 4 - Seeing what autofix is doing (P2)

**Goal**: per-PR state on the PR tab, a mark on the badge, the inbox item, history entries.

**Independent Test**: drive one failed and one fixed attempt; the PR tab shows "Fixing CI: attempt 1 of 3", then "watching"; a stop shows the reason and "Retry autofix".

### Tests for User Story 4

- [ ] T043 [P] [US4] Vitest `app/src/components/PrView.test.tsx`: off / watching / fixing (attempt N of 3, failed checks, sent time) / stopped (reason + "Retry autofix" calling `setAutofix(key,'retry',repo)`); `PrBadge.test.tsx`: mark when `task.ci_autofix`; `InboxView.test.tsx`: `pr-autofix` item with Retry and Dismiss, and an unknown kind still renders

### Implementation for User Story 4

- [ ] T044 [US4] Per-PR autofix state line in `app/src/components/PrView.tsx` (via `AutofixControl`), mark in `app/src/components/PrBadge.tsx`, `pr-autofix` item in `app/src/components/InboxView.tsx`; history labels for the autofix events wherever history events are labelled in `app/src/`; i18n es/en

**Checkpoint**: everything autofix does is visible.

---

## Phase 7: Polish & Cross-Cutting

- [ ] T045 [P] `docs/decisions.md`: decision 87 "CI autofix forwards CI failures on its own, opt-in and bounded" (amends #29 for CI only; reviews stay manual; why: limit 3, one per sha, state before send), and an "Amended by 87" note on #29
- [ ] T046 [P] `docs/architecture.md`: an `autofix/` section (decide/evaluate/guard, triggers, CAS, turn-end judgement, cancel, notices) and the module table row
- [ ] T047 [P] `docs/status.md` (PR loop: CI autofix built; upgrade note: live autopilot tasks get autofix on, seeded so no old failure fires) and `docs/usage.md` (toggle, project default, CLI, what the agent is told, stops and retry)
- [ ] T048 Run `cargo fmt`, `cargo clippy --workspace`, `cargo test --workspace`, `cd app && npx tsc --noEmit && npx vitest run`, and the e2e `cargo test -p delilah-e2e --test autofix`
- [ ] T049 Walk through `specs/010-ci-autofix/quickstart.md` in a preview (`scripts/preview.sh --fresh`), never on the installed daemon; check the UI with `ui-verifier`

---

## Dependencies & Execution Order

- **Setup (T001–T002)** → **Foundational (T003–T011)** → stories.
- **US1 (T012–T025)** needs Foundational. **US2 (T026–T035)** needs US1's `evaluate` (T020) and builds the stop path on it.
- **US3 (T036–T042)**: daemon route T039 needs T020/T030/T033; CLI T040 and UI T041–T042 need only the contract and can start after Foundational against the contract.
- **US4 (T043–T044)** needs T041–T042 (types, `AutofixControl`) and the DTO (T024).
- **Polish** last; docs T045–T047 can be written in parallel with US3/US4.

Within Foundational: T003/T004 (core project), T005 (core task) in parallel; T006–T008 (store) after T005; T009 after T004+T007; T010, T011 independent of the store work.

## Parallel Examples

- Foundational: T004 (`project.rs`) ‖ T005 (`task.rs`) ‖ T010 (`forward.rs`) ‖ T011 (`state.rs`).
- US1 tests: T012 ‖ T013 ‖ T014 (all in `autofix/mod.rs` tests, but separate test fns — run as one package if one implementer owns the file); T016 (`notices.rs`) ‖ T017 (`forward.rs`).
- US3: daemon route (T039) ‖ CLI (T040) ‖ UI (T041–T042) — separate crates/dirs.
- Polish docs T045 ‖ T046 ‖ T047.

## Implementation Strategy

1. **MVP** = Setup + Foundational + US1 + US2: autofix works and stops safely; turned on via the default for autopilot tasks (project default `autopilot`) and the API.
2. US3: toggles and the project setting in UI and CLI.
3. US4: visibility in the PR tab, badge and inbox.
4. Polish: docs, full checks, quickstart in a preview.
