# Work packages — 013-jira-subtasks-import

Source: specs/013-jira-subtasks-import/tasks.md · 47 pending tasks · 17 packages

## WP-01 Setup: module skeletons and migration
- Phase: 1 Setup
- Tasks: T001, T002
- Files: crates/delilah-daemon/src/jira_import.rs, crates/delilah-daemon/src/lib.rs (guess; or main.rs), crates/delilah-daemon/src/store.rs, crates/delilah-daemon/src/store/jira_skips.rs, crates/delilah-daemon/migrations/0026_jira_subtask_skips.sql
- Depends on: —
- Parallel group: 1A
- Acceptance: FR-011 — empty modules compile; migration 0026 creates jira_subtask_skips (parent_task, jira_key) with no foreign key
- Checks: cargo test -p delilah-daemon
- Reviewers: migration, architecture

## WP-02 Jira client: my_subtask_keys
- Phase: 2 Foundational
- Tasks: T003, T004
- Files: crates/delilah-integrations/src/jira.rs
- Depends on: —
- Parallel group: 2A
- Acceptance: FR-003 — my_subtask_keys returns only the sub-tasks assigned to the current user (JQL remote, filtered sample); bad key is BadKey; sample data has the fixture ticket
- Checks: cargo test -p delilah-integrations
- Reviewers: security

## WP-03 Store primitives: become_parent_task, set_kind_tx, tasks_by_keys
- Phase: 2 Foundational
- Tasks: T005, T006, T007
- Files: crates/delilah-daemon/src/store.rs, crates/delilah-daemon/src/api_hierarchy.rs
- Depends on: —
- Parallel group: 2A
- Acceptance: FR-006, FR-007 — transactional kind change and parent-task reset reusable by the import; make_parent_task behaves as before
- Checks: cargo test -p delilah-daemon
- Reviewers: —

## WP-04 Jira skips store
- Phase: 2 Foundational
- Tasks: T008, T009, T010
- Files: crates/delilah-daemon/src/store/jira_skips.rs, crates/delilah-daemon/src/actions.rs
- Depends on: WP-01
- Parallel group: 2A
- Acceptance: FR-011 — skips are added/removed idempotently, listed sorted, deleted with their parent task, kept on archive
- Checks: cargo test -p delilah-daemon
- Reviewers: migration

## WP-05 Shared task constructor
- Phase: 2 Foundational
- Tasks: T011, T012
- Files: crates/delilah-daemon/src/api.rs, crates/delilah-daemon/src/api_hierarchy.rs
- Depends on: —
- Parallel group: 2A
- Acceptance: FR-004 — pure new_task gives the same defaults as POST /tasks; existing route tests pass
- Checks: cargo test -p delilah-daemon
- Reviewers: —

## WP-06 Import planner (pure)
- Phase: 2 Foundational
- Tasks: T013, T014
- Files: crates/delilah-daemon/src/jira_import.rs
- Depends on: WP-01
- Parallel group: 2A
- Acceptance: FR-002, FR-003, FR-007, FR-008 — plan_import classifies parent and sub-task rows and default selection per data-model, reusing delilah_core::hierarchy checks
- Checks: cargo test -p delilah-daemon
- Reviewers: architecture

## WP-07 US1 daemon: preview, apply, routes, Parent line
- Phase: 3 US1
- Tasks: T015, T016, T017, T020, T021, T022, T023
- Files: crates/delilah-daemon/src/jira_import.rs, crates/delilah-daemon/src/jira.rs, crates/delilah-daemon/src/api.rs, crates/delilah-daemon/tests/parent_tasks.rs
- Depends on: WP-02, WP-03, WP-04, WP-05, WP-06
- Parallel group: 3A
- Acceptance: FR-001, FR-002, FR-004, FR-005, FR-006, FR-013, FR-015 — GET/POST /api/v1/jira/issues/{key}/import create the parent task and chosen subtasks atomically in the backlog; ticket file carries a Parent line
- Checks: cargo test -p delilah-daemon
- Reviewers: contract, security

## WP-08 US1 UI: import dialog (create mode) and button
- Phase: 3 US1
- Tasks: T018, T019, T024, T025, T026
- Files: app/src/api.ts, app/src/api.test.ts, app/src/components/JiraImportDialog.tsx, app/src/components/JiraImportDialog.test.tsx, app/src/components/JiraView.tsx, app/src/components/JiraView.test.tsx, app/src/i18n/en/JiraImportDialog.ts, app/src/i18n/es/JiraImportDialog.ts (guess), app/src/i18n/en/JiraView.ts, app/src/i18n/es/JiraView.ts (guess), app/src/i18n/en/en.ts, app/src/i18n/es/index.ts (guess)
- Depends on: —
- Parallel group: 3A
- Acceptance: FR-001, FR-002, FR-003 — "Create parent task" on a ticket with sub-tasks opens a dialog with default ticks, project picker and result with left-out reasons
- Checks: app (tsc --noEmit, vitest related)
- Reviewers: contract

