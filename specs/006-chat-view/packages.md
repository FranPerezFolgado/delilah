# Work packages — 006-chat-view

Source: specs/006-chat-view/tasks.md · 70 pending tasks · 31 packages

T071 (version bump) is left out on purpose: the orchestrator does it.
Phases 6–9 (WP-14 to WP-29) only run if the spike (WP-02) says "go"; "adjust" rewrites them first; "drop" deletes them. WP-25 only runs if the spike's d2 was "no".

## WP-01 Add `similar` dependency
- Phase: 1 Setup
- Tasks: T001
- Files: crates/delilah-agents/Cargo.toml, Cargo.lock
- Depends on: —
- Parallel group: 1A
- Acceptance: plan.md — `similar` 2.x builds in delilah-agents
- Checks: cargo build -p delilah-agents
- Reviewers: —

## WP-02 Spike: is chat mode viable
- Phase: 2 US5 Spike
- Tasks: T002, T003, T004
- Files: crates/delilah-agents/examples/claude_stream_spike.rs, docs/spikes/2026-10-2x-claude-stream.md, docs/decisions.md, specs/006-chat-view/tasks.md
- Depends on: —
- Parallel group: 2A
- Acceptance: US5 — spike doc answers R7 a–h with the Claude version; decisions.md has the go/adjust/drop verdict; Phases 6–9 of tasks.md match it
- Checks: cargo build -p delilah-agents --examples
- Reviewers: security

## WP-03 Split idea logic out of conversation.rs
- Phase: 3 Foundational
- Tasks: T005
- Files: crates/delilah-daemon/src/conversation.rs, crates/delilah-daemon/src/idea.rs, crates/delilah-daemon/src/sessions.rs, crates/delilah-daemon/src/hooks.rs (guess: callers of is_conversation/set_conversation), crates/delilah-daemon/src/main.rs (guess)
- Depends on: —
- Parallel group: 3A
- Acceptance: no behaviour change — idea-only logic lives in idea.rs, `is_idea`/`set_idea` renamed everywhere, every existing test passes
- Checks: cargo test -p delilah-daemon
- Reviewers: architecture

## WP-04 Transcript fixtures and parser
- Phase: 3 Foundational
- Tasks: T006, T007, T008
- Files: crates/delilah-agents/tests/fixtures/transcripts/{basic,unknown,partial,noise,sidechain,large,ask,pending}.jsonl, crates/delilah-agents/src/transcript.rs, crates/delilah-agents/src/lib.rs, crates/delilah-agents/src/claude.rs
- Depends on: WP-01
- Parallel group: 3A
- Acceptance: tolerant `parse`/`parse_message`/`tool_detail`/`transcript_path` pass all fixture tests; `claude::has_conversation` uses the shared path rule
- Checks: cargo test -p delilah-agents
- Reviewers: architecture, security

## WP-05 Conversation read API from the transcript
- Phase: 4 US1
- Tasks: T009, T010, T011, T012, T013
- Files: crates/delilah-daemon/src/hooks.rs, crates/delilah-daemon/src/sessions.rs, crates/delilah-daemon/src/conversation.rs, crates/delilah-daemon/src/api.rs
- Depends on: WP-03, WP-04
- Parallel group: 4A
- Acceptance: US1 — `GET /sessions/{id}/conversation` merges transcript and stored entries with cursor/complete/transcript/runtime; tool-detail route; shell session gets 409
- Checks: cargo test -p delilah-daemon
- Reviewers: contract, security

## WP-06 Transcript watcher
- Phase: 4 US1
- Tasks: T014, T015
- Files: crates/delilah-daemon/src/transcript_watch.rs, crates/delilah-daemon/src/main.rs (guess: where background loops start), crates/delilah-daemon/src/hooks.rs
- Depends on: WP-05
- Parallel group: 4B
- Acceptance: US1 live — a complete appended line emits `Topic::Conversations` within one interval; partial lines and no live session emit nothing; hooks `poke`
- Checks: cargo test -p delilah-daemon
- Reviewers: architecture

## WP-07 Per-task view setting (migration + API)
- Phase: 4 US1
- Tasks: T016, T017
- Files: crates/delilah-daemon/migrations/0016_task_view.sql, crates/delilah-core/src/task.rs, crates/delilah-daemon/src/store.rs, crates/delilah-daemon/src/api.rs
- Depends on: WP-05
- Parallel group: 4B
- Acceptance: US1 starting view — `Task.view` stored, accepted on create and PATCH, invalid value gives 400, `tasks` event emitted
- Checks: cargo test -p delilah-core, cargo test -p delilah-daemon
- Reviewers: migration, contract

