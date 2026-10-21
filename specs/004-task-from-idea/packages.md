# Work packages — 004 Task from idea

Source: specs/004-task-from-idea/tasks.md · 41 pending tasks · 20 packages

Checks: Rust = `cargo test -p <crate>`; app = `cd app && npx tsc --noEmit && npx vitest related --run <files>`.

## WP-01 Spike on Claude Code behaviour (manual, no code)
- Phase: 1 Setup
- Tasks: T001
- Files: specs/004-task-from-idea/research.md
- Depends on: —
- Parallel group: 1A
- Acceptance: FR-005, FR-008 — spike results (a)-(f) written in research.md; if (a), (c) or (d) fails, stop and report
- Checks: none (manual, in scripts/preview.sh only, never 7420)
- Reviewers: —

## WP-02 Plugin source
- Phase: 1 Setup
- Tasks: T002
- Files: crates/delilah-agents/plugin/.claude-plugin/plugin.json, crates/delilah-agents/plugin/skills/task-from-idea/SKILL.md
- Depends on: —
- Parallel group: 1A
- Acceptance: FR-006, FR-007 — plugin.json and SKILL.md exist with the six-point protocol and Spanish brief headings
- Checks: none (text only; exercised by WP-03)
- Reviewers: —

## WP-03 Agents crate: launch flags, settings, write_plugin
- Phase: 2 Foundational
- Tasks: T003, T004, T005
- Files: crates/delilah-agents/src/claude.rs
- Depends on: WP-01, WP-02
- Parallel group: 2A
- Acceptance: FR-005, FR-006 — Permission enum, plugin_dir and append_system_prompt flags, dontAsk allow/deny settings, idempotent write_plugin
- Checks: cargo test -p delilah-agents
- Reviewers: security

## WP-04 Core: CreatedFromConversation event
- Phase: 2 Foundational
- Tasks: T006
- Files: crates/delilah-core/src/task.rs
- Depends on: —
- Parallel group: 2A
- Acceptance: FR-015 — event applies only to Queued tasks and leaves status/phase unchanged
- Checks: cargo test -p delilah-core
- Reviewers: contract

## WP-05 Daemon store: migration, conversation entries, task-from-conversation
- Phase: 2 Foundational
- Tasks: T007, T008, T009, T010
- Files: crates/delilah-daemon/migrations/0014_idea_conversations.sql, crates/delilah-daemon/src/store_sessions.rs, crates/delilah-daemon/src/store.rs, crates/delilah-daemon/src/run.rs (SessionRecord literals), crates/delilah-daemon/src/hooks.rs (literals), crates/delilah-daemon/src/session_api.rs (literals)
- Depends on: WP-04
- Parallel group: 2B
- Acceptance: FR-015, FR-016 — conversation column, conversation_entries CRUD with queue rules, transactional create_task_from_conversation
- Checks: cargo test -p delilah-daemon
- Reviewers: migration

## WP-06 Daemon plumbing: Conversations topic, plugin dir at start-up
- Phase: 2 Foundational
- Tasks: T011, T012
- Files: crates/delilah-daemon/src/events.rs, crates/delilah-daemon/src/state.rs, crates/delilah-daemon/src/config.rs, crates/delilah-daemon/src/main.rs (guess)
- Depends on: WP-03
- Parallel group: 2B
- Acceptance: FR-006 — plugin written to <home>/plugins/delilah at start; /conversations topic exists
- Checks: cargo test -p delilah-daemon
- Reviewers: —

