# Work packages — 004-task-branch-names

Source: specs/004-task-branch-names/tasks.md · 27 pending tasks · 11 packages

## WP-01 Config: claude_bin
- Phase: 1 Setup
- Tasks: T001
- Files: crates/delilah-daemon/src/config.rs
- Depends on: —
- Parallel group: 1A
- Acceptance: FR-012, FR-013 — `Config.claude_bin` reads `DELILAH_CLAUDE_BIN` (default `claude`) and can be set in tests so fake claude scripts work
- Checks: cargo test -p delilah-daemon
- Reviewers: security (env-controlled binary path)

## WP-02 Core: branch rules
- Phase: 2 Foundational
- Tasks: T002, T005
- Files: crates/delilah-core/src/branch.rs, crates/delilah-core/src/lib.rs
- Depends on: —
- Parallel group: 2A
- Acceptance: FR-001, FR-002, FR-003, FR-004, FR-005, FR-008 — `slug`, `title_slug`, `compose`, `custom`, `fallback` behave as the unit tests specify (feature/ prefix, transliteration, 50-char cut, Jira key kept, LIBRE-n dropped)
- Checks: cargo test -p delilah-core
- Reviewers: architecture

## WP-03 Agents: Claude branch-name adapter
- Phase: 2 Foundational
- Tasks: T003, T006
- Files: crates/delilah-agents/src/branch_name.rs, crates/delilah-agents/src/lib.rs
- Depends on: —
- Parallel group: 2A
- Acceptance: FR-012, FR-013 — prompt, tool-less haiku args and output parsing (JSON in prose, is_error, empty name) tested; reuses `suggest::env()`
- Checks: cargo test -p delilah-agents
- Reviewers: architecture, security (claude CLI args, parsing untrusted output)

## WP-04 Git: branch_taken and free_branch
- Phase: 2 Foundational
- Tasks: T004, T007
- Files: crates/delilah-git/tests/workspace.rs, crates/delilah-git/src/workspace.rs, crates/delilah-git/src/lib.rs
- Depends on: —
- Parallel group: 2A
- Acceptance: FR-006 — a name taken locally or on a remote in any repo of the workspace gets `-2`…`-99`, then a unix-time suffix
- Checks: cargo test -p delilah-git
- Reviewers: security (git calls with branch names)

## WP-05 Daemon: name the branch on start
- Phase: 3 US1
- Tasks: T008, T009, T010, T011, T012
- Files: crates/delilah-daemon/src/branch_name.rs, crates/delilah-daemon/src/run.rs, crates/delilah-daemon/src/main.rs, crates/delilah-daemon/src/lib.rs (guess, if it exists), crates/delilah-cli/src/main.rs
- Depends on: WP-01, WP-02, WP-03, WP-04
- Parallel group: 3A
- Acceptance: FR-001, FR-002, FR-003, FR-006, FR-008, FR-009, FR-010, FR-012, FR-013 — a task started without `branch` works on a Claude/title/key-named unique `feature/…` branch, stored in the workspace; typed names normalised (400 if unusable); checkout mode never reuses an existing branch
- Checks: cargo test -p delilah-daemon, cargo test -p delilah-cli (help text only), cargo clippy --workspace --all-targets -- -D warnings
- Reviewers: security (spawns the claude process, branch names reach git), architecture (new daemon module, crosses core/agents/git/daemon), contract (RunRequest.branch semantics)

## WP-06 Daemon: branch-name endpoint
- Phase: 4 US2
- Tasks: T013, T015
- Files: crates/delilah-daemon/src/branch_name.rs, crates/delilah-daemon/src/api.rs, crates/delilah-daemon/src/run.rs
- Depends on: WP-05
- Parallel group: 4A
- Acceptance: FR-007, FR-008, FR-012 — `POST /api/v1/projects/{name}/branch-name` returns `{branch, source}` (200), 400 for empty title, 404 for unknown project; `start_run` normalises typed names and leaves a backlog task in backlog on 400
- Checks: cargo test -p delilah-daemon
- Reviewers: contract, security

## WP-07 App: Rama field in start dialog
- Phase: 4 US2
- Tasks: T014, T016, T017, T018
- Files: app/src/components/NewTaskDialog.test.tsx, app/src/components/NewTaskDialog.tsx, app/src/api.ts, app/src/i18n/en.ts
- Depends on: —
- Parallel group: 4A
- Acceptance: FR-007, FR-008 — dialog shows an editable "Rama" field filled by a debounced proposal, stale answers ignored, sends `branch` only when non-empty, in all variants; `api.proposeBranch` and `BranchProposal` typed to match contracts/api.md (runtime needs WP-06)
- Checks: cd app && npx tsc --noEmit && npx vitest related --run src/components/NewTaskDialog.tsx src/api.ts
- Reviewers: contract

## WP-08 Backend: Task.branch
- Phase: 5 US3
- Tasks: T019, T021
- Files: crates/delilah-daemon/src/store.rs, crates/delilah-core/src/task.rs (plus any `Task { … }` literals that stop compiling, files unknown)
- Depends on: —
- Parallel group: 5A
- Acceptance: FR-010, FR-011 — `get_task`/`list_tasks` return `branch` from the stored workspace, `None` for backlog or after delete, field omitted in JSON when absent
- Checks: cargo test --workspace
- Reviewers: contract

## WP-09 App: show branch on task view
- Phase: 5 US3
- Tasks: T020, T022
- Files: app/src/components/TaskScreen.test.tsx (or app/src/components/TaskView.test.tsx), app/src/components/TaskView.tsx, app/src/api.ts
- Depends on: WP-07
- Parallel group: 5A
- Acceptance: FR-011 — `TaskView` shows the branch in monospace under the title when `task.branch` is set, nothing otherwise
- Checks: cd app && npx tsc --noEmit && npx vitest related --run src/components/TaskView.tsx
- Reviewers: contract

## WP-10 Docs
- Phase: 6 Polish
- Tasks: T023, T024, T025, T026
- Files: docs/status.md, README.md, docs/architecture.md, docs/decisions.md
- Depends on: WP-06, WP-08, WP-09
- Parallel group: 6A
- Acceptance: FR-001..FR-013 — docs describe the naming flow, dialog field, Task.branch, DELILAH_CLAUDE_BIN and the decisions taken
- Checks: —
- Reviewers: —

## WP-11 Final verification
- Phase: 6 Polish
- Tasks: T027
- Files: — (verification only; fixes go to the owning files)
- Depends on: WP-10
- Parallel group: 6B
- Acceptance: all success criteria — workspace tests, clippy, fmt, tsc and vitest green; quickstart passes in a preview
- Checks: cargo test --workspace, cargo clippy --workspace --all-targets -- -D warnings, cargo fmt --check, cd app && npx tsc --noEmit && npx vitest run
- Reviewers: —
