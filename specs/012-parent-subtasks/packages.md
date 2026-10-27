# Work packages — 012-parent-subtasks

Source: specs/012-parent-subtasks/tasks.md · 56 pending tasks · 19 packages

## WP-01 Module skeletons
- Phase: 1 Setup
- Tasks: T001
- Files: crates/delilah-core/src/hierarchy.rs, crates/delilah-core/src/lib.rs, crates/delilah-daemon/src/integration.rs, crates/delilah-daemon/src/api_hierarchy.rs, crates/delilah-daemon/src/lib.rs (guess; or main.rs)
- Depends on: —
- Parallel group: 1A
- Acceptance: FR-004 — empty modules compile and are declared; workspace builds
- Checks: cargo test -p delilah-core, cargo test -p delilah-daemon
- Reviewers: architecture

## WP-02 Core hierarchy model
- Phase: 2 Foundational
- Tasks: T002, T003, T004, T005
- Files: crates/delilah-core/src/hierarchy.rs, crates/delilah-core/src/task.rs, crates/delilah-core/src/project.rs
- Depends on: WP-01
- Parallel group: 2A
- Acceptance: FR-001, FR-002, FR-003, FR-018, FR-021 — TaskKind, progress, Refusal, check_* and MergePreference exist; old JSON reads as normal task / merge
- Checks: cargo test -p delilah-core
- Reviewers: contract

## WP-03 Git base override and merge helpers
- Phase: 2 Foundational
- Tasks: T008, T009, T010
- Files: crates/delilah-git/src/base.rs, crates/delilah-git/src/workspace.rs, every `WorkspaceSpec { .. }` literal in the workspace (grep; callers in crates/delilah-daemon/src/run.rs and others)
- Depends on: —
- Parallel group: 2A
- Acceptance: FR-010, FR-013, FR-015 — workspaces can branch from a given origin ref (BaseMissing when absent); commit_empty and merge_refs work and undo on conflict
- Checks: cargo test -p delilah-git, cargo test --workspace (compile of WorkspaceSpec literals)
- Reviewers: security

## WP-04 Storage and migration
- Phase: 2 Foundational
- Tasks: T006, T007
- Files: crates/delilah-daemon/migrations/0025_task_hierarchy.sql, crates/delilah-daemon/src/store.rs
- Depends on: WP-02
- Parallel group: 2B
- Acceptance: FR-001, FR-004, FR-029 — kind and parent_task persist; pre-upgrade DB migrates with all tasks normal
- Checks: cargo test -p delilah-daemon
- Reviewers: migration

## WP-05 API client types
- Phase: 2 Foundational
- Tasks: T015
- Files: app/src/api.ts, app/src/api.test.ts (guess)
- Depends on: WP-02
- Parallel group: 2B
- Acceptance: FR-029 — client types and calls mirror contracts/api.md
- Checks: app (npx tsc --noEmit, npx vitest related --run)
- Reviewers: contract

## WP-06 Daemon integration module, messages, notices
- Phase: 2 Foundational
- Tasks: T011, T012, T013, T014
- Files: crates/delilah-daemon/src/integration.rs, crates/delilah-daemon/src/lang.rs, crates/delilah-daemon/src/inbox.rs
- Depends on: WP-02, WP-04
- Parallel group: 2C
- Acceptance: FR-013, FR-017, FR-021 — base_for, progress, live_subtasks, preferred_method pass tests; all refusals map to 409 messages; two new notice kinds
- Checks: cargo test -p delilah-daemon
- Reviewers: architecture, contract

## WP-07 Create subtasks and task DTO
- Phase: 3 US1
- Tasks: T016, T018, T019
- Files: crates/delilah-daemon/src/api.rs, crates/delilah-daemon/src/api_hierarchy.rs (tests)
- Depends on: WP-06
- Parallel group: 3A
- Acceptance: FR-002, FR-003, FR-005, FR-006, FR-007, FR-021 — parent tasks and subtasks can be created with refusals; GET tasks returns kind, parent_task, subtask_progress
- Checks: cargo test -p delilah-daemon
- Reviewers: contract, security

## WP-08 Start rules for parent tasks and subtasks
- Phase: 3 US1
- Tasks: T017, T020, T021, T022
- Files: crates/delilah-daemon/src/integration.rs, crates/delilah-daemon/src/run.rs, crates/delilah-daemon/src/branching.rs, crates/delilah-daemon/tests/parent_tasks.rs
- Depends on: WP-03, WP-07
- Parallel group: 3B
- Acceptance: FR-009, FR-010, FR-011, FR-012 — parent task gets worktree and pushed branch with no agent; subtasks branch from origin/<parent branch> in parallel; start before parent refused
- Checks: cargo test -p delilah-daemon
- Reviewers: architecture

## WP-09 Subtask endpoints and CLI
- Phase: 3 US1
- Tasks: T023, T024
- Files: crates/delilah-daemon/src/api_hierarchy.rs, crates/delilah-daemon/src/api.rs, crates/delilah-cli/src/main.rs
- Depends on: WP-07
- Parallel group: 3B
- Acceptance: FR-005, FR-022 — push-branch and subtasks endpoints; CLI new --parent-task / --parent, ls nesting, show details
- Checks: cargo test -p delilah-daemon, cargo test -p delilah-cli
- Reviewers: contract