## WP-08 UI types, DiffRows and ToolCard
- Phase: 4 US1
- Tasks: T018, T019, T020, T021
- Files: app/src/components/ChangesView.tsx, app/src/components/DiffRows.tsx, app/src/api.ts, app/src/components/ToolCard.tsx, app/src/components/ToolCard.test.tsx, app/src/i18n (guess: es/en string files), app/src/styles (guess: conv-* styles file)
- Depends on: WP-05, WP-07
- Parallel group: 4C
- Acceptance: US1 tool cards — api.ts matches the daemon contract; every tool type renders collapsed/expanded, truncated results load via toolDetail; ChangesView tests unchanged
- Checks: app (npx tsc --noEmit && npx vitest related --run app/src/components/ToolCard.test.tsx app/src/components/ChangesView.tsx)
- Reviewers: contract

## WP-09 ConversationView for every session
- Phase: 4 US1
- Tasks: T022, T023
- Files: app/src/components/ConversationView.tsx, app/src/components/ConversationView.test.tsx, app/src/components/TaskScreen.tsx
- Depends on: WP-08
- Parallel group: 4D
- Acceptance: US1, FR-012 — non-idea sessions render user/claude/tool/marker entries, incremental cursor fetch, notices, open-file wiring; idea tests still pass
- Checks: app
- Reviewers: —

## WP-10 Chat/Terminal switch in TaskScreen
- Phase: 4 US1
- Tasks: T024, T025
- Files: app/src/components/TaskScreen.tsx, app/src/components/TaskScreen.test.tsx
- Depends on: WP-09
- Parallel group: 4E
- Acceptance: US1 — start view follows `task.view`, per-session Chat/Terminal switch, shell sessions always terminal, view control calls `api.updateTask`
- Checks: app
- Reviewers: —

## WP-11 "Vista" select in NewTaskDialog
- Phase: 4 US1
- Tasks: T026
- Files: app/src/components/NewTaskDialog.tsx, app/src/components/NewTaskDialog.test.tsx
- Depends on: WP-08
- Parallel group: 4D
- Acceptance: US1 — "Vista" select (Chat default) sent as `view` on create
- Checks: app
- Reviewers: —

## WP-12 Composer, queue and ask cards (backend)
- Phase: 5 US2
- Tasks: T027, T028, T029, T030
- Files: crates/delilah-daemon/src/conversation.rs, crates/delilah-daemon/src/hooks.rs, crates/delilah-daemon/src/asks.rs
- Depends on: WP-05, WP-06
- Parallel group: 5A
- Acceptance: US2, FR-008 — messages/queue/stop work on any live Claude session; `needs_terminal` set for task sessions; ask cards carry `tool_use_id` and merge without duplicates; idea merge rules (R5)
- Checks: cargo test -p delilah-daemon
- Reviewers: contract, security

## WP-13 Composer and inline cards (UI)
- Phase: 5 US2
- Tasks: T031, T032
- Files: app/src/components/ConversationView.tsx, app/src/components/ConversationView.test.tsx, app/src/components/InboxView.tsx
- Depends on: WP-12, WP-10
- Parallel group: 5B
- Acceptance: US2 — composer, "en cola" with remove, Detener and inline AskCard for any live session; Bandeja and card stay in sync
- Checks: app
- Reviewers: —

## WP-14 Runtime enum (Pty arm)
- Phase: 6 Foundational (conditional on spike "go")
- Tasks: T033, T034
- Files: crates/delilah-agents/src/runtime.rs, crates/delilah-agents/src/lib.rs, crates/delilah-daemon/src/actions.rs
- Depends on: WP-02, WP-12
- Parallel group: 6A
- Acceptance: `Runtime::Pty` sends prompt/stop/kill/wait with the same bytes as before; `prompt_bytes` moved with tests
- Checks: cargo test -p delilah-agents, cargo test -p delilah-daemon
- Reviewers: architecture

## WP-15 Replace Live.pty with Live.runtime
- Phase: 6 Foundational (conditional on spike "go")
- Tasks: T035, T036
- Files: crates/delilah-daemon/src/sessions.rs, actions.rs, archive.rs, projects.rs, proposals.rs, session_api.rs, scheduler.rs, handoff.rs, inbox.rs, conversation.rs, terminal.rs, run.rs (all under crates/delilah-daemon/src/)
- Depends on: WP-14
- Parallel group: 6B
- Acceptance: no behaviour change — workspace tests and clippy green, only construction helpers changed in tests
- Checks: cargo test --workspace, cargo clippy --workspace --all-targets
- Reviewers: architecture

