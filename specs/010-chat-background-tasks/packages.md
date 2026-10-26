# Work packages — 010-chat-background-tasks

Source: specs/010-chat-background-tasks/tasks.md · 59 pending tasks · 18 packages

Check names: `cargo:agents` = `cargo test -p delilah-agents`; `cargo:daemon` = `cargo test -p delilah-daemon`; `app` = `cd app && npx tsc --noEmit && npx vitest related --run <files>`; `cargo:workspace` = `cargo test --workspace`.

## WP-01 Capture real background streams and freeze fixtures
- Phase: 1 Setup
- Tasks: T001, T002
- Files: crates/delilah-agents/examples/claude_stream_spike.rs, crates/delilah-agents/tests/fixtures/stream-background.jsonl, crates/delilah-agents/tests/fixtures/stream-background-stop.jsonl, crates/delilah-agents/tests/fixtures/stream-background-hooks.jsonl, specs/010-chat-background-tasks/research.md, docs/spikes/2026-10-24-claude-stream.md
- Depends on: —
- Parallel group: 1A
- Acceptance: FR-017 — fixtures exist with home paths stripped; R4/R8 answers (Stop on background turns, UserPromptSubmit on self-started turns, stop_task support) written down
- Checks: cargo:agents (example builds)
- Reviewers: —

## WP-02 Stream parser: sub-agent routing, task events, self_started
- Phase: 2 Foundational
- Tasks: T003, T004
- Files: crates/delilah-agents/src/stream.rs, crates/delilah-agents/src/runtime.rs, crates/delilah-daemon/src/stream_events.rs
- Depends on: WP-01
- Parallel group: 2A
- Acceptance: FR-002, FR-005, FR-007, FR-008, FR-017 — new StreamEvent variants parsed; sub-agent lines never corrupt main text; Init carries self_started; all StreamEvent::Init matches updated (3 files found by grep)
- Checks: cargo:agents, cargo:daemon
- Reviewers: —

## WP-03 Transcript: background-done marker and step summaries
- Phase: 2 Foundational
- Tasks: T005, T006
- Files: crates/delilah-agents/src/transcript.rs, crates/delilah-daemon/src/conversation.rs
- Depends on: WP-01
- Parallel group: 2A
- Acceptance: FR-015, FR-017 — task-notification user lines become `background-done` markers with detail; `tool_step_summary` shared and used by conversation.rs
- Checks: cargo:agents, cargo:daemon
- Reviewers: contract

## WP-04 Progress model and reducer
- Phase: 2 Foundational
- Tasks: T007, T008
- Files: crates/delilah-daemon/src/progress.rs, crates/delilah-daemon/src/lib.rs
- Depends on: WP-02
- Parallel group: 2B
- Acceptance: FR-001..FR-008, FR-016 — pure `apply`/`tick` reducer and serde shapes exactly as contracts/api.md, tested by replaying the fixture
- Checks: cargo:daemon
- Reviewers: contract, architecture

## WP-05 Live progress channel, held-Stop state and pump wiring
- Phase: 2 Foundational
- Tasks: T009, T010, T011, T012
- Files: crates/delilah-daemon/src/sessions.rs, crates/delilah-daemon/src/stream_events.rs
- Depends on: WP-04, WP-03
- Parallel group: 2C
- Acceptance: FR-004, FR-009, FR-016 — Live holds progress in a watch channel and an atomic held-Stop; pump feeds the reducer and runs the 5 s tick only while final rows wait
- Checks: cargo:daemon
- Reviewers: architecture

## WP-06 Daemon exposure of progress (REST, socket, PreToolUse activity)
- Phase: 3 User Story 1
- Tasks: T013, T014, T015, T016, T017, T018
- Files: crates/delilah-daemon/src/conversation.rs, crates/delilah-daemon/src/live.rs
- Depends on: WP-05
- Parallel group: 3A
- Acceptance: FR-001, FR-002, FR-004, FR-018 — conversation GET returns `progress`; socket sends `progress` on open and throttled (250 ms); PTY PreToolUse sets activity
- Checks: cargo:daemon
- Reviewers: contract

## WP-07 Working indicator in the UI
- Phase: 3 User Story 1
- Tasks: T019, T020, T021, T022, T023, T024, T025
- Files: app/src/api.ts, app/src/live.ts, app/src/live.test.ts, app/src/components/WorkingIndicator.tsx, app/src/components/WorkingIndicator.test.tsx, app/src/components/ConversationView.css, app/src/i18n/en/WorkingIndicator.ts, app/src/components/ConversationView.tsx, app/src/components/ConversationView.test.tsx
- Depends on: WP-05
- Parallel group: 3A
- Acceptance: FR-001, FR-002, FR-003, FR-004, FR-019 — indicator with turn clock, step and "sin novedades desde hace…"; reload keeps server started_at; fallback to old activity line
- Checks: app
- Reviewers: contract

## WP-08 Sub-agent card progress
- Phase: 4 User Story 2
- Tasks: T026, T027, T028, T029
- Files: app/src/components/ToolCard.tsx, app/src/components/ToolCard.css, app/src/components/ToolCard.test.tsx, app/src/i18n/en/ToolCard.ts (guess), app/src/components/ConversationView.tsx, app/src/components/ConversationView.test.tsx
- Depends on: WP-07
- Parallel group: 4A
- Acceptance: FR-005, FR-006, FR-007, FR-019 — Agent card shows state, clock, step count, latest step and recent list; parallel sub-agents each show only their own steps
- Checks: app
- Reviewers: —

