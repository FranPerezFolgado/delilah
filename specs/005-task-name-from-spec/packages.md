# Work packages — 005-task-name-from-spec

Source: specs/005-task-name-from-spec/tasks.md · 45 pending tasks · 12 packages

T046 (version bump) is left out on purpose: the orchestrator does it. Phase 1 has no tasks.

## WP-01 Core types, migration and store
- Phase: 2 Foundational
- Tasks: T001, T002, T003, T004, T005, T006, T007
- Files: crates/delilah-core/src/task.rs, crates/delilah-daemon/migrations/0015_task_branch_from_spec.sql, crates/delilah-daemon/src/store.rs, crates/delilah-daemon/src/store_runs.rs
- Depends on: —
- Parallel group: 2A
- Acceptance: FR-001, FR-003, FR-014 — Task has title_source/previous_title/branch_error (old rows read `manual`); workspaces store a branchless record and spec_dirs_at_start; store setters (set_title_source, set_branch_error, set_title, set_workspace_branch) work.
- Checks: cargo test -p delilah-core, cargo test -p delilah-daemon
- Reviewers: migration

## WP-02 Git: has_branch, spec_dirs, detached HEAD
- Phase: 2 Foundational
- Tasks: T008, T009
- Files: crates/delilah-git/src/workspace.rs
- Depends on: —
- Parallel group: 2A
- Acceptance: FR-002 — Workspace::has_branch(), spec_dirs(root) and current_branch() returning None on a detached HEAD.
- Checks: cargo test -p delilah-git
- Reviewers: —

## WP-03 Branching module skeleton and triggers
- Phase: 2 Foundational
- Tasks: T010, T011, T012
- Files: crates/delilah-daemon/src/branching.rs, crates/delilah-daemon/src/lib.rs, crates/delilah-daemon/src/state.rs, crates/delilah-daemon/src/phases.rs, crates/delilah-daemon/src/scheduler.rs, crates/delilah-daemon/src/main.rs, crates/delilah-daemon/src/run.rs, crates/delilah-daemon/src/api.rs, crates/delilah-daemon/src/proposals.rs
- Depends on: WP-01, WP-02
- Parallel group: 2B
- Acceptance: FR-002, FR-016 — new_spec_folder, on_activity and recover exist as guarded no-ops with a per-task lock; start_run records spec_dirs_at_start; free tasks are created with title_source=initial; no behaviour change yet.
- Checks: cargo test -p delilah-core, cargo test -p delilah-git, cargo test -p delilah-daemon
- Reviewers: architecture

## WP-04 US1 daemon: retitle and title-only PATCH
- Phase: 3 US1 The task takes its name from the analysis
- Tasks: T013, T014, T015, T016, T017, T018
- Files: crates/delilah-daemon/src/branching.rs, crates/delilah-daemon/src/api.rs
- Depends on: WP-03
- Parallel group: 3A
- Acceptance: FR-003, FR-004, FR-005, FR-013 — the first new spec's title renames an initial free task once (previous_title kept, Topic::Tasks emitted); PATCH with only title works on a running task and marks it manual.
- Checks: cargo test -p delilah-daemon
- Reviewers: contract

## WP-05 US1 UI: editable title and restore
- Phase: 3 US1 The task takes its name from the analysis
- Tasks: T019, T020, T021
- Files: app/src/api.ts, app/src/components/TaskScreen.tsx, app/src/components/TaskScreen.test.tsx, app/src/i18n/en.ts
- Depends on: —
- Parallel group: 3A
- Acceptance: FR-003, FR-004 — Task type carries the new fields; the header title is editable in place; "Antes: <previous>" and "Restaurar" show only when previous_title is set.
- Checks: app (cd app && npx tsc --noEmit && npx vitest related --run <files>)
- Reviewers: contract

## WP-06 US2 git layer: branchless workspaces
- Phase: 4 US2 The branch is named after the analysis
- Tasks: T022, T023
- Files: crates/delilah-git/src/workspace.rs, crates/delilah-daemon/src/run.rs
- Depends on: WP-04
- Parallel group: 4A
- Acceptance: FR-001, FR-009 — WorkspaceSpec.branch is Option; branchless worktree and checkout (parent included) work; apply_branch and branch_exists exist; callers updated.
- Checks: cargo test -p delilah-git, cargo test -p delilah-daemon
- Reviewers: security

