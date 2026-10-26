# Work packages — 010-ci-autofix

Source: specs/010-ci-autofix/tasks.md · 49 pending tasks · 19 packages

Paths: daemon `crates/delilah-daemon/src/`, core `crates/delilah-core/src/`, CLI `crates/delilah-cli/src/`, e2e `crates/delilah-e2e/tests/`, UI `app/src/`.

## WP-01 Migration and autofix module skeleton
- Phase: 1 Setup
- Tasks: T001, T002
- Files: crates/delilah-daemon/migrations/0022_ci_autofix.sql, crates/delilah-daemon/src/autofix/mod.rs, crates/delilah-daemon/src/autofix/api.rs, crates/delilah-daemon/src/lib.rs, crates/delilah-daemon/src/store.rs (read only, to match status/archived encoding), crates/delilah-daemon/migrations/0001*.sql (read only)
- Depends on: —
- Parallel group: 1A
- Acceptance: FR-001, FR-003 — migration applies and backfills live autopilot tasks (with seeded PR `sha`); `autofix` module compiles with empty `evaluate`, `on_turn_ended`, `cancel`
- Checks: cargo test -p delilah-daemon
- Reviewers: migration, architecture

## WP-02 Core types: project default and task flag
- Phase: 2 Foundational
- Tasks: T003, T004, T005
- Files: crates/delilah-core/src/project.rs, crates/delilah-core/src/task.rs, daemon field-error localisation file (guess: crates/delilah-daemon/src/i18n.rs or wherever decision 51 messages live)
- Depends on: —
- Parallel group: 2A
- Acceptance: FR-001, FR-002, FR-013 — `AutofixDefault` and `ProjectSettings.ci_autofix` validated (`ci_autofix.invalid`); `Task.ci_autofix` and the five autofix `Event` variants exist
- Checks: cargo test -p delilah-core, cargo test -p delilah-daemon
- Reviewers: contract

## WP-03 Store: task flag and per-PR autofix CAS
- Phase: 2 Foundational
- Tasks: T006, T007, T008
- Files: crates/delilah-daemon/src/store.rs, crates/delilah-daemon/src/store_prs.rs, crates/delilah-daemon/src/autofix/mod.rs
- Depends on: WP-01, WP-02
- Parallel group: 2B
- Acceptance: FR-001, FR-005, FR-008 — `ci_autofix` read/written, autofix events in history, `PrAutofix` + `update_pr_autofix` compare-and-set, migration backfill tested
- Checks: cargo test -p delilah-daemon
- Reviewers: migration

## WP-04 Shared send path and per-task guard
- Phase: 2 Foundational
- Tasks: T010, T011
- Files: crates/delilah-daemon/src/pr_api/forward.rs, crates/delilah-daemon/src/state.rs
- Depends on: WP-01
- Parallel group: 2A
- Acceptance: FR-005, FR-006 — `forward::send` extracted with unchanged forward behaviour; `AutofixGuard` claims once per task and releases on drop (guard placed in state.rs, not autofix/mod.rs, to keep it disjoint from WP-03)
- Checks: cargo test -p delilah-daemon
- Reviewers: —

## WP-05 Resolve ci_autofix at task creation
- Phase: 2 Foundational
- Tasks: T009
- Files: crates/delilah-daemon/src/api.rs, crates/delilah-daemon/src/proposals.rs
- Depends on: WP-02, WP-03
- Parallel group: 2C
- Acceptance: FR-001, FR-002 — explicit `ci_autofix` wins, else project default (`always`/`autopilot`/`off`) on create, backlog create/PATCH and proposals
- Checks: cargo test -p delilah-daemon
- Reviewers: contract

## WP-06 Autofix decision, idle check and prompt
- Phase: 3 US1
- Tasks: T012, T013, T014, T019
- Files: crates/delilah-daemon/src/autofix/mod.rs
- Depends on: WP-03, WP-04
- Parallel group: 3A
- Acceptance: FR-003, FR-004, FR-005, FR-006, FR-007, FR-016 — pure `decide()`, idle rules and es/en prompt wrapper over `forward::ci_prompt` pass their table tests
- Checks: cargo test -p delilah-daemon
- Reviewers: architecture