## WP-16 Stream runtime (agents crate)
- Phase: 7 US3 (conditional)
- Tasks: T037, T038, T039, T040
- Files: crates/delilah-agents/tests/fixtures/fake-claude-stream.sh, crates/delilah-agents/src/stream.rs, crates/delilah-agents/src/runtime.rs, crates/delilah-agents/src/claude.rs, crates/delilah-agents/src/lib.rs
- Depends on: WP-15
- Parallel group: 7A
- Acceptance: US3 — `StreamSession` yields typed events, user lines, interrupt, permission replies; `Runtime::Stream` and `claude::stream_command` with spike adjustments
- Checks: cargo test -p delilah-agents
- Reviewers: architecture, security

## WP-17 chat_mode migration
- Phase: 7 US3 (conditional)
- Tasks: T041
- Files: crates/delilah-daemon/migrations/0017_chat_mode.sql, crates/delilah-core/src/task.rs, crates/delilah-daemon/src/store.rs
- Depends on: WP-07
- Parallel group: 7A
- Acceptance: US3 — `chat_mode` and `allowed_tools` stored and read, `set_chat_mode`, `add_allowed_tool` dedupes
- Checks: cargo test -p delilah-core, cargo test -p delilah-daemon
- Reviewers: migration

## WP-18 Launch tasks in chat mode
- Phase: 7 US3 (conditional)
- Tasks: T042, T043, T044
- Files: crates/delilah-daemon/src/run.rs, crates/delilah-daemon/src/api.rs, crates/delilah-daemon/src/session_api.rs
- Depends on: WP-16, WP-17
- Parallel group: 7B
- Acceptance: US3 — chat_mode tasks launch a Stream runtime (waits for Init, clear failure message); projects/ideas stay PTY; resume relaunches in stream; task/session JSON expose `chat_mode`, `allowed_tools`, `runtime`
- Checks: cargo test -p delilah-daemon
- Reviewers: contract, architecture

## WP-19 Terminal refusal, live deltas socket, stop marker
- Phase: 7 US3 (conditional)
- Tasks: T045, T046, T047
- Files: crates/delilah-daemon/src/terminal.rs, crates/delilah-daemon/src/live.rs, crates/delilah-daemon/src/conversation.rs, crates/delilah-daemon/src/api.rs
- Depends on: WP-18
- Parallel group: 7C
- Acceptance: US3 — terminal routes 409 on stream sessions; `/live` relays deltas (4409 for PTY); interrupt keeps the session and records `marker: stopped`
- Checks: cargo test -p delilah-daemon
- Reviewers: contract, security, architecture

## WP-20 CLI `--chat` flag
- Phase: 7 US3 (conditional)
- Tasks: T048
- Files: crates/delilah-cli/src/main.rs
- Depends on: WP-18
- Parallel group: 7C
- Acceptance: US3 — `delilah new|run --chat` sends `chat_mode: true`; `attach` prints the 409 message and exits 1
- Checks: cargo test -p delilah-cli
- Reviewers: —

## WP-21 Chat-mode UI (live bubble, terminal lock, dialog switch)
- Phase: 7 US3 (conditional)
- Tasks: T049, T050, T051
- Files: app/src/api.ts, app/src/components/ConversationView.tsx, app/src/components/ConversationView.test.tsx, app/src/components/TaskScreen.tsx, app/src/components/TaskScreen.test.tsx, app/src/components/NewTaskDialog.tsx, app/src/components/NewTaskDialog.test.tsx, start dialog component (guess: where `autopilot` is offered)
- Depends on: WP-19
- Parallel group: 7D
- Acceptance: US3 — streaming "writing" bubble replaced by the transcript entry; Terminal switch disabled for stream sessions with the experimental mark; "Modo chat (experimental)" switch and "Iniciar en terminal" retry
- Checks: app
- Reviewers: contract

## WP-22 Permission flow (backend)
- Phase: 8 US3 permissions (conditional)
- Tasks: T052, T053, T054, T055
- Files: crates/delilah-daemon/src/permissions.rs, crates/delilah-daemon/src/run.rs, crates/delilah-daemon/src/api.rs, crates/delilah-agents/src/claude.rs
- Depends on: WP-19
- Parallel group: 8A
- Acceptance: FR-020 — permission asks created, answered (allow once / always / deny), rule-matched, released on stop; `allowed_tools` written into session settings; `POST /sessions/{id}/permissions/{ask}`
- Checks: cargo test -p delilah-daemon, cargo test -p delilah-agents
- Reviewers: security, contract, architecture