## WP-09 US1 CLI: delilah jira import
- Phase: 3 US1
- Tasks: T027
- Files: crates/delilah-cli/src/main.rs
- Depends on: WP-07
- Parallel group: 3B
- Acceptance: FR-001, FR-002 — preview table without write flags; --project/--defaults/--add apply; exit code 1 on refusal
- Checks: cargo test -p delilah-cli
- Reviewers: —

## WP-10 US2 daemon: convert, attach, in-transaction re-checks
- Phase: 4 US2
- Tasks: T028, T029, T031
- Files: crates/delilah-daemon/src/jira_import.rs
- Depends on: WP-07
- Parallel group: 4A
- Acceptance: FR-006, FR-007, FR-008 — existing ticket task becomes the parent task, existing sub-task tasks attach or are left out with reasons; all or nothing; second run creates nothing
- Checks: cargo test -p delilah-daemon
- Reviewers: —

## WP-11 US2 UI: convert/attach/conflict rendering
- Phase: 4 US2
- Tasks: T030, T032
- Files: app/src/components/JiraImportDialog.tsx, app/src/components/JiraImportDialog.test.tsx, app/src/i18n/en/JiraImportDialog.ts, app/src/i18n/es/JiraImportDialog.ts (guess)
- Depends on: WP-08
- Parallel group: 4A
- Acceptance: FR-007, FR-008 — dialog shows convert/existing/refused, attach and disabled conflict rows with reasons
- Checks: app
- Reviewers: —

## WP-12 US3 daemon: skip and unskip
- Phase: 5 US3
- Tasks: T033, T036
- Files: crates/delilah-daemon/src/jira_import.rs
- Depends on: WP-10
- Parallel group: 5A
- Acceptance: FR-010, FR-011, FR-012 — POST skip/unskip persist skips; skipped rows are not selected; hand-made parent tasks get the offer; no Jira call for non-Jira keys
- Checks: cargo test -p delilah-daemon
- Reviewers: contract

## WP-13 US3 UI: add mode and new-sub-tasks banner
- Phase: 5 US3
- Tasks: T034, T035, T037, T038
- Files: app/src/components/SubtaskList.tsx, app/src/components/SubtaskList.test.tsx, app/src/components/JiraImportDialog.tsx, app/src/components/JiraImportDialog.test.tsx, app/src/i18n/en/SubtaskList.ts, app/src/i18n/es/SubtaskList.ts (guess), app/src/i18n/en/JiraImportDialog.ts, app/src/i18n/es/JiraImportDialog.ts (guess)
- Depends on: WP-11
- Parallel group: 5A
- Acceptance: FR-010, FR-011, FR-012 — parent task screen shows "N new Jira sub-tasks · Add" without blocking the list, plus Skipped (n) with Undo
- Checks: app
- Reviewers: —

## WP-14 US3 CLI: --skip / --unskip
- Phase: 5 US3
- Tasks: T039
- Files: crates/delilah-cli/src/main.rs
- Depends on: WP-09, WP-12
- Parallel group: 5B
- Acceptance: FR-011 — delilah jira import accepts --skip and --unskip; preview on a parent task shows only new and skipped rows
- Checks: cargo test -p delilah-cli
- Reviewers: —

## WP-15 US4: Jira status in the subtask list
- Phase: 6 US4
- Tasks: T040, T041, T042
- Files: app/src/components/SubtaskList.tsx, app/src/components/SubtaskList.test.tsx, app/src/i18n/en/SubtaskList.ts, app/src/i18n/es/SubtaskList.ts (guess), crates/delilah-daemon/src/jira_import.rs
- Depends on: WP-12, WP-13
- Parallel group: 6A
- Acceptance: FR-009, FR-013, FR-014 — each subtask shows its Jira status; subtasks gone from the ticket are marked and left unchanged
- Checks: cargo test -p delilah-daemon, app
- Reviewers: —

## WP-16 Docs
- Phase: 7 Polish
- Tasks: T043, T044, T045, T046
- Files: docs/usage.md, docs/status.md, docs/decisions.md, docs/architecture.md
- Depends on: WP-15
- Parallel group: 7A
- Acceptance: FR-016 — docs describe the flow, the decision (one-way, pull on open) and the new modules/routes
- Checks: —
- Reviewers: —

## WP-17 Full check and quickstart
- Phase: 7 Polish
- Tasks: T047
- Files: —
- Depends on: WP-14, WP-16
- Parallel group: 7B
- Acceptance: all — workspace, Tauri shell and UI checks pass; quickstart walks through in a fresh preview
- Checks: cargo test --workspace, cargo test --manifest-path app/src-tauri/Cargo.toml, app (tsc, vitest run)
- Reviewers: —
