# Work packages — 004-project-config (Per-project configuration)

Source: specs/004-project-config/tasks.md · 57 pending tasks · 24 packages

## WP-01 Add unicode-normalization dependency
- Phase: 1 Setup
- Tasks: T001
- Files: Cargo.toml (guess, workspace dep), crates/delilah-core/Cargo.toml
- Depends on: —
- Parallel group: 1A
- Acceptance: FR-007 — `unicode-normalization` is available to delilah-core and the workspace builds
- Checks: cargo test -p delilah-core
- Reviewers: —

## WP-02 Core ProjectSettings type and validation
- Phase: 2 Foundational
- Tasks: T002, T003, T006
- Files: crates/delilah-core/src/project.rs, crates/delilah-core/src/lib.rs
- Depends on: WP-01
- Parallel group: 2A
- Acceptance: FR-001, FR-002, FR-004, FR-017 — `ProjectSettings` defaults equal today's behaviour, `{}` deserialises to defaults, `validate()` returns field-named errors
- Checks: cargo test -p delilah-core
- Reviewers: contract

## WP-03 PTY exit code
- Phase: 2 Foundational
- Tasks: T004, T007
- Files: crates/delilah-agents/src/pty.rs
- Depends on: —
- Parallel group: 2A
- Acceptance: FR-010 — `exit_code()` returns the child's status after `wait()`
- Checks: cargo test -p delilah-agents
- Reviewers: —

## WP-04 Settings storage and migration
- Phase: 2 Foundational
- Tasks: T005, T008, T009
- Files: crates/delilah-daemon/migrations/0016_project_settings.sql, crates/delilah-daemon/src/store_projects.rs
- Depends on: WP-02
- Parallel group: 2B
- Acceptance: FR-001, FR-002, FR-017 — projects read back with default settings, `set_project_settings` round-trips, bad stored value reads as defaults
- Checks: cargo test -p delilah-daemon
- Reviewers: migration

## WP-05 Split launch_agent out of start_run
- Phase: 2 Foundational
- Tasks: T011
- Files: crates/delilah-daemon/src/run.rs
- Depends on: —
- Parallel group: 2A
- Acceptance: FR-002 — `launch_agent` exists, no behaviour change, existing run tests green
- Checks: cargo test -p delilah-daemon
- Reviewers: architecture

## WP-06 App settings types
- Phase: 2 Foundational
- Tasks: T010
- Files: app/src/api.ts
- Depends on: —
- Parallel group: 2A
- Acceptance: FR-017 — `ProjectSettings`, `SharedEntry` and `Project.settings` match contracts/api.md
- Checks: app (cd app && npx tsc --noEmit)
- Reviewers: contract

## WP-07 Settings API endpoint
- Phase: 3 US1
- Tasks: T012, T015, T016
- Files: crates/delilah-daemon/src/projects.rs, crates/delilah-daemon/src/api.rs
- Depends on: WP-04
- Parallel group: 3A
- Acceptance: FR-001, FR-004, FR-017 — `PUT /api/v1/projects/{name}/settings` saves, 422 with `fields` on invalid input, 404 unknown project, GET shows full settings
- Checks: cargo test -p delilah-daemon
- Reviewers: contract, security

## WP-08 CLI project show/set/unset
- Phase: 3 US1
- Tasks: T013, T017
- Files: crates/delilah-cli/src/main.rs
- Depends on: WP-02
- Parallel group: 3A
- Acceptance: FR-005, FR-017 — `project show|set|unset` parse and call the PUT; 422 prints `field: message` and exits 1 (runtime needs WP-07 endpoint, parse tests do not)
- Checks: cargo test -p delilah-cli
- Reviewers: contract