## WP-10 US2 Bandeja: branch error item and retry button
- Phase: 4 US2 The branch is named after the analysis
- Tasks: T038, T039, T040, T041
- Files: crates/delilah-daemon/src/inbox.rs, app/src/api.ts, app/src/components/InboxView.tsx, app/src/components/InboxView.test.tsx, app/src/i18n/en.ts
- Depends on: WP-01, WP-05
- Parallel group: 4A
- Acceptance: FR-011 — a task with branch_error yields a `branch` inbox item shown with "Reintentar" calling retryBranch (the route itself is built in WP-08).
- Checks: cargo test -p delilah-daemon, app
- Reviewers: contract

## WP-07 US2 ensure: slug, branch creation, branchless start
- Phase: 4 US2 The branch is named after the analysis
- Tasks: T024, T025, T026, T027, T028
- Files: crates/delilah-daemon/src/branching.rs, crates/delilah-daemon/src/run.rs
- Depends on: WP-04, WP-06
- Parallel group: 4B
- Acceptance: FR-001, FR-002, FR-002a, FR-006, FR-008, FR-010, FR-011 — slug and ensure adopt or create the branch (worktree, checkout, parent with submodules, collisions, idempotent, branch_error on failure); free tasks start branchless, Jira ones keep feature/<KEY>.
- Checks: cargo test -p delilah-daemon
- Reviewers: security, architecture

## WP-09 US2 compatibility: specs, PR, archive
- Phase: 4 US2 The branch is named after the analysis
- Tasks: T035, T036, T037
- Files: crates/delilah-daemon/src/specs.rs, crates/delilah-daemon/src/pr_api/open.rs, crates/delilah-daemon/src/pr_loop/mod.rs, crates/delilah-daemon/src/archive.rs, crates/delilah-git/src/workspace.rs (guess, only if archive_workspace needs a change)
- Depends on: WP-06
- Parallel group: 4B
- Acceptance: FR-015 — spec progress, PR open (422 without a branch), PR loop and archive/delete work for branchless worktree and checkout tasks.
- Checks: cargo test -p delilah-daemon, cargo test -p delilah-git
- Reviewers: security

## WP-08 US2 guard hook and branch retry endpoint
- Phase: 4 US2 The branch is named after the analysis
- Tasks: T029, T030, T031, T032, T033, T034
- Files: crates/delilah-agents/src/claude.rs, crates/delilah-daemon/src/run.rs, crates/delilah-daemon/src/branching.rs, crates/delilah-daemon/src/hooks.rs, crates/delilah-daemon/src/api.rs
- Depends on: WP-07
- Parallel group: 4C
- Acceptance: FR-007, FR-011 — branchless sessions get a PreToolUse guard that runs ensure(Guard) before edits outside specs/ and commit/push commands (deny on failure); POST /api/v1/tasks/{key}/branch retries (404/409 as specified).
- Checks: cargo test -p delilah-agents, cargo test -p delilah-daemon
- Reviewers: security, contract

## WP-11 Docs and checklist
- Phase: 5 Polish
- Tasks: T042, T043
- Files: docs/architecture.md, docs/status.md, docs/decisions.md, README.md, specs/005-task-name-from-spec/checklists/workspace.md
- Depends on: WP-08, WP-09, WP-10
- Parallel group: 5A
- Acceptance: all FRs documented — architecture, status, decisions and README describe the feature; answered checklist items ticked with a pointer to research.md.
- Checks: — (docs only)
- Reviewers: —

## WP-12 Full checks and quickstart walk
- Phase: 5 Polish
- Tasks: T044, T045
- Files: — (no edits expected)
- Depends on: WP-11
- Parallel group: 5B
- Acceptance: SC — cargo test --workspace, cargo test --manifest-path app/src-tauri/Cargo.toml, cd app && npx tsc --noEmit && npx vitest run, cargo clippy --workspace all clean; quickstart.md walked in `scripts/preview.sh --fresh`, never on 7420.
- Checks: cargo test --workspace, cargo test --manifest-path app/src-tauri/Cargo.toml, app (full), cargo clippy --workspace
- Reviewers: —