## WP-07 App API types and client
- Phase: 2 Foundational
- Tasks: T015
- Files: app/src/api.ts, plus components fixed for the new required Session.conversation field (guess: app/src/components/*.test.tsx fixtures)
- Depends on: —
- Parallel group: 2A
- Acceptance: FR-001, FR-003 — api.ts types and client functions match contracts/api.md; tsc passes
- Checks: app (tsc --noEmit)
- Reviewers: contract

## WP-08 Session API: open a conversation session and launch it
- Phase: 2 Foundational
- Tasks: T013, T014
- Files: crates/delilah-daemon/src/session_api.rs, crates/delilah-daemon/src/run.rs
- Depends on: WP-03, WP-05, WP-06
- Parallel group: 2C
- Acceptance: FR-001, FR-005, FR-014 — conversation sessions only for claude project sessions, launched with dontAsk, plugin and idea prompt
- Checks: cargo test -p delilah-daemon
- Reviewers: contract, security

## WP-09 Proposal to task (daemon)
- Phase: 3 US1
- Tasks: T016, T017, T018, T019
- Files: crates/delilah-daemon/src/asks.rs, crates/delilah-daemon/src/api.rs, crates/delilah-daemon/src/inbox.rs
- Depends on: WP-05, WP-08
- Parallel group: 3A
- Acceptance: FR-008, FR-009, FR-010, FR-011, FR-014 — "Crear" on a project proposal creates a LIBRE-n backlog task with the brief; nothing else does
- Checks: cargo test -p delilah-daemon
- Reviewers: contract, security

## WP-10 Conversation history, queue and stop (daemon)
- Phase: 3 US1
- Tasks: T020, T021, T022, T023, T024
- Files: crates/delilah-daemon/src/conversation.rs, crates/delilah-daemon/src/lib.rs, crates/delilah-daemon/src/sessions.rs, crates/delilah-daemon/src/asks.rs, crates/delilah-daemon/src/api.rs, crates/delilah-daemon/src/hooks.rs
- Depends on: WP-09
- Parallel group: 3B
- Acceptance: FR-002, FR-002a, FR-003, FR-004 — entries recorded, messages queued and flushed, stop, activity, needs_terminal, card transitions
- Checks: cargo test -p delilah-daemon
- Reviewers: contract, architecture, security

## WP-11 UI: AskCard and ConversationView
- Phase: 3 US1
- Tasks: T025, T026, T027, T028
- Files: app/src/components/AskCard.tsx, app/src/components/AskCard.test.tsx, app/src/components/ConversationView.tsx, app/src/components/ConversationView.test.tsx, app/src/styles.css
- Depends on: WP-07
- Parallel group: 3A
- Acceptance: FR-002, FR-003, FR-007, FR-011 — proposal card with Markdown preview and Crear/Cambiar algo/Descartar; chat view with queue, stop, activity and terminal hand-off
- Checks: app (tsc, vitest related AskCard, ConversationView)
- Reviewers: architecture

## WP-12 UI: wire the view and entry points
- Phase: 3 US1
- Tasks: T029, T030, T031
- Files: app/src/components/TaskScreen.tsx, app/src/components/TaskScreen.test.tsx, app/src/components/ProjectView.tsx, app/src/components/Sidebar.tsx, app/src/components/Sidebar.test.tsx, app/src/components/Icon.tsx, app/src/components/NewTaskDialog.tsx, app/src/components/NewTaskDialog.test.tsx (guess)
- Depends on: WP-11, WP-08
- Parallel group: 3C
- Acceptance: FR-001, FR-002, FR-004, FR-016, FR-017 — three entry points open a conversation; toggle terminal/conversation; Jira and Libre paths unchanged
- Checks: app (tsc, vitest related the files above)
- Reviewers: —

## WP-13 Regression: started task reads the edited brief
- Phase: 4 US2
- Tasks: T032
- Files: crates/delilah-daemon/src/run.rs
- Depends on: WP-09
- Parallel group: 4A
- Acceptance: FR-010 — edited notes reach notes.md and the prompt of a started backlog task
- Checks: cargo test -p delilah-daemon
- Reviewers: —

## WP-14 UI: history event label
- Phase: 4 US2
- Tasks: T033
- Files: app/src/components/TaskView.tsx (guess), app/src/components/BacklogTaskView.tsx (guess), app/src/i18n/en.ts, matching test file (guess)
- Depends on: WP-04, WP-07
- Parallel group: 4A
- Acceptance: FR-015 — history shows "Creada desde la conversación «{label}»"
- Checks: app (tsc, vitest related)
- Reviewers: —

## WP-15 Project-session creation tests and launch flags
- Phase: 5 US3
- Tasks: T034, T035
- Files: crates/delilah-daemon/src/asks.rs, crates/delilah-daemon/src/run.rs (or session_api.rs)
- Depends on: WP-10
- Parallel group: 5A
- Acceptance: FR-013, FR-014 — project sessions can create, converted and task sessions cannot; flags per launch table
- Checks: cargo test -p delilah-daemon
- Reviewers: security

## WP-16 UI: proposal card in project sessions and Bandeja
- Phase: 5 US3
- Tasks: T036
- Files: app/src/components/TaskScreen.tsx, app/src/components/TaskScreen.test.tsx, app/src/components/InboxView.tsx
- Depends on: WP-12
- Parallel group: 5A
- Acceptance: FR-013 — confirmation card renders beside the terminal and in the Bandeja, key links to the task
- Checks: app (tsc, vitest related)
- Reviewers: —

## WP-17 Several ideas per conversation
- Phase: 6 US4
- Tasks: T037
- Files: crates/delilah-daemon/src/asks.rs
- Depends on: WP-10
- Parallel group: 6A
- Acceptance: FR-012 — two confirmed proposals give two tasks with own briefs and cards
- Checks: cargo test -p delilah-daemon
- Reviewers: —

## WP-18 UI strings
- Phase: 7 Polish
- Tasks: T038
- Files: app/src/i18n/en.ts
- Depends on: WP-12, WP-14, WP-16
- Parallel group: 7A
- Acceptance: all new UI strings have English entries
- Checks: app (tsc, vitest related)
- Reviewers: —

## WP-19 Docs
- Phase: 7 Polish
- Tasks: T039
- Files: docs/status.md, docs/architecture.md, docs/decisions.md, README.md
- Depends on: WP-01, WP-12
- Parallel group: 7A
- Acceptance: status, architecture, decisions #35/#36 and README describe the feature
- Checks: none
- Reviewers: —

## WP-20 Full checks and manual walkthrough
- Phase: 7 Polish
- Tasks: T040, T041
- Files: specs/004-task-from-idea/progress.md (if kept)
- Depends on: WP-15, WP-16, WP-17, WP-18, WP-19
- Parallel group: 7B
- Acceptance: all FR — cargo test --workspace, app/src-tauri tests, tsc and vitest run pass; quickstart 1-11 done in a preview
- Checks: cargo test --workspace, cargo test --manifest-path app/src-tauri/Cargo.toml, app (tsc, vitest run)
- Reviewers: —