## WP-09 Project settings screen
- Phase: 3 US1
- Tasks: T014, T018, T019, T020, T021
- Files: app/src/components/ProjectSettings.tsx, app/src/components/ProjectSettings.test.tsx, app/src/components/ProjectsView.tsx, app/src/api.ts, app/src/i18n/en.ts, app/src/styles.css
- Depends on: WP-06
- Parallel group: 3A
- Acceptance: FR-003, FR-004, FR-015 — settings screen from the project row edits all fields, shows 422 errors under fields, asks to discard unsaved edits
- Checks: app (cd app && npx tsc --noEmit && npx vitest related --run app/src/components/ProjectSettings.tsx app/src/components/ProjectsView.tsx)
- Reviewers: contract

## WP-10 Core branch name and slug
- Phase: 4 US2
- Tasks: T022, T023, T026
- Files: crates/delilah-core/tests/branch_names.json, crates/delilah-core/src/project.rs
- Depends on: WP-02, WP-01
- Parallel group: 4A
- Acceptance: FR-007 — `slug()` and `branch_name()` pass every case in branch_names.json
- Checks: cargo test -p delilah-core
- Reviewers: —

## WP-11 App branch name preview
- Phase: 4 US2
- Tasks: T024, T027, T029
- Files: app/src/branchName.ts, app/src/branchName.test.ts, app/src/components/NewTaskDialog.tsx, app/src/components/NewTaskDialog.test.tsx (guess)
- Depends on: WP-10
- Parallel group: 4B
- Acceptance: FR-007 — `branchName()` passes the same JSON; the new-task form shows "Rama: …" live
- Checks: app (cd app && npx tsc --noEmit && npx vitest related --run app/src/branchName.ts app/src/components/NewTaskDialog.tsx)
- Reviewers: —

## WP-12 Daemon uses branch template
- Phase: 4 US2
- Tasks: T025, T028
- Files: crates/delilah-daemon/src/run.rs
- Depends on: WP-10, WP-05
- Parallel group: 4B
- Acceptance: FR-002, FR-007 — workspace branch follows the template, explicit branch wins, no settings gives `feature/<key>`, submodules on the templated branch
- Checks: cargo test -p delilah-daemon
- Reviewers: —

## WP-13 Shared entries in git crate
- Phase: 5 US3
- Tasks: T030, T033
- Files: crates/delilah-git/src/share.rs, crates/delilah-git/src/lib.rs
- Depends on: WP-02
- Parallel group: 5A
- Acceptance: FR-008 — `share_entries` copies or links per mode, leaves existing entries, reports missing sources
- Checks: cargo test -p delilah-git
- Reviewers: security, architecture

## WP-14 SetupFailed task status
- Phase: 5 US3
- Tasks: T031, T034
- Files: crates/delilah-core/src/task.rs
- Depends on: —
- Parallel group: 5A
- Acceptance: FR-010 — `SetupFailed` status and `SetupFailed`/`RetrySetup`/`SkipSetup` transitions, non-terminal
- Checks: cargo test -p delilah-core
- Reviewers: contract

## WP-15 Setup session runner
- Phase: 5 US3
- Tasks: T032, T035, T036
- Files: crates/delilah-daemon/src/setup.rs, crates/delilah-daemon/src/store_sessions.rs, crates/delilah-daemon/src/session_api.rs, crates/delilah-daemon/src/sessions.rs
- Depends on: WP-03, WP-05, WP-14
- Parallel group: 5B
- Acceptance: FR-009, FR-010, FR-011, FR-016 — setup runs in the worktree before the agent; failure, retry, skip, cancel and timeout behave per spec; output kept in task logs
- Checks: cargo test -p delilah-daemon
- Reviewers: security, architecture

## WP-16 Wire setup into run, API and recovery
- Phase: 5 US3
- Tasks: T037, T038, T039, T040
- Files: crates/delilah-daemon/src/run.rs, crates/delilah-daemon/src/api.rs, crates/delilah-daemon/src/main.rs, crates/delilah-daemon/src/sessions.rs (guess for T039), crates/delilah-daemon/src/actions.rs
- Depends on: WP-13, WP-15
- Parallel group: 5C
- Acceptance: FR-008, FR-009, FR-010, FR-016 — worktree start shares entries then runs setup, `POST .../setup/retry|skip|cancel` (409 wrong status), stuck tasks marked interrupted, archive/delete work on `SetupFailed`
- Checks: cargo test -p delilah-daemon
- Reviewers: contract, architecture

