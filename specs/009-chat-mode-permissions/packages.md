# Work packages — 009 Chat mode asks for permission as often as terminal mode

Source: specs/009-chat-mode-permissions/tasks.md · 25 pending tasks · 8 packages

## WP-01 Spike against the real CLI (gate)
- Phase: 1 Setup
- Tasks: T001, T002
- Files: crates/delilah-agents/examples/claude_stream_spike.rs, docs/spikes/2026-10-26-chat-permission-mode.md, specs/009-chat-mode-permissions/research.md, specs/009-chat-mode-permissions/plan.md (only if the gate fails)
- Depends on: —
- Parallel group: 1A
- Acceptance: FR-001 — spike record exists with CLI version, per-mode action table, escalation channel, rejection text and PTY baseline; R1 confirmed or plan revised. T002 runs the real `claude` and spends tokens, so run it from a plain terminal.
- Checks: cargo build -p delilah-agents --examples
- Reviewers: —

## WP-02 Permission::Auto end to end in the daemon (tests first)
- Phase: 2 Foundational
- Tasks: T003, T004, T005, T006, T007, T008, T009, T010, T011, T012, T013
- Files: crates/delilah-agents/src/claude.rs, crates/delilah-e2e/src/fake/stream.rs, crates/delilah-e2e/src/fake/scenario.rs, crates/delilah-e2e/src/fake/args.rs (guess, only if it needs to parse the mode), crates/delilah-daemon/src/preferences.rs, crates/delilah-daemon/src/run.rs, crates/delilah-daemon/src/state.rs, crates/delilah-daemon/src/main.rs, crates/delilah-daemon/src/api.rs, app/src/api.ts, specs/009-chat-mode-permissions/progress.md
- Depends on: WP-01
- Parallel group: 2A
- Acceptance: FR-002, FR-003, FR-004, FR-010 — task and project sessions launch with `--permission-mode auto` in PTY and chat by default and `Default` adds no flag. The `agent-permissions` preference is validated (400 on bad values) and takes effect at once. Idea and read-only sessions are unchanged. Red then green is recorded in progress.md.
- Checks: cargo test -p delilah-agents, cargo test -p delilah-daemon, cargo test -p delilah-e2e --lib, cargo clippy --workspace --all-targets -- -D warnings, cargo fmt --all --check
- Reviewers: contract, security, architecture

## WP-03 US1 e2e proof and start-failure hint
- Phase: 3 User Story 1
- Tasks: T014, T015
- Files: crates/delilah-e2e/tests/chat_permissions.rs, crates/delilah-daemon/src/run.rs, crates/delilah-daemon/src/stream_events.rs, crates/delilah-e2e/src/fake/scenario.rs, crates/delilah-e2e/src/fake/run.rs (guess; `exit_with_stderr` step), specs/009-chat-mode-permissions/progress.md
- Depends on: WP-02
- Parallel group: 3A
- Acceptance: FR-003, FR-005, FR-007, FR-008 — a chat-mode autopilot run of routine work raises no card and no pause. A real ask (`auto: "ask"`) still pauses with "needs permission: Bash". A runtime switch relaunches with the current mode. A CLI rejection at start surfaces the Settings hint on both runtimes, in both languages.
- Checks: cargo test -p delilah-e2e, cargo test -p delilah-daemon, cargo clippy --workspace --all-targets -- -D warnings
- Reviewers: —

## WP-04 US2 "Claude's default" regression e2e
- Phase: 4 User Story 2
- Tasks: T016
- Files: crates/delilah-e2e/tests/chat_permissions.rs
- Depends on: WP-03
- Parallel group: 4A
- Acceptance: FR-004, FR-006 — with the preference at `"default"`, the four-tool chat scenario gives four cards with no `--permission-mode` in argv. A remembered "always in this task" rule still answers without a card.
- Checks: cargo test -p delilah-e2e
- Reviewers: —

## WP-05 US2 Settings control for agent permissions
- Phase: 4 User Story 2
- Tasks: T017, T018
- Files: app/src/components/SettingsView.test.tsx, app/src/components/SettingsView.tsx, app/src/api.ts (read only, type added in WP-02)
- Depends on: WP-02
- Parallel group: 4A
- Acceptance: FR-010 — the Settings section shows Auto for null or "auto" and "Claude's default" for "default". Choosing an option calls `setPreference("agent-permissions", value)`. A failed save shows the error and restores the previous choice. Text in both languages.
- Checks: cd app && npx tsc --noEmit, cd app && npx vitest related --run src/components/SettingsView.tsx
- Reviewers: —

## WP-06 US3 evidence check and full suite
- Phase: 5 User Story 3
- Tasks: T019, T020
- Files: docs/spikes/2026-10-26-chat-permission-mode.md, crates/delilah-e2e/src/fake/scenario.rs, crates/delilah-e2e/src/fake/stream.rs, crates/delilah-e2e/tests/chat_permissions.rs, specs/009-chat-mode-permissions/progress.md
- Depends on: WP-03, WP-04, WP-05
- Parallel group: 5A
- Acceptance: FR-002, FR-007 — the fake's `auto` semantics and the e2e expectations match the spike table. The whole suite is green, including the existing permission-card, remembered-rule and autopilot-pause tests (SC-004).
- Checks: cargo test --workspace, cargo test --manifest-path app/src-tauri/Cargo.toml, cd app && npx vitest run, cargo clippy --workspace --all-targets -- -D warnings, cargo fmt --all --check
- Reviewers: —

## WP-07 Docs
- Phase: 6 Polish
- Tasks: T021, T022, T023, T024
- Files: docs/status.md, docs/architecture.md, docs/decisions.md, README.md
- Depends on: WP-03
- Parallel group: 6A
- Acceptance: FR-009 — the docs describe how chat mode chooses its permission behaviour, the setting, the new numbered decision, and the remaining limits. No version bump.
- Checks: —
- Reviewers: —

## WP-08 Preview verification
- Phase: 6 Polish
- Tasks: T025
- Files: specs/009-chat-mode-permissions/progress.md
- Depends on: WP-06, WP-07
- Parallel group: 6B
- Acceptance: SC-001, FR-010 — in a preview (never the installed app) the setting shows Auto, routine chat work shows no card, and after switching to "Claude's default" and the runtime, cards appear. Verdict recorded.
- Checks: ui-verifier agent on scripts/preview.sh --fresh
- Reviewers: —