## WP-07 evaluate and pr_loop trigger
- Phase: 3 US1
- Tasks: T015, T018, T020, T021
- Files: crates/delilah-daemon/src/autofix/mod.rs, crates/delilah-daemon/src/pr_loop/mod.rs
- Depends on: WP-06, WP-05
- Parallel group: 3B
- Acceptance: FR-003, FR-005, FR-011, FR-012, FR-013 — a failing finished head sends one prompt, state `fixing`, notice resolved; green head returns to `watching`; failed send rolls back the attempt
- Checks: cargo test -p delilah-daemon
- Reviewers: architecture

## WP-08 Notification skip, manual-forward interplay, DTOs
- Phase: 3 US1
- Tasks: T016, T017, T022, T023, T024
- Files: crates/delilah-daemon/src/pr_loop/notices.rs, crates/delilah-daemon/src/pr_api/forward.rs, crates/delilah-daemon/src/pr_api/dto.rs, crates/delilah-daemon/src/api.rs
- Depends on: WP-03, WP-04
- Parallel group: 3A
- Acceptance: FR-005, FR-012, FR-014 — no "CI failed" notification when autofix handles the PR; manual forward CASes the sha and never double-sends; PR DTO exposes `autofix` (no `session`, with `limit`) and task DTO `ci_autofix`
- Checks: cargo test -p delilah-daemon
- Reviewers: contract

## WP-09 E2E: autofix fixes CI
- Phase: 3 US1
- Tasks: T025
- Files: crates/delilah-e2e/tests/autofix.rs, crates/delilah-e2e/ gh/fake.rs and fake bin (guess, only as needed)
- Depends on: WP-07, WP-08
- Parallel group: 3C
- Acceptance: FR-003, FR-004, FR-013 — failing head then green head: one autofix prompt reaches the agent, history shows sent + fixed
- Checks: cargo test -p delilah-e2e --test autofix
- Reviewers: —

## WP-10 Stop path, turn-end judgement, waiting check
- Phase: 4 US2
- Tasks: T026, T027, T030, T031, T032
- Files: crates/delilah-daemon/src/autofix/mod.rs, crates/delilah-daemon/src/pr_loop/mod.rs, crates/delilah-daemon/src/handoff.rs, crates/delilah-daemon/src/store_prs.rs
- Depends on: WP-07
- Parallel group: 4A
- Acceptance: FR-008, FR-009, FR-011 — limit 3 and `no-push` stop with one `autofix-stopped` notice + notification + history; green resolves it; turn-end judged only from a fresh snapshot for the attempt's session
- Checks: cargo test -p delilah-daemon
- Reviewers: architecture

## WP-11 Cancel on toggle, merge, archive, delete, backlog
- Phase: 4 US2
- Tasks: T028, T033
- Files: crates/delilah-daemon/src/autofix/mod.rs, crates/delilah-daemon/src/pr_loop/notices.rs, crates/delilah-daemon/src/pr_loop/merge.rs, crates/delilah-daemon/src/archive.rs, crates/delilah-daemon/src/actions.rs
- Depends on: WP-10
- Parallel group: 4B
- Acceptance: FR-010 — every exit from `fixing` (toggle off, merged/closed, archived, deleted, back to backlog) returns to `watching`, records `AutofixCancelled` and sends nothing
- Checks: cargo test -p delilah-daemon
- Reviewers: architecture

## WP-12 pr-autofix inbox item
- Phase: 4 US2
- Tasks: T029, T034
- Files: crates/delilah-daemon/src/inbox.rs, crates/delilah-daemon/src/pr_api/forward.rs
- Depends on: WP-10
- Parallel group: 4B
- Acceptance: FR-009, FR-014, FR-016 — open `autofix-stopped` notice is a `pr-autofix` inbox item (reason, attempts, es/en text); forward on it returns 409
- Checks: cargo test -p delilah-daemon
- Reviewers: contract

## WP-13 E2E: limit and no-push stops
- Phase: 4 US2
- Tasks: T035
- Files: crates/delilah-e2e/tests/autofix.rs
- Depends on: WP-10, WP-11, WP-12
- Parallel group: 4C
- Acceptance: FR-009 — always-failing checks give exactly 3 prompts and one stopped item; a non-pushing turn gives a `no-push` item
- Checks: cargo test -p delilah-e2e --test autofix
- Reviewers: —