## WP-17 CLI setup commands
- Phase: 5 US3
- Tasks: T041
- Files: crates/delilah-cli/src/main.rs
- Depends on: WP-16
- Parallel group: 5D
- Acceptance: FR-010 — `delilah setup retry|skip|cancel <KEY>`
- Checks: cargo test -p delilah-cli
- Reviewers: contract

## WP-18 App setup status and banner
- Phase: 5 US3
- Tasks: T042, T043, T044
- Files: app/src/api.ts, app/src/status.ts, app/src/components/TaskScreen.tsx, app/src/components/SetupBanner.tsx (guess), app/src/components/SetupBanner.test.tsx (guess), app/src/i18n/en.ts, app/src/styles.css
- Depends on: WP-16
- Parallel group: 5D
- Acceptance: FR-010, FR-016, FR-015 — Setup tab with cancel while preparing; banner with retry/skip on `setup-failed`
- Checks: app (cd app && npx tsc --noEmit && npx vitest related --run app/src/api.ts app/src/status.ts app/src/components/TaskScreen.tsx)
- Reviewers: contract

## WP-19 Daemon always/never submodules
- Phase: 6 US4
- Tasks: T045, T047, T048
- Files: crates/delilah-daemon/src/run.rs, crates/delilah-daemon/src/projects.rs
- Depends on: WP-12, WP-16
- Parallel group: 6A
- Acceptance: FR-012 — always merged into the selection, never refused with 422 and hidden from list and suggestions
- Checks: cargo test -p delilah-daemon
- Reviewers: contract

## WP-20 App always/never submodules
- Phase: 6 US4
- Tasks: T046, T049
- Files: app/src/components/NewTaskDialog.tsx, app/src/components/NewTaskDialog.test.tsx (guess)
- Depends on: WP-11
- Parallel group: 6A
- Acceptance: FR-012 — always checked and disabled, never not listed
- Checks: app (cd app && npx tsc --noEmit && npx vitest related --run app/src/components/NewTaskDialog.tsx)
- Reviewers: —

## WP-21 Daemon default workspace mode
- Phase: 7 US5
- Tasks: T050, T052
- Files: crates/delilah-daemon/src/run.rs
- Depends on: WP-19
- Parallel group: 7A
- Acceptance: FR-006, FR-002 — mode defaults from settings after request and backlog choice; no settings means checkout
- Checks: cargo test -p delilah-daemon
- Reviewers: —

## WP-22 App default workspace mode
- Phase: 7 US5
- Tasks: T051, T053
- Files: app/src/components/NewTaskDialog.tsx, app/src/components/NewTaskDialog.test.tsx (guess)
- Depends on: WP-20
- Parallel group: 7A
- Acceptance: FR-013 — WherePicker preselects the project's mode
- Checks: app (cd app && npx tsc --noEmit && npx vitest related --run app/src/components/NewTaskDialog.tsx)
- Reviewers: —

## WP-23 Documentation
- Phase: 8 Polish
- Tasks: T054, T055
- Files: README.md, docs/status.md, docs/architecture.md, docs/vision.md
- Depends on: WP-21, WP-22
- Parallel group: 8A
- Acceptance: FR-014 — README covers settings screen, CLI and setup; status, architecture and vision updated
- Checks: —
- Reviewers: —

## WP-24 Final verification
- Phase: 8 Polish
- Tasks: T056, T057
- Files: — (no edits expected)
- Depends on: WP-23
- Parallel group: 8B
- Acceptance: all FR — workspace tests, clippy, tsc and vitest pass; quickstart walk-through in a preview works
- Checks: cargo test --workspace, cargo clippy --workspace, app (npx tsc --noEmit && npx vitest run), cargo test --manifest-path app/src-tauri/Cargo.toml
- Reviewers: —
