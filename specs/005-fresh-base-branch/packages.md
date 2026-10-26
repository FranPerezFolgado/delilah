# Work packages — 005-fresh-base-branch

Source: specs/005-fresh-base-branch/tasks.md · 32 pending tasks · 10 packages

## WP-01 Test helpers for remote fixtures
- Phase: 1 Setup
- Tasks: T001
- Files: crates/delilah-git/tests/workspace.rs, crates/delilah-git/tests/common/mod.rs (guess)
- Depends on: —
- Parallel group: 1A
- Acceptance: supports FR-001..FR-003 tests — `advance_remote`, `remote_head`, `upstream_of` exist and are usable from both test files
- Checks: cargo test -p delilah-git
- Reviewers: —

## WP-02 Bounded git calls, per-repo fetch lock, base resolution
- Phase: 2 Foundational
- Tasks: T002, T003, T004, T005, T006
- Files: crates/delilah-git/src/command.rs, crates/delilah-git/src/base.rs, crates/delilah-git/src/workspace.rs, crates/delilah-git/src/lib.rs, crates/delilah-git/tests/base.rs
- Depends on: WP-01
- Parallel group: 2A
- Acceptance: FR-001, FR-002, FR-007, FR-008, FR-009, FR-012 — `Deadline`, `git_with_timeout`, `prepare_base` (fetch + resolve, lock, timeout, no prompt) and the locked `fetch()` work; nothing calls `prepare_base` yet
- Checks: cargo test -p delilah-git
- Reviewers: architecture, security

## WP-03 US1 Worktree tasks on repo projects start fresh
- Phase: 3 US1
- Tasks: T007, T008, T009, T010, T011
- Files: crates/delilah-git/src/workspace.rs, crates/delilah-git/tests/workspace.rs, crates/delilah-daemon/src/run.rs, crates/delilah-daemon/src/actions.rs
- Depends on: WP-02
- Parallel group: 3A
- Acceptance: FR-001, FR-002, FR-003, FR-006, FR-010, FR-011 — new `create_workspace` signature returns bases; worktree branch starts at the remote head, no upstream, project folder untouched; `start_run` logs bases via `report_bases`
- Checks: cargo test -p delilah-git, cargo test -p delilah-daemon
- Reviewers: —

## WP-04 US2 Checkout-mode tasks start fresh
- Phase: 4 US2
- Tasks: T012, T013, T014
- Files: crates/delilah-git/src/workspace.rs, crates/delilah-git/tests/workspace.rs, crates/delilah-daemon/src/run.rs, crates/delilah-daemon/src/actions.rs
- Depends on: WP-03
- Parallel group: 4A
- Acceptance: FR-003, FR-005, FR-006, FR-011 — new branch in the project folder starts at the fresh remote head; an existing branch is used untouched (`Existing`); dirty check precedes any fetch
- Checks: cargo test -p delilah-git, cargo test -p delilah-daemon
- Reviewers: —

## WP-05 US3 Parent projects, per-submodule fresh bases
- Phase: 5 US3
- Tasks: T015, T016
- Files: crates/delilah-git/src/workspace.rs, crates/delilah-git/tests/workspace.rs
- Depends on: WP-04
- Parallel group: 5A
- Acceptance: FR-001, FR-004, FR-004a, FR-005 — parent worktree on parent's fresh default; each chosen submodule on its own fresh default in both modes; parent folder never gets a branch in checkout mode
- Checks: cargo test -p delilah-git
- Reviewers: —

## WP-06 US4 Daemon: base-stale notices and inbox item
- Phase: 6 US4
- Tasks: T017, T018, T019, T020
- Files: crates/delilah-daemon/src/run.rs, crates/delilah-daemon/src/store_prs.rs, crates/delilah-daemon/src/inbox.rs
- Depends on: WP-04
- Parallel group: 6A
- Acceptance: FR-007, FR-009 — a failed fetch records an open `base-stale` notice; the Bandeja lists it (`notice` id, no PR), dismiss/archive resolves it; NoRemote/Existing/success record nothing
- Checks: cargo test -p delilah-daemon
- Reviewers: contract

## WP-07 US4 UI: Bandeja item and task banner
- Phase: 6 US4
- Tasks: T021, T022, T023, T024, T025, T026
- Files: app/src/api.ts, app/src/alerts.ts, app/src/components/InboxView.tsx, app/src/components/InboxView.test.tsx, app/src/components/TaskScreen.tsx, app/src/components/TaskScreen.test.tsx, app/src/App.tsx, app/src/i18n/en.ts
- Depends on: —
- Parallel group: 6A
- Acceptance: FR-007 — `base-stale` item in the Bandeja with "Descartar" (`api.dismissNotice`), banner on the task screen, English strings; not blocking
- Checks: app (cd app && npx tsc --noEmit && npx vitest related --run <files>)
- Reviewers: contract

## WP-08 Docs
- Phase: 7 Polish
- Tasks: T027, T028, T029
- Files: docs/architecture.md, docs/status.md, docs/decisions.md, README.md
- Depends on: WP-06, WP-07
- Parallel group: 7A
- Acceptance: FR-010 and all — docs describe fresh bases, the sha start point, warn-and-continue, `base-stale` notices
- Checks: —
- Reviewers: —

## WP-09 Full suite and preview walkthrough
- Phase: 7 Polish
- Tasks: T030, T031
- Files: specs/005-fresh-base-branch/quickstart.md (read only; fixes in any file the failures point to)
- Depends on: WP-08
- Parallel group: 7B
- Acceptance: SC-001..SC-005 — `cargo test --workspace`, the Tauri shell tests and `npx vitest run` green; quickstart scenarios 1-5 pass in a preview (never port 7420)
- Checks: cargo test --workspace, cargo test --manifest-path app/src-tauri/Cargo.toml, app (npx tsc --noEmit && npx vitest run)
- Reviewers: —

## WP-10 Version bump 0.4.0 to 0.5.0
- Phase: 7 Polish
- Tasks: T032
- Files: Cargo.toml, app/src-tauri/Cargo.toml, app/package.json, app/src-tauri/tauri.conf.json, Cargo.lock, app/src-tauri/Cargo.lock, app/package-lock.json
- Depends on: WP-09
- Parallel group: 7C
- Acceptance: minor bump in the four places plus lockfiles, own commit `chore: bump version to 0.5.0`; re-check main's version first
- Checks: cargo check --workspace, cargo check --manifest-path app/src-tauri/Cargo.toml
- Reviewers: —