## WP-10 UI: create, sidebar, subtask list
- Phase: 3 US1
- Tasks: T025, T026, T027
- Files: app/src/components/NewTaskDialog.tsx, app/src/components/Sidebar.tsx, app/src/components/SubtaskList.tsx, app/src/components/TaskScreen.tsx, app/src/components/TaskHeader.tsx, app/src/i18n/en/ (plus Spanish strings file, guess), matching *.test.tsx
- Depends on: WP-05
- Parallel group: 3A
- Acceptance: FR-005, FR-021, FR-022, FR-024, FR-025 — create parent task or subtask, nested sidebar with N/M, subtask list and parent link in the task screen
- Checks: app
- Reviewers: —

## WP-11 Subtask PRs into the integration branch
- Phase: 4 US2
- Tasks: T028, T029, T030, T031, T032
- Files: crates/delilah-daemon/src/pr_api/open.rs, crates/delilah-daemon/src/base_update.rs, crates/delilah-daemon/src/pr_api/merge.rs, crates/delilah-daemon/src/pr_api/dto.rs
- Depends on: WP-08
- Parallel group: 4A
- Acceptance: FR-013, FR-015, FR-017 — every subtask PR path targets the parent branch; behind/Update branch use it; squash by default for subtasks
- Checks: cargo test -p delilah-daemon
- Reviewers: contract

## WP-12 UI: PR method and base warning
- Phase: 4 US2
- Tasks: T033
- Files: app/src/components/PrView.tsx, app/src/components/PrView.test.tsx (guess)
- Depends on: WP-05
- Parallel group: 4A
- Acceptance: FR-013, FR-017 — shows pr.method; warns when base differs from the expected branch
- Checks: app
- Reviewers: —

## WP-13 Parent task PR flow (daemon PR API)
- Phase: 5 US3
- Tasks: T034, T037, T038, T039
- Files: crates/delilah-daemon/src/pr_api/open.rs, crates/delilah-daemon/src/pr_api/merge.rs, crates/delilah-daemon/src/gh/mod.rs, crates/delilah-daemon/src/api.rs, crates/delilah-daemon/src/pr_api/ (ready handler, guess ready.rs)
- Depends on: WP-11
- Parallel group: 5A
- Acceptance: FR-014, FR-017, FR-019, FR-020 — parent PR is draft unless ready; empty commit path; Mark ready endpoint; merge needs confirmation with live subtasks
- Checks: cargo test -p delilah-daemon
- Reviewers: contract, security

## WP-14 Parent task Update branch and PR loop
- Phase: 5 US3
- Tasks: T035, T036, T040, T041
- Files: crates/delilah-daemon/src/base_update.rs, crates/delilah-daemon/src/pr_loop/mod.rs, crates/delilah-daemon/src/pr_loop/merge.rs
- Depends on: WP-11
- Parallel group: 5A
- Acceptance: FR-016, FR-021, FR-023, SC-006 — Update branch merges own integration branch then default; subtask_commits reported; merges refresh parent progress; parent-task-ready notice once
- Checks: cargo test -p delilah-daemon
- Reviewers: —

## WP-15 UI: parent task PR and setting
- Phase: 5 US3
- Tasks: T042, T043
- Files: app/src/components/ProjectSettings.tsx, app/src/components/PrView.tsx, app/src/i18n/en/, matching *.test.tsx
- Depends on: WP-12, WP-13, WP-14
- Parallel group: 5B
- Acceptance: FR-014, FR-016, FR-018, FR-019, FR-023 — merge-method setting, draft badge, Mark ready, empty commit, live-subtasks confirmation, base banner
- Checks: app
- Reviewers: —

## WP-16 Archive and delete rules
- Phase: 6 US4
- Tasks: T044, T045, T046, T047
- Files: crates/delilah-daemon/src/actions.rs, crates/delilah-daemon/src/pr_loop/merge.rs, crates/delilah-cli/src/main.rs, app/src/components/TaskHeader.tsx, matching tests
- Depends on: WP-09, WP-10, WP-14
- Parallel group: 6A
- Acceptance: FR-026, FR-027, FR-028, SC-004, SC-006 — parent with live subtasks cannot be archived or deleted (even with force); merged parent with live subtasks becomes done with a notice
- Checks: cargo test -p delilah-daemon, cargo test -p delilah-cli, app
- Reviewers: —

## WP-17 Convert and attach
- Phase: 7 US5
- Tasks: T048, T049, T050, T051
- Files: crates/delilah-daemon/src/api_hierarchy.rs, crates/delilah-cli/src/main.rs, app/src/components/BacklogTaskView.tsx, app/src/components/TaskHeader.tsx, matching tests
- Depends on: WP-16
- Parallel group: 7A
- Acceptance: FR-008 — make-parent-task and attach endpoints, CLI and UI with refusal reasons
- Checks: cargo test -p delilah-daemon, cargo test -p delilah-cli, app
- Reviewers: contract

## WP-18 Docs
- Phase: 8 Polish
- Tasks: T052, T053, T054
- Files: docs/decisions.md, docs/usage.md, docs/status.md, docs/architecture.md
- Depends on: WP-17
- Parallel group: 8A
- Acceptance: FR-030 — decision, usage, status and architecture describe the integration branch pattern
- Checks: —
- Reviewers: —

## WP-19 Full checks and quickstart
- Phase: 8 Polish
- Tasks: T055, T056
- Files: —
- Depends on: WP-18
- Parallel group: 8B
- Acceptance: SC-001, SC-005 — whole workspace, Tauri shell and UI green; quickstart passes in a preview
- Checks: cargo test --workspace, cargo test --manifest-path app/src-tauri/Cargo.toml, app (npx vitest run)
- Reviewers: —