## WP-23 Permission asks in Bandeja, conversation and autopilot
- Phase: 8 US3 permissions (conditional)
- Tasks: T056, T057
- Files: crates/delilah-daemon/src/inbox.rs, crates/delilah-daemon/src/conversation.rs, crates/delilah-daemon/src/autopilot.rs
- Depends on: WP-22
- Parallel group: 8B
- Acceptance: FR-020 — inbox kind `permission`, conversation card with `ask_kind: "permission"`, autopilot pauses with "necesita permiso: <tool>"
- Checks: cargo test -p delilah-daemon
- Reviewers: contract

## WP-24 PermissionCard UI
- Phase: 8 US3 permissions (conditional)
- Tasks: T058
- Files: app/src/api.ts, app/src/components/PermissionCard.tsx, app/src/components/PermissionCard.test.tsx, app/src/components/ConversationView.tsx, app/src/components/ConversationView.test.tsx, app/src/components/InboxView.tsx, app/src/components/InboxView.test.tsx (guess)
- Depends on: WP-23
- Parallel group: 8C
- Acceptance: FR-020 — card with Permitir una vez / siempre en esta tarea / Denegar (reason) in conversation and Bandeja
- Checks: app
- Reviewers: contract

## WP-25 Permission fallback (only if spike d2 = no)
- Phase: 8 US3 permissions (conditional)
- Tasks: T059
- Files: crates/delilah-cli/src/main.rs (guess), crates/delilah-daemon/src/permissions.rs, crates/delilah-agents/src/claude.rs, specs/006-chat-view/tasks.md
- Depends on: WP-22
- Parallel group: 8D
- Acceptance: FR-020 — the spike's fallback (MCP permission tool or allow-list with "Pasar a terminal") replaces the control-protocol path
- Checks: cargo test -p delilah-daemon, cargo test -p delilah-cli
- Reviewers: security, architecture

## WP-26 Runtime switch API
- Phase: 9 US4 (conditional)
- Tasks: T060, T061
- Files: crates/delilah-daemon/src/runtime_api.rs, crates/delilah-daemon/src/api.rs, crates/delilah-daemon/src/run.rs
- Depends on: WP-24
- Parallel group: 9A
- Acceptance: US4 — `POST /tasks/{key}/runtime` switches idle tasks with `--resume`, 409 while working or with pending asks, same mode is a no-op
- Checks: cargo test -p delilah-daemon
- Reviewers: contract, architecture

## WP-27 CLI `delilah runtime`
- Phase: 9 US4 (conditional)
- Tasks: T062
- Files: crates/delilah-cli/src/main.rs
- Depends on: WP-26
- Parallel group: 9B
- Acceptance: US4 — `delilah runtime <task> chat|terminal` prints the 409 message on refusal
- Checks: cargo test -p delilah-cli
- Reviewers: —

## WP-28 Switch buttons in TaskScreen
- Phase: 9 US4 (conditional)
- Tasks: T063
- Files: app/src/api.ts, app/src/components/TaskScreen.tsx, app/src/components/TaskScreen.test.tsx
- Depends on: WP-26
- Parallel group: 9B
- Acceptance: US4 — "Pasar a terminal" / "Pasar a modo chat" call `api.setRuntime`, 409 message shown, Terminal switch enabled afterwards
- Checks: app
- Reviewers: contract

## WP-29 Stream crash and restart resume
- Phase: 9 US4 (conditional)
- Tasks: T064
- Files: crates/delilah-daemon/src/run.rs
- Depends on: WP-26
- Parallel group: 9B
- Acceptance: US4 — a stream process dying mid-turn marks the session exited and the task resumes in either mode; daemon restart relaunches chat-mode tasks in stream
- Checks: cargo test -p delilah-daemon
- Reviewers: —

## WP-30 Docs
- Phase: 10 Polish
- Tasks: T065, T066, T067, T068
- Files: docs/decisions.md, docs/status.md, docs/architecture.md, README.md
- Depends on: WP-13 (WP-29 when phases 6–9 ran)
- Parallel group: 10A
- Acceptance: decisions, status, architecture and README describe the chat view and (if built) chat mode; chat-mode entries skipped if the spike dropped it
- Checks: —
- Reviewers: —

## WP-31 Final verification
- Phase: 10 Polish
- Tasks: T069, T070
- Files: — (fixes land wherever the failures are)
- Depends on: WP-30
- Parallel group: 10B
- Acceptance: SC-003 — 3,000-entry transcript opens and scrolls without lag in a preview; quickstart passes end to end; full workspace, Tauri shell and vitest checks green
- Checks: cargo test --workspace, cargo clippy --workspace --all-targets, cargo test --manifest-path app/src-tauri/Cargo.toml, cd app && npx tsc --noEmit && npx vitest run
- Reviewers: —