## WP-09 Self-started turns (daemon)
- Phase: 5 User Story 4
- Tasks: T030, T031, T032
- Files: crates/delilah-daemon/src/stream_events.rs, crates/delilah-daemon/src/hooks.rs, crates/delilah-daemon/src/phases.rs, crates/delilah-daemon/src/conversation.rs
- Depends on: WP-06
- Parallel group: 5A
- Acceptance: FR-014, FR-015 — Init{self_started} moves session and task to working with history `agent-resumed`; synthetic prompt hook (`delilah_origin: agent`) idempotent and skipped by phases/note_activity
- Checks: cargo:daemon
- Reviewers: architecture

## WP-10 Agent-resumed history and background-done divider (UI)
- Phase: 5 User Story 4
- Tasks: T033, T034, T035, T036
- Files: app/src/history.ts, app/src/history.test.ts, app/src/i18n/en/history.ts, app/src/components/ConversationView.tsx, app/src/components/ConversationView.test.tsx, app/src/i18n/en/ConversationView.ts
- Depends on: WP-08
- Parallel group: 5A
- Acceptance: FR-015, FR-017, FR-019 — `agent-resumed` history text; `background-done` divider with detail; unknown marker kinds render nothing
- Checks: app
- Reviewers: —

## WP-11 Held Stop deferral and message/interrupt handling
- Phase: 6 User Story 3
- Tasks: T037, T038, T039, T040
- Files: crates/delilah-daemon/src/hooks.rs, crates/delilah-daemon/src/conversation.rs
- Depends on: WP-09
- Parallel group: 6A
- Acceptance: FR-009, FR-010, FR-011 — Stop with background running is held (state stays Working, no inbox/review/phase effects); never held for failures, limits, PTY or a later turn; queued messages flushed; messages typed directly when only background runs; interrupt sets Waiting
- Checks: cargo:daemon
- Reviewers: architecture

## WP-12 Release of held Stop (30 s fallback and process exit)
- Phase: 6 User Story 3
- Tasks: T041, T042, T043, T044
- Files: crates/delilah-daemon/src/stream_events.rs, crates/delilah-daemon/src/run.rs
- Depends on: WP-11
- Parallel group: 6B
- Acceptance: FR-009, FR-012 — 30 s timer replays held Stop when background empties; any Init cancels it; process exit replays before exit handling and marks running rows stopped
- Checks: cargo:daemon
- Reviewers: —

## WP-13 Refuse runtime switch while background runs
- Phase: 6 User Story 3
- Tasks: T045, T046
- Files: crates/delilah-daemon/src/runtime_api.rs
- Depends on: WP-05
- Parallel group: 6A
- Acceptance: FR-013 — POST /api/v1/tasks/{key}/runtime answers 409 (Spanish/English by daemon lang) while background runs
- Checks: cargo:daemon
- Reviewers: contract

## WP-14 Stop background tasks on "Detener" (conditional on stop_task)
- Phase: 6 User Story 3
- Tasks: T047, T048
- Files: crates/delilah-agents/src/stream.rs, crates/delilah-daemon/src/conversation.rs
- Depends on: WP-11
- Parallel group: 6B
- Acceptance: FR-011 — `StreamSession::stop_task` and sessions stop endpoint send it per running row; if WP-01 found stop_task unsupported, record in research.md R8 and skip T048
- Checks: cargo:agents, cargo:daemon
- Reviewers: contract

## WP-15 Background strip component
- Phase: 6 User Story 3
- Tasks: T049, T050
- Files: app/src/components/BackgroundStrip.tsx, app/src/components/BackgroundStrip.test.tsx, app/src/components/BackgroundStrip.css (guess), app/src/i18n/en/BackgroundStrip.ts
- Depends on: WP-07
- Parallel group: 6A
- Acceptance: FR-008, FR-010, FR-019 — one row per item with description, ticking clock or total time, five state labels, "desde hace ?" when started_at is null
- Checks: app
- Reviewers: —

## WP-16 Strip, Detener and composer in ConversationView
- Phase: 6 User Story 3
- Tasks: T051, T052
- Files: app/src/components/ConversationView.tsx, app/src/components/ConversationView.test.tsx
- Depends on: WP-15, WP-10, WP-11
- Parallel group: 6B
- Acceptance: FR-010, FR-011 — strip above composer from progress.background; "Detener" visible while background runs with no turn; composer sends instead of queueing
- Checks: app
- Reviewers: —

## WP-17 Docs and i18n coverage
- Phase: 7 Polish
- Tasks: T053, T054, T055, T056, T057
- Files: docs/architecture.md, docs/status.md, README.md, docs/decisions.md, app/src/i18n/coverage.test.ts
- Depends on: WP-16, WP-14, WP-12, WP-13
- Parallel group: 7A
- Acceptance: FR-019 — docs describe new behaviour and decision; every new Spanish string has its English entry
- Checks: app (vitest src/i18n/coverage.test.ts)
- Reviewers: —

## WP-18 Full checks and quickstart run
- Phase: 7 Polish
- Tasks: T058, T059
- Files: specs/010-chat-background-tasks/research.md
- Depends on: WP-17
- Parallel group: 7B
- Acceptance: SC-003, SC-004 — workspace tests, clippy and full vitest green; quickstart US1–US4 and Detener variant run in a preview
- Checks: cargo:workspace, cargo clippy --workspace -- -D warnings, app (npx tsc --noEmit && npx vitest run)
- Reviewers: —
