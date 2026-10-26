# Work packages — 011-readable-task-notifications

Source: specs/011-readable-task-notifications/tasks.md · 36 pending tasks · 12 packages

## WP-01 Rust background module skeleton
- Phase: 1 Setup
- Tasks: T001
- Files: crates/delilah-agents/src/transcript/background.rs, crates/delilah-agents/src/transcript.rs
- Depends on: —
- Parallel group: 1A
- Acceptance: FR-012 — new submodule compiles empty, tests stay green
- Checks: cargo test -p delilah-agents
- Reviewers: architecture

## WP-02 Shared UI formatters
- Phase: 1 Setup
- Tasks: T002, T003
- Files: app/src/format.ts, app/src/format.test.ts, app/src/components/BackgroundStrip.tsx, app/src/playground/art/kinds.ts
- Depends on: —
- Parallel group: 1A
- Acceptance: FR-004, FR-013 — duration/formatTokens live in format.ts; BackgroundStrip and playground tests unchanged and green
- Checks: cd app && npx tsc --noEmit && npx vitest related --run (changed files)
- Reviewers: —

## WP-03 Rust background field (marker contract)
- Phase: 2 Foundational
- Tasks: T004, T005
- Files: crates/delilah-agents/src/transcript/background.rs, crates/delilah-agents/src/transcript.rs, (every other `Entry::Marker {` construction under crates/, guess: crates/delilah-daemon/src/*)
- Depends on: WP-01
- Parallel group: 2A
- Acceptance: FR-012 — Entry::Marker serializes an optional `background`; absent when None, existing markers byte-identical
- Checks: cargo test -p delilah-agents, cargo test -p delilah-daemon
- Reviewers: contract

## WP-04 TS BackgroundInfo type
- Phase: 2 Foundational
- Tasks: T006
- Files: app/src/api.ts
- Depends on: —
- Parallel group: 2A
- Acceptance: FR-012 — BackgroundInfo type and optional `background` on the marker entry; tsc passes
- Checks: cd app && npx tsc --noEmit
- Reviewers: contract

## WP-05 Notification parsing (Rust)
- Phase: 3 US1
- Tasks: T007, T008, T011, T012
- Files: crates/delilah-agents/src/transcript/background.rs, crates/delilah-agents/src/transcript.rs
- Depends on: WP-03
- Parallel group: 3A
- Acceptance: FR-001, FR-002, FR-003, FR-004, FR-010 — each notification yields one marker with `background` filled (kind, label, outcome, metrics, no boilerplate)
- Checks: cargo test -p delilah-agents
- Reviewers: security

## WP-06 BackgroundDoneItem row (UI)
- Phase: 3 US1
- Tasks: T009, T010, T013, T014, T015
- Files: app/src/backgroundItems.ts, app/src/backgroundItems.test.ts, app/src/components/BackgroundDoneItem.tsx, app/src/components/BackgroundDoneItem.css, app/src/components/BackgroundDoneItem.test.tsx, app/src/i18n/en/BackgroundDoneItem.ts, app/src/i18n/ (catalogue registration, guess: app/src/i18n/index.ts), app/src/components/ConversationView.tsx, app/src/components/ConversationView.test.tsx
- Depends on: WP-02, WP-04
- Parallel group: 3A
- Acceptance: FR-001, FR-002, FR-003, FR-004, FR-005, FR-014 — background-done markers render as one compact row with outcome styling; old-daemon fallback line; i18n coverage passes
- Checks: cd app && npx tsc --noEmit && npx vitest related --run (changed files) && npx vitest run src/i18n/coverage.test.ts
- Reviewers: —

## WP-07 Report and hand-back parsing (Rust)
- Phase: 4 US2
- Tasks: T016, T017, T018, T021, T022, T023
- Files: crates/delilah-agents/src/transcript/background.rs, crates/delilah-agents/src/transcript.rs
- Depends on: WP-05
- Parallel group: 4A
- Acceptance: FR-006, FR-007, FR-008, FR-010 — report decoded and capped at 16 KB; hand-back isMeta lines become Handback markers
- Checks: cargo test -p delilah-agents
- Reviewers: security

## WP-08 Expandable report and fold (UI)
- Phase: 4 US2
- Tasks: T019, T020, T024, T025, T026
- Files: app/src/backgroundItems.ts, app/src/backgroundItems.test.ts, app/src/components/BackgroundDoneItem.tsx, app/src/components/BackgroundDoneItem.test.tsx, app/src/i18n/en/BackgroundDoneItem.ts, app/src/components/ConversationView.tsx, app/src/components/ConversationView.test.tsx
- Depends on: WP-06
- Parallel group: 4A
- Acceptance: FR-006, FR-007, FR-008, FR-014 — item expands to Markdown report; notification and hand-back fold into one item, also across pages
- Checks: cd app && npx tsc --noEmit && npx vitest related --run (changed files)
- Reviewers: —

## WP-09 Notification splitting in any message shape (Rust)
- Phase: 5 US3
- Tasks: T027, T028, T030, T031
- Files: crates/delilah-agents/src/transcript/background.rs, crates/delilah-agents/src/transcript.rs
- Depends on: WP-07
- Parallel group: 5A
- Acceptance: FR-009, FR-010, FR-011 — multiple/embedded notifications give one marker each; no User entry contains raw notification or agent-message markup; `is_agent_message` exported
- Checks: cargo test -p delilah-agents
- Reviewers: security

## WP-10 Hooks last-message guard
- Phase: 5 US3
- Tasks: T029, T032
- Files: crates/delilah-daemon/src/hooks.rs
- Depends on: WP-09
- Parallel group: 5B
- Acceptance: FR-011 — agent-message and notification-embedded prompts never become the session's last message
- Checks: cargo test -p delilah-daemon
- Reviewers: security

## WP-11 Docs and decision
- Phase: 6 Polish
- Tasks: T033, T034
- Files: docs/status.md, docs/usage.md, docs/decisions.md, docs/architecture.md
- Depends on: WP-10, WP-08
- Parallel group: 6A
- Acceptance: project docs updated, decision 97 added
- Checks: —
- Reviewers: —

## WP-12 Full checks and preview run
- Phase: 6 Polish
- Tasks: T035, T036
- Files: specs/011-readable-task-notifications/quickstart.md
- Depends on: WP-11
- Parallel group: 6B
- Acceptance: FR-012, FR-013 — workspace tests, clippy, fmt, tsc, vitest green; quickstart run in a preview and recorded
- Checks: cargo test --workspace, cargo clippy --workspace --all-targets -- -D warnings, cargo fmt --check, cd app && npx tsc --noEmit && npx vitest run
- Reviewers: —
