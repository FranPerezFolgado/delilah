# Work packages — 004-saving-mode

Source: specs/004-saving-mode/tasks.md · 36 pending tasks · 10 packages

Phase 1 (Setup) has no tasks. Check names: `cargo test -p <crate>`, `app/src-tauri` (`cargo test --manifest-path app/src-tauri/Cargo.toml`), `web` (`cd app && npx tsc --noEmit && npx vitest related --run <files>`).

## WP-01 Core types, migration and session store
- Phase: 2 Foundational
- Tasks: T001, T002, T003, T004, T005, T006, T007
- Files: crates/delilah-core/src/task.rs, crates/delilah-daemon/migrations/0013_saving_mode.sql, crates/delilah-daemon/src/store.rs, crates/delilah-daemon/src/store_sessions.rs, crates/delilah-daemon/src/run.rs, crates/delilah-daemon/src/session_api.rs
- Depends on: —
- Parallel group: 2A
- Acceptance: FR-001, FR-006, FR-011, FR-012 — Step/Handoff types, saving_mode/handoff/step persisted, hand_over/undo_hand_over keep exactly one primary and are idempotent; no behaviour change.
- Checks: cargo test -p delilah-core, cargo test -p delilah-daemon
- Reviewers: migration, contract, architecture

## WP-02 Run request and handoff engine
- Phase: 3 User Story 1
- Tasks: T008, T009, T010, T011, T012
- Files: crates/delilah-daemon/src/run.rs, crates/delilah-daemon/src/handoff.rs, crates/delilah-daemon/src/lib.rs
- Depends on: WP-01
- Parallel group: 3A
- Acceptance: FR-001, FR-002, FR-003, FR-005, FR-006, FR-007, FR-011, FR-014 — saving-mode first session is "Spec y plan"; boundary detection; confirm/decline/retry/start with undo on failure.
- Checks: cargo test -p delilah-daemon
- Reviewers: architecture, security

## WP-03 Handoff wiring: phases, API routes, inbox, recovery
- Phase: 3 User Story 1
- Tasks: T013, T014, T015, T016, T017
- Files: crates/delilah-daemon/src/phases.rs, crates/delilah-daemon/src/api.rs, crates/delilah-daemon/src/handoff.rs, crates/delilah-daemon/src/inbox.rs, crates/delilah-daemon/src/scheduler.rs
- Depends on: WP-02
- Parallel group: 3B
- Acceptance: FR-004, FR-005, FR-010, FR-011, FR-014 — TurnEnded triggers the offer, POST /handoff and /saving-mode routes work (200/404/409/400), inbox `handoff` item, restart recovery.
- Checks: cargo test -p delilah-daemon
- Reviewers: contract, security

## WP-04 CLI flag
- Phase: 3 User Story 1
- Tasks: T018
- Files: crates/delilah-cli/src/main.rs
- Depends on: WP-02
- Parallel group: 3B
- Acceptance: FR-001, FR-002 — `--saving-mode` / `--no-saving-mode` on `new` and `run` send `saving_mode`.
- Checks: cargo test -p delilah-cli
- Reviewers: —

## WP-05 Web UI: types, task dialog, Bandeja item, English strings
- Phase: 3 User Story 1
- Tasks: T019, T020, T021, T022
- Files: app/src/api.ts, app/src/components/NewTaskDialog.tsx, app/src/components/NewTaskDialog.test.tsx, app/src/components/BacklogTaskView.tsx, app/src/components/InboxView.tsx, app/src/components/InboxView.test.tsx, app/src/i18n/en.ts
- Depends on: WP-02
- Parallel group: 3B
- Acceptance: FR-001, FR-002, FR-005, FR-014 — client types match contracts/api.md; "Modo ahorro" checkbox; Bandeja shows confirm/decline/retry for the handoff item.
- Checks: web
- Reviewers: contract

## WP-06 Session chain view
- Phase: 4 User Story 2
- Tasks: T023, T024, T025, T026, T027
- Files: app/src/chain.ts, app/src/chain.test.ts, app/src/components/SessionChain.tsx, app/src/components/SessionChain.test.tsx, app/src/components/TaskScreen.tsx, app/src/components/TaskScreen.test.tsx, app/src/styles.css
- Depends on: WP-05
- Parallel group: 4A
- Acceptance: FR-009 — chain and marker only for saving-mode tasks, correct step states, tab of the new primary selected after a handoff.
- Checks: web
- Reviewers: architecture

## WP-07 Save notes before handoff and settings
- Phase: 5 User Story 3
- Tasks: T028, T029, T030
- Files: crates/delilah-daemon/src/handoff.rs, app/src/components/SettingsView.tsx, app/src/i18n/en.ts
- Depends on: WP-03, WP-05
- Parallel group: 5A
- Acceptance: FR-002, FR-008 — with save-notes on, `/save-session` is typed and the handoff waits for the turn end; limited turn keeps `saving`; settings expose both preferences.
- Checks: cargo test -p delilah-daemon, web
- Reviewers: —

## WP-08 Automatic review session
- Phase: 6 User Story 4
- Tasks: T031, T032
- Files: crates/delilah-daemon/src/handoff.rs, crates/delilah-daemon/src/inbox.rs
- Depends on: WP-07
- Parallel group: 6A
- Acceptance: FR-004, FR-005, FR-007, FR-015 — all tasks checked on turn end starts "Revisión" automatically, with notification and inbox item; review prompt forbids push and PR.
- Checks: cargo test -p delilah-daemon
- Reviewers: contract

## WP-09 Docs
- Phase: 7 Polish
- Tasks: T033, T034, T035
- Files: docs/architecture.md, docs/decisions.md, docs/status.md, README.md
- Depends on: WP-08, WP-06
- Parallel group: 7A
- Acceptance: FR-013 — docs describe saving mode, decision recorded, status updated.
- Checks: —
- Reviewers: —

## WP-10 Full check and quickstart
- Phase: 7 Polish
- Tasks: T036
- Files: — (no edits expected)
- Depends on: WP-09
- Parallel group: 7B
- Acceptance: FR-013 — `cargo test --workspace`, app/src-tauri tests, `npx tsc --noEmit` and `npx vitest run` green; quickstart walked in a preview.
- Checks: cargo test --workspace, cargo test --manifest-path app/src-tauri/Cargo.toml, web (full vitest run)
- Reviewers: —