## WP-14 Autofix HTTP route
- Phase: 5 US3
- Tasks: T036, T039
- Files: crates/delilah-daemon/src/autofix/api.rs, crates/delilah-daemon/src/api.rs, crates/delilah-daemon/src/autofix/mod.rs (`set` / `retry`, guess)
- Depends on: WP-07, WP-10, WP-11
- Parallel group: 5A
- Acceptance: FR-001, FR-010, FR-015 — `POST /api/v1/tasks/{key}/autofix` on/off/retry per contracts/api.md (200/404/409/422); `on` with a failed idle head sends attempt 1; `off` leaves a running session alone
- Checks: cargo test -p delilah-daemon
- Reviewers: contract, security

## WP-15 CLI autofix commands
- Phase: 5 US3
- Tasks: T037, T040
- Files: crates/delilah-cli/src/main.rs (and client module if separate, guess)
- Depends on: WP-02
- Parallel group: 5A
- Acceptance: FR-002, FR-015 — `autofix <KEY> on|off|retry [--repo]`, `--autofix/--no-autofix` on `new`/`run`, `project set/unset ci-autofix`
- Checks: cargo test -p delilah-cli
- Reviewers: contract

## WP-16 UI: client types, AutofixControl, project select
- Phase: 5 US3
- Tasks: T038, T041, T042
- Files: app/src/api.ts, app/src/components/AutofixControl.tsx, app/src/components/AutofixControl.css, app/src/components/AutofixControl.test.tsx, app/src/components/PrView.tsx, app/src/components/NewTaskDialog.tsx, app/src/components/NewTaskDialog.test.tsx, app/src/components/BacklogTaskView.tsx, app/src/components/ProjectSettings.tsx, app/src/components/ProjectSettings.test.tsx, app/src/i18n/es/, app/src/i18n/en/
- Depends on: WP-02
- Parallel group: 5A
- Acceptance: FR-001, FR-002, FR-015, FR-016 — toggle, new-task option following project default and autopilot, and three-value project select work against the contract
- Checks: cd app && npx tsc --noEmit && npx vitest related --run <files>
- Reviewers: contract

## WP-17 UI: PR state, badge mark, inbox item, history labels
- Phase: 6 US4
- Tasks: T043, T044
- Files: app/src/components/PrView.tsx, app/src/components/PrView.test.tsx, app/src/components/PrBadge.tsx, app/src/components/PrBadge.test.tsx, app/src/components/InboxView.tsx, app/src/components/InboxView.test.tsx, history-event label file under app/src/ (guess), app/src/i18n/es/, app/src/i18n/en/
- Depends on: WP-16, WP-08, WP-12
- Parallel group: 6A
- Acceptance: FR-013, FR-014, FR-016 — PR tab shows off/watching/fixing (attempt N of 3)/stopped with Retry; badge mark; `pr-autofix` inbox item with Retry and Dismiss; unknown inbox kinds still render
- Checks: cd app && npx tsc --noEmit && npx vitest related --run <files>
- Reviewers: —

## WP-18 Docs
- Phase: 7 Polish
- Tasks: T045, T046, T047
- Files: docs/decisions.md, docs/architecture.md, docs/status.md, docs/usage.md
- Depends on: WP-14
- Parallel group: 7A
- Acceptance: FR-001..FR-017 — decision 99 (and "Amended by 99" on #29), `autofix/` architecture section, status and usage updated including the upgrade note
- Checks: —
- Reviewers: —

## WP-19 Full checks and quickstart walk-through
- Phase: 7 Polish
- Tasks: T048, T049
- Files: none (verification; fixes may touch any file above)
- Depends on: WP-09, WP-13, WP-14, WP-15, WP-16, WP-17, WP-18
- Parallel group: 7B
- Acceptance: all FR — fmt, clippy, workspace tests, tsc, full vitest and e2e green; quickstart passes in a preview (never the installed daemon)
- Checks: cargo test --workspace, cargo test -p delilah-e2e --test autofix, cd app && npx tsc --noEmit && npx vitest run
- Reviewers: —
