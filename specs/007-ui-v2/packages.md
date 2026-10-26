# Work packages — UI v2

Source: specs/007-ui-v2/tasks.md · 104 pending tasks · 60 packages

Notes for the orchestrator:
- Check names: `app` = `cd app && npx tsc --noEmit && npx vitest related --run <files>`; `daemon` = `cargo test -p delilah-daemon`; `core` = `cargo test -p delilah-core`; `cli` = `cargo test -p delilah-cli`; `integrations` = `cargo test -p delilah-integrations`; `tauri` = `cargo test --manifest-path app/src-tauri/Cargo.toml`; `e2e-api` = `scripts/e2e.sh api`; `e2e-ui` = `scripts/e2e.sh ui`; `app-e2e-tsc` = `cd app && npx tsc --noEmit -p e2e`.
- "Depends on" lists the real dependencies only. A package may start as soon as they are done, also across phases (Phase 2 runs alongside Phases 3–8). Parallel group ids only mean "safe together, same checkout, disjoint files".
- Every package of Phase 4 onward depends on WP-17 (T027 token rename), and every package that uses a new API field or call depends on WP-02 (T004 then T005).
- No package edits `app/src/inline-style.test.ts` (shared by every UI task, would collide in one checkout); packages report their `style={{` count and WP-55 sets the final allow-list. The ratchet only fails on increases, so not lowering it is safe.
- `app/src/i18n/en/<Component>.ts` files are named per component, so they are never shared.

## WP-01 Setup guards
- Phase: 1 Setup
- Tasks: T001, T002, T003
- Files: app/src/i18n/en.ts, app/src/i18n/en/legacy.ts, app/src/i18n/coverage.test.ts, app/src/inline-style.test.ts, crates/delilah-daemon/tests/fixtures/blocking_kinds.json
- Depends on: —
- Parallel group: 1A
- Acceptance: FR-056–FR-059 (guards), SC-010 — per-component English files load through the glob, the ratchet and coverage tests pass on today's tree, the blocking-kinds fixture equals `BLOCKING`.
- Checks: app
- Reviewers: —

## WP-02 Client read types and calls (api.ts)
- Phase: 2 Foundational
- Tasks: T004, T005
- Files: app/src/api.ts, app/src/api.test.ts
- Depends on: WP-01
- Parallel group: 2A
- Acceptance: contracts/api.md — every new field and call is typed field-for-field; `dismissInbox` id is URI-encoded; `onRequestResult` observer exists. Only package that edits api.ts.
- Checks: app
- Reviewers: contract, security

## WP-03 Core events and created_at
- Phase: 2 Foundational
- Tasks: T006
- Files: crates/delilah-core/src/task.rs
- Depends on: WP-01
- Parallel group: 2A
- Acceptance: FR-023 (reopen), R8, R14 — `Task.created_at`, `Event::Reopen` (Done → WaitingForUser), history-only `RuntimeSwitched`, `PermissionAsked`, `PermissionExpired`.
- Checks: core
- Reviewers: contract

## WP-04 Route skeleton
- Phase: 2 Foundational
- Tasks: T007
- Files: crates/delilah-daemon/src/api.rs, crates/delilah-daemon/src/actions.rs, crates/delilah-daemon/src/inbox.rs, crates/delilah-daemon/src/pr_api/mod.rs, crates/delilah-daemon/src/pr_api/merge.rs (new), crates/delilah-daemon/src/setup.rs, crates/delilah-daemon/src/jira.rs, crates/delilah-daemon/src/runtime_api.rs
- Depends on: WP-01
- Parallel group: 2A
- Acceptance: contracts/api.md — every new route answers non-404 with a 501 stub, `/health` has `home`; the only package that edits api.rs.
- Checks: daemon
- Reviewers: contract, architecture

## WP-05 Pending permission waits; ended permission cards
- Phase: 2 Foundational
- Tasks: T008, T015
- Files: crates/delilah-daemon/src/permissions.rs, crates/delilah-daemon/src/main.rs, crates/delilah-daemon/src/conversation.rs
- Depends on: WP-03
- Parallel group: 2B
- Acceptance: R8, R13 — a pending permission puts the task in «Te espera» with its recovery rules; answered/expired permission cards stay in the conversation with their decision; `permission-expired` is recorded.
- Checks: daemon
- Reviewers: contract

## WP-06 Checkout is the default workspace
- Phase: 2 Foundational
- Tasks: T009
- Files: crates/delilah-daemon/src/run.rs
- Depends on: WP-01
- Parallel group: 2B
- Acceptance: FR-049, R9 — no workspace choice anywhere works in the checkout; saved choices still win.
- Checks: daemon
- Reviewers: contract

## WP-07 created_at exposed and reopen
- Phase: 2 Foundational
- Tasks: T010, T011
- Files: crates/delilah-daemon/src/store.rs, crates/delilah-daemon/src/actions.rs
- Depends on: WP-03, WP-04
- Parallel group: 2B
- Acceptance: FR-050, R10 — tasks carry `created_at`; `POST reopen` turns a done task into waiting-for-user, 409 otherwise.
- Checks: daemon
- Reviewers: contract

## WP-08 Inbox store, ids, removal and new avisos
- Phase: 2 Foundational
- Tasks: T012, T013, T014
- Files: crates/delilah-daemon/migrations/0020_inbox_dismissals.sql, crates/delilah-daemon/src/store_inbox.rs (new), crates/delilah-daemon/src/lib.rs, crates/delilah-daemon/src/inbox.rs, crates/delilah-daemon/src/pr_api/mod.rs, crates/delilah-daemon/src/hooks.rs, crates/delilah-daemon/src/pr_api/open.rs, crates/delilah-daemon/src/prs.rs, crates/delilah-daemon/src/store_prs.rs, crates/delilah-daemon/tests/fixtures/blocking_kinds.json
- Depends on: WP-01, WP-04
- Parallel group: 2B
- Acceptance: FR-037–FR-041, R11, R12 — stable item ids, clear never removes blocking items and restore brings them back, `turn-done` and `pr-opened` avisos exist; dismissals are the single record.
- Checks: daemon
- Reviewers: migration, contract, security, architecture

## WP-09 PR check times, review comments, merge
- Phase: 2 Foundational
- Tasks: T016, T017
- Files: crates/delilah-daemon/src/gh/graphql.rs, crates/delilah-daemon/src/pr_types.rs, crates/delilah-daemon/src/gh/fake.rs, crates/delilah-daemon/src/gh/mod.rs, crates/delilah-daemon/src/pr_api/merge.rs, scripts/fake-gh.sh
- Depends on: WP-04
- Parallel group: 2B
- Acceptance: FR-033, FR-034, R17 — snapshots carry check times, review comments, merge methods; `POST merge` checks the gh account and CI, classifies gh failures, never runs on a draft/failing PR.
- Checks: daemon
- Reviewers: contract, security

## WP-10 Setup failure detail and Jira subtasks
- Phase: 2 Foundational
- Tasks: T018, T019
- Files: crates/delilah-daemon/src/setup.rs, crates/delilah-integrations/src/jira.rs, crates/delilah-daemon/src/jira.rs
- Depends on: WP-04
- Parallel group: 2B
- Acceptance: FR-035, FR-046, R15, R18 — `GET …/setup` and `…/setup/log` answer per contract; Jira subtasks route and `subtask_count`/`subtasks_done` work, sample data has a 20-child parent.
- Checks: daemon, integrations
- Reviewers: contract, security

## WP-11 Reveal in Finder (Tauri + platform.ts)
- Phase: 2 Foundational
- Tasks: T022
- Files: app/src-tauri/src/main.rs, app/src/platform.ts, app/src/platform.test.ts (new)
- Depends on: WP-01
- Parallel group: 2B
- Acceptance: R20 — `reveal_path` refuses paths outside canonical `$HOME` (symlinks, `..`, missing); `revealInFinder` no-ops outside the desktop app.
- Checks: tauri, app
- Reviewers: security

## WP-12 Session activity and per-session runtime
- Phase: 2 Foundational
- Tasks: T020, T021
- Files: crates/delilah-daemon/migrations/0021_session_activity.sql, crates/delilah-daemon/src/store_sessions.rs, crates/delilah-daemon/src/session_api.rs, crates/delilah-daemon/src/hooks.rs, crates/delilah-daemon/src/conversation.rs, crates/delilah-daemon/src/runtime_api.rs, crates/delilah-daemon/src/run.rs
- Depends on: WP-03, WP-04, WP-05, WP-06, WP-08
- Parallel group: 2C
- Acceptance: FR-024, FR-040, R14, R16 — sessions expose `runtime_override`, `effective_runtime`, `last_activity_at`, `last_message`; switching one session's runtime leaves the others and survives a restart.
- Checks: daemon
- Reviewers: migration, contract

## WP-13 CLI
- Phase: 2 Foundational
- Tasks: T023
- Files: crates/delilah-cli/src/main.rs
- Depends on: WP-07, WP-12
- Parallel group: 2D
- Acceptance: `delilah reopen`, `delilah runtime`, `show` prints created, help says checkout is the default.
- Checks: cli
- Reviewers: contract

## WP-14 API e2e flows
- Phase: 2 Foundational
- Tasks: T024
- Files: crates/delilah-e2e/tests/ui_v2.rs (new)
- Depends on: WP-05, WP-06, WP-07, WP-08, WP-09, WP-10, WP-12
- Parallel group: 2D
- Acceptance: SC-005, SC-006 data side — permission ask waits then runs, clear/restore, reopen, setup detail, checkout default; no 501 stub left.
- Checks: daemon, e2e-api
- Reviewers: —

## WP-15 Token guards and v2 tokens
- Phase: 3 US1 Tokens and base components
- Tasks: T025, T026
- Files: app/src/styles.test.ts (new), app/src/tokens.test.ts (new), app/src/styles.css
- Depends on: WP-01
- Parallel group: 3A
- Acceptance: FR-001–FR-004, SC-001, SC-003, SC-004 — v2 dark/light tokens, no old token or raw hex, contrast ≥ 4.5:1 pairs; guards fully green once WP-17 and WP-21 land (report any pair under 4.5:1 to the owner).
- Checks: app
- Reviewers: —

## WP-16 Vocabulary, relative time, icons
- Phase: 3 US1 Tokens and base components
- Tasks: T028, T029, T030
- Files: app/src/vocab.ts, app/src/vocab.test.ts, app/src/i18n/en/vocab.ts, app/src/time.ts, app/src/time.test.ts, app/src/i18n/en/time.ts, app/src/components/Icon.tsx
- Depends on: WP-01, WP-02
- Parallel group: 3A
- Acceptance: FR-005–FR-008 — one status/kind/PR vocabulary matching the blocking-kinds fixture, relative time es/en, v2 icon set with no emoji.
- Checks: app
- Reviewers: —

## WP-17 Rename old tokens in TSX
- Phase: 3 US1 Tokens and base components
- Tasks: T027
- Files: app/src/App.tsx, app/src/components/*.tsx (those with old token names; not Icon.tsx or TerminalPane.tsx)
- Depends on: WP-15
- Parallel group: 3B
- Acceptance: FR-001 — no old token name left in App/components TSX; no other change. Must finish before any Phase 4+ package starts.
- Checks: app
- Reviewers: —

## WP-18 Kit: controls
- Phase: 3 US1 Tokens and base components
- Tasks: T031, T036, T037, T038
- Files: app/src/components/ui/Button.tsx, app/src/components/ui/Button.css, app/src/components/ui/Button.test.tsx, app/src/components/ui/IconButton.tsx, app/src/components/ui/IconButton.test.tsx, app/src/components/ui/Toggle.tsx, app/src/components/ui/Toggle.css, app/src/components/ui/Toggle.test.tsx, app/src/components/ui/Segmented.tsx, app/src/components/ui/Segmented.css, app/src/components/ui/Segmented.test.tsx, app/src/i18n/en/Segmented.ts, app/src/components/ui/OptionCard.tsx, app/src/components/ui/OptionCard.css, app/src/components/ui/OptionCard.test.tsx
- Depends on: WP-15
- Parallel group: 3B
- Acceptance: FR-009–FR-012 — Button/IconButton, Toggle, Segmented (radio and pressed modes), OptionCard with tests, tokens only.
- Checks: app
- Reviewers: architecture

## WP-19 Kit: lists, overlays, feedback
- Phase: 3 US1 Tokens and base components
- Tasks: T039, T040, T041, T042, T043, T044
- Files: app/src/components/ui/Row.tsx, app/src/components/ui/Row.css, app/src/components/ui/Row.test.tsx, app/src/components/ui/Count.tsx, app/src/components/ui/Count.css, app/src/components/ui/Count.test.tsx, app/src/components/ui/Skeleton.tsx, app/src/components/ui/Skeleton.css, app/src/components/ui/Skeleton.test.tsx, app/src/components/ui/Dialog.tsx, app/src/components/ui/Dialog.css, app/src/components/ui/Dialog.test.tsx, app/src/components/ui/ConfirmDialog.tsx, app/src/components/ui/ConfirmDialog.test.tsx, app/src/i18n/en/Dialog.ts, app/src/components/ui/Menu.tsx, app/src/components/ui/Menu.css, app/src/components/ui/Menu.test.tsx, app/src/components/ui/Toast.tsx, app/src/components/ui/Toast.css, app/src/components/ui/Toast.test.tsx
- Depends on: WP-15
- Parallel group: 3B
- Acceptance: FR-013–FR-015 — Row, Count, Skeleton, Dialog/ConfirmDialog (focus trap, Esc, focus return), Menu (keyboard), Toast host with tests.
- Checks: app
- Reviewers: —

## WP-20 Kit: status components
- Phase: 3 US1 Tokens and base components
- Tasks: T032, T033, T034, T035
- Files: app/src/components/ui/StatusBadge.tsx, app/src/components/ui/StatusBadge.css, app/src/components/ui/StatusBadge.test.tsx, app/src/components/ui/InboxKind.tsx, app/src/components/ui/InboxKind.css, app/src/components/ui/InboxKind.test.tsx, app/src/components/ui/PrChip.tsx, app/src/components/ui/PrChip.css, app/src/components/ui/PrChip.test.tsx, app/src/components/ui/PhaseBars.tsx, app/src/components/ui/PhaseBars.css, app/src/components/ui/PhaseBars.test.tsx
- Depends on: WP-15, WP-16
- Parallel group: 3B
- Acceptance: FR-005–FR-008 — StatusBadge, InboxKind, PrChip, PhaseBars render the §3 vocabulary with text always present.
- Checks: app
- Reviewers: —

## WP-21 Live terminal and editor themes
- Phase: 3 US1 Tokens and base components
- Tasks: T046, T047
- Files: app/src/theme.ts, app/src/theme.test.ts, app/src/components/TerminalPane.tsx, app/src/editor/codemirror.ts, app/src/editor/codemirror.test.ts
- Depends on: WP-15
- Parallel group: 3B
- Acceptance: FR-003, R5 — terminal and editor re-theme with the OS scheme without reload; no old tokens in those files.
- Checks: app
- Reviewers: —

## WP-22 Kit barrel
- Phase: 3 US1 Tokens and base components
- Tasks: T045
- Files: app/src/components/ui/index.ts (new)
- Depends on: WP-18, WP-19, WP-20
- Parallel group: 3C
- Acceptance: all kit components are re-exported; tsc green.
- Checks: app
- Reviewers: architecture

## WP-23 Connection, breakpoints, sidebar shortcut
- Phase: 4 US2 Shell, sidebar, statusbar, disconnected
- Tasks: T048, T049, T050
- Files: app/src/connection.ts, app/src/connection.test.ts, app/src/useWidth.ts, app/src/useWidth.test.ts, app/src/shortcuts.ts, app/src/shortcuts.test.ts, app/src/i18n/en/shortcuts.ts
- Depends on: WP-02, WP-17
- Parallel group: 4A
- Acceptance: FR-016–FR-018, SC-009 — connection hook recovers within 5 s, window and container breakpoints, `toggle-sidebar` ⌘\ rebindable.
- Checks: app
- Reviewers: —

## WP-24 Logs view, statusbar, disconnected banner
- Phase: 4 US2 Shell, sidebar, statusbar, disconnected
- Tasks: T051, T052, T053
- Files: app/src/logLevel.ts, app/src/logLevel.test.ts, app/src/components/LogsView.tsx, app/src/components/LogsView.css, app/src/components/LogsView.test.tsx, app/src/i18n/en/LogsView.ts, app/src/components/StatusBar.tsx, app/src/components/StatusBar.css, app/src/components/StatusBar.test.tsx, app/src/i18n/en/StatusBar.ts, app/src/components/DisconnectedBanner.tsx, app/src/components/DisconnectedBanner.css, app/src/components/DisconnectedBanner.test.tsx, app/src/i18n/en/DisconnectedBanner.ts
- Depends on: WP-16, WP-17, WP-18, WP-23
- Parallel group: 4B
- Acceptance: FR-017, FR-018, FR-021 — Logs with level filter, statusbar with connection/counts/usage/Logs button, disconnected banner with retry.
- Checks: app
- Reviewers: —

## WP-25 Sidebar rewrite
- Phase: 4 US2 Shell, sidebar, statusbar, disconnected
- Tasks: T054
- Files: app/src/components/Sidebar.tsx, app/src/components/Sidebar.css, app/src/components/Sidebar.test.tsx, app/src/i18n/en/Sidebar.ts
- Depends on: WP-16, WP-17, WP-19, WP-23
- Parallel group: 4B
- Acceptance: FR-019, FR-020 — nav, project groups, task rows with ellipsis + tooltip, backlog and done folds, context menu with delete, 56 px rail mode.
- Checks: app
- Reviewers: —

## WP-26 App shell
- Phase: 4 US2 Shell, sidebar, statusbar, disconnected
- Tasks: T055
- Files: app/src/App.tsx, app/src/App.css (new), app/src/App.test.tsx (new), app/src/i18n/en/App.ts
- Depends on: WP-19, WP-23, WP-24, WP-25
- Parallel group: 4C
- Acceptance: FR-016–FR-018, SC-009 — titlebar breadcrumb with drag regions, rail, statusbar, disconnected state greys content, ToastHost mounted.
- Checks: app
- Reviewers: architecture

## WP-27 E2E selectors for the shell
- Phase: 4 US2 Shell, sidebar, statusbar, disconnected
- Tasks: T056
- Files: app/e2e/screens.spec.ts, app/e2e/projects.spec.ts, app/e2e/smoke.spec.ts
- Depends on: WP-26
- Parallel group: 4D
- Acceptance: UI e2e passes with the new sidebar, board «New task» and statusbar Logs.
- Checks: app-e2e-tsc, e2e-ui
- Reviewers: —

## WP-28 History sentences and AskCard
- Phase: 5 US3 Task screen
- Tasks: T057, T058
- Files: app/src/history.ts, app/src/history.test.ts, app/src/i18n/en/history.ts, app/src/components/AskCard.tsx, app/src/components/AskCard.css, app/src/components/AskCard.test.tsx, app/src/components/PermissionCard.tsx, app/src/components/PermissionCard.test.tsx, app/src/i18n/en/AskCard.ts
- Depends on: WP-02, WP-16, WP-17, WP-18, WP-20
- Parallel group: 5A
- Acceptance: FR-027, FR-030, R22 — every history event has a sentence; one AskCard for question/permission/aviso in inbox, inline and chat variants.
- Checks: app
- Reviewers: —

## WP-29 Header controls and tabs
- Phase: 5 US3 Task screen
- Tasks: T059, T060, T061
- Files: app/src/components/AutopilotControls.tsx, app/src/components/AutopilotControls.css, app/src/components/AutopilotControls.test.tsx, app/src/i18n/en/AutopilotControls.ts, app/src/components/SessionViewSwitch.tsx, app/src/components/SessionViewSwitch.css, app/src/components/SessionViewSwitch.test.tsx, app/src/i18n/en/SessionViewSwitch.ts, app/src/components/TaskTabs.tsx, app/src/components/TaskTabs.css, app/src/components/TaskTabs.test.tsx, app/src/i18n/en/TaskTabs.ts
- Depends on: WP-02, WP-17, WP-18, WP-19
- Parallel group: 5A
- Acceptance: FR-023–FR-026 — autopilot/Ahorro with confirmation, Chat|Terminal per session with confirm, tab bar with session menu.
- Checks: app
- Reviewers: —

## WP-30 Cambios, PR and setup panels
- Phase: 5 US3 Task screen
- Tasks: T066, T067, T068
- Files: app/src/components/ChangesView.tsx, app/src/components/ChangesView.css, app/src/components/ChangesView.test.tsx, app/src/components/DiffRows.tsx, app/src/components/DiffRows.test.tsx, app/src/components/FileTree.tsx, app/src/i18n/en/ChangesView.ts, app/src/components/PrView.tsx, app/src/components/PrView.css, app/src/components/PrView.test.tsx, app/src/components/OpenPrDialog.tsx, app/src/components/OpenPrDialog.test.tsx, app/src/i18n/en/PrView.ts, app/src/components/SetupBanner.tsx, app/src/components/SetupBanner.css, app/src/components/SetupBanner.test.tsx, app/src/i18n/en/SetupBanner.ts
- Depends on: WP-02, WP-16, WP-17, WP-18, WP-19, WP-20
- Parallel group: 5A
- Acceptance: FR-032–FR-035 — grouped changes with skeleton, PR tab with checks/comments/merge confirm, setup-failed panel with exit code and log.
- Checks: app
- Reviewers: —

## WP-31 Task header
- Phase: 5 US3 Task screen
- Tasks: T062
- Files: app/src/components/TaskHeader.tsx, app/src/components/TaskHeader.css, app/src/components/TaskHeader.test.tsx, app/src/components/PreviewControl.tsx, app/src/components/PreviewControl.test.tsx, app/src/i18n/en/TaskHeader.ts
- Depends on: WP-17, WP-19, WP-20, WP-29
- Parallel group: 5B
- Acceptance: FR-022 — key never wraps, editable title with ellipsis, status, phase bars, session switch, autopilot, preview, done and setup-failed variants.
- Checks: app
- Reviewers: —

## WP-32 Task tab
- Phase: 5 US3 Task screen
- Tasks: T064
- Files: app/src/components/TaskView.tsx, app/src/components/TaskView.css, app/src/components/TaskView.test.tsx, app/src/components/SpecPanel.tsx, app/src/components/HandoffActions.tsx, app/src/components/HandoffActions.test.tsx, app/src/i18n/en/TaskView.ts
- Depends on: WP-02, WP-17, WP-19, WP-28
- Parallel group: 5B
- Acceptance: FR-027, FR-028, FR-036, SC-005 — pending ask answered inline, mark done with undo toast, history sentences, sessions list, spec tasks.
- Checks: app
- Reviewers: —

## WP-33 Chat tab
- Phase: 5 US3 Task screen
- Tasks: T065
- Files: app/src/components/ConversationView.tsx, app/src/components/ConversationView.css, app/src/components/ConversationView.test.tsx, app/src/components/ToolCard.tsx, app/src/components/ToolCard.css, app/src/components/ToolCard.test.tsx, app/src/i18n/en/ConversationView.ts
- Depends on: WP-02, WP-17, WP-19, WP-28
- Parallel group: 5B
- Acceptance: FR-030, FR-031 — collapsed tool calls, live and ended permission cards, Enviar/Detener composer, «Detenida» line.
- Checks: app
- Reviewers: —

## WP-34 Task screen integration
- Phase: 5 US3 Task screen
- Tasks: T063
- Files: app/src/components/TaskScreen.tsx, app/src/components/TaskScreen.css, app/src/components/TaskScreen.test.tsx, app/src/components/SessionChain.tsx, app/src/components/SessionChain.test.tsx, app/src/components/TreeResizer.tsx, app/src/components/TreeResizer.test.tsx, app/src/i18n/en/TaskScreen.ts
- Depends on: WP-17, WP-29, WP-31
- Parallel group: 5C
- Acceptance: FR-022, FR-024–FR-026 — TaskScreen uses TaskHeader and TaskTabs, removed experimental toggles and view-switch, one inline style left (`--tree-w`).
- Checks: app
- Reviewers: —

## WP-35 E2E selectors for the task screen
- Phase: 5 US3 Task screen
- Tasks: T069
- Files: app/e2e/chat.spec.ts, app/e2e/conversation.spec.ts, app/e2e/pr.spec.ts, app/e2e/screens.spec.ts
- Depends on: WP-27, WP-30, WP-32, WP-33, WP-34
- Parallel group: 5D
- Acceptance: UI e2e passes with the new task screen selectors.
- Checks: app-e2e-tsc, e2e-ui
- Reviewers: —

## WP-36 E2E task-screen flow
- Phase: 5 US3 Task screen
- Tasks: T070
- Files: app/e2e/task-screen.spec.ts (new)
- Depends on: WP-07, WP-32, WP-34
- Parallel group: 5D
- Acceptance: SC-005 — question answered from the Task tab, mark done then undo, long title keeps the key on one line.
- Checks: app-e2e-tsc, e2e-ui
- Reviewers: —

## WP-37 Bandeja
- Phase: 6 US4 Bandeja
- Tasks: T071
- Files: app/src/components/InboxView.tsx, app/src/components/InboxView.css, app/src/components/InboxView.test.tsx, app/src/i18n/en/InboxView.ts
- Depends on: WP-02, WP-16, WP-17, WP-19, WP-20, WP-28
- Parallel group: 6A
- Acceptance: FR-037–FR-041 — listbox with detail, AskCard per kind, Quitar, Limpiar avisos (never a blocking id) with Deshacer, keyboard keys, empty state with latest activity.
- Checks: app
- Reviewers: —

## WP-38 First run and board
- Phase: 6 US5 Board, first run
- Tasks: T072, T073
- Files: app/src/components/FirstRun.tsx, app/src/components/FirstRun.css, app/src/components/FirstRun.test.tsx, app/src/i18n/en/FirstRun.ts, app/src/components/BoardView.tsx, app/src/components/BoardView.css, app/src/components/BoardView.test.tsx, app/src/phases.ts, app/src/phases.test.ts, app/src/i18n/en/BoardView.ts
- Depends on: WP-17, WP-18, WP-19, WP-20
- Parallel group: 6A
- Acceptance: FR-042–FR-044 — board columns with cards (text status, mini bars, PR chip, statusbar filter), first-run checklist that persists «Omitir».
- Checks: app
- Reviewers: —

## WP-39 Jira
- Phase: 6 US5 Jira
- Tasks: T074
- Files: app/src/components/JiraView.tsx, app/src/components/JiraView.css, app/src/components/JiraView.test.tsx, app/src/i18n/en/JiraView.ts
- Depends on: WP-02, WP-17, WP-18, WP-19, WP-20
- Parallel group: 6A
- Acceptance: FR-045–FR-047 — filter chips, group by, two-line rows, detail with subtasks and linked task, connect form.
- Checks: app
- Reviewers: —

## WP-40 E2E Bandeja
- Phase: 6 US4 Bandeja
- Tasks: T075
- Files: app/e2e/inbox.spec.ts
- Depends on: WP-37, WP-08
- Parallel group: 6B
- Acceptance: SC-006 — «Clear notices» keeps the blocking question, «Undo» restores the avisos.
- Checks: app-e2e-tsc, e2e-ui
- Reviewers: —

## WP-41 E2E board
- Phase: 6 US5 Board
- Tasks: T076
- Files: app/e2e/board.spec.ts, app/e2e/screens.spec.ts
- Depends on: WP-35, WP-38, WP-39
- Parallel group: 6B
- Acceptance: UI e2e passes with board, inbox and Jira selectors.
- Checks: app-e2e-tsc, e2e-ui
- Reviewers: —

## WP-42 WherePicker
- Phase: 7 US6 Dialogs, projects, settings
- Tasks: T077
- Files: app/src/components/WherePicker.tsx, app/src/components/WherePicker.css, app/src/components/WherePicker.test.tsx, app/src/i18n/en/WherePicker.ts
- Depends on: WP-17, WP-18
- Parallel group: 7A
- Acceptance: FR-049 — checkout is the labelled default, worktree is the alternative.
- Checks: app
- Reviewers: —

## WP-43 ProposalCard, delete task, Quick Open
- Phase: 7 US6 Dialogs, projects, settings
- Tasks: T079, T085, T086
- Files: app/src/components/ProposalCard.tsx, app/src/components/ProposalCard.css, app/src/components/ProposalCard.test.tsx, app/src/i18n/en/ProposalCard.ts, app/src/components/DeleteTaskDialog.tsx, app/src/components/DeleteTaskDialog.test.tsx, app/src/i18n/en/DeleteTaskDialog.ts, app/src/components/QuickOpen.tsx, app/src/components/QuickOpen.css, app/src/components/QuickOpen.test.tsx, app/src/i18n/en/QuickOpen.ts
- Depends on: WP-17, WP-18, WP-19
- Parallel group: 7A
- Acceptance: FR-048, FR-055 — proposal step on the kit, delete on ConfirmDialog, Quick Open on Dialog with bold matches.
- Checks: app
- Reviewers: —

## WP-44 Projects and project view
- Phase: 7 US6 Dialogs, projects, settings
- Tasks: T082, T083
- Files: app/src/components/ProjectsView.tsx, app/src/components/ProjectsView.css, app/src/components/ProjectsView.test.tsx, app/src/components/AddProjectDialog.tsx, app/src/components/AddProjectDialog.test.tsx, app/src/i18n/en/ProjectsView.ts, app/src/components/ProjectView.tsx, app/src/components/ProjectView.css, app/src/components/ProjectView.test.tsx, app/src/i18n/en/ProjectView.ts
- Depends on: WP-02, WP-16, WP-17, WP-19
- Parallel group: 7A
- Acceptance: FR-051, FR-053 — projects table with add dialog, rename/remove (blocked note), project view with conversations, ideas, preview command.
- Checks: app
- Reviewers: —

## WP-45 Settings and auto-update preference
- Phase: 7 US6 Dialogs, projects, settings
- Tasks: T084
- Files: app/src/components/SettingsView.tsx, app/src/components/SettingsView.css, app/src/components/SettingsView.test.tsx, app/src/useUpdates.ts, app/src/useUpdates.test.ts, app/src/i18n/en/SettingsView.ts
- Depends on: WP-02, WP-11, WP-17, WP-18
- Parallel group: 7A
- Acceptance: FR-054 — section index, shortcuts with «Por defecto», auto-update preference, Diagnóstico with Finder reveal (desktop only).
- Checks: app
- Reviewers: —

## WP-46 Nueva tarea dialog
- Phase: 7 US6 Dialogs, projects, settings
- Tasks: T078
- Files: app/src/components/NewTaskDialog.tsx, app/src/components/NewTaskDialog.css, app/src/components/NewTaskDialog.test.tsx, app/src/components/NewTaskDialog.project-settings.test.tsx, app/src/i18n/en/NewTaskDialog.ts
- Depends on: WP-17, WP-18, WP-19, WP-29, WP-42
- Parallel group: 7B
- Acceptance: FR-048, SC-007 — fixed-height dialog, «Iniciar» always visible, Chat|Terminal advanced, dirty-checkout error with «Iniciar en un worktree».
- Checks: app
- Reviewers: —

## WP-47 Backlog task and project settings
- Phase: 7 US6 Dialogs, projects, settings
- Tasks: T080, T081
- Files: app/src/components/BacklogTaskView.tsx, app/src/components/BacklogTaskView.css, app/src/components/BacklogTaskView.test.tsx, app/src/i18n/en/BacklogTaskView.ts, app/src/components/ProjectSettings.tsx, app/src/components/ProjectSettings.css, app/src/components/ProjectSettings.test.tsx, app/src/i18n/en/ProjectSettings.ts
- Depends on: WP-02, WP-16, WP-17, WP-19, WP-42
- Parallel group: 7B
- Acceptance: FR-050, FR-052 — backlog form with «Iniciar»/«Guardar»/«Eliminar…» and «Creada ayer»; project settings dialog with help and checkout default.
- Checks: app
- Reviewers: —

## WP-48 E2E dialogs and projects
- Phase: 7 US6 Dialogs, projects, settings
- Tasks: T087
- Files: app/e2e/projects.spec.ts, app/e2e/quick-open.spec.ts, app/e2e/screens.spec.ts, app/e2e/fixtures.ts, app/e2e/*.spec.ts (create-task selectors only; not new-task.spec.ts)
- Depends on: WP-41, WP-43, WP-44, WP-45, WP-46, WP-47
- Parallel group: 7C
- Acceptance: UI e2e passes with the new dialogs, «Start», add-project dialog, quick open.
- Checks: app-e2e-tsc, e2e-ui
- Reviewers: —

## WP-49 E2E new task
- Phase: 7 US6 Dialogs, projects, settings
- Tasks: T088
- Files: app/e2e/new-task.spec.ts (new)
- Depends on: WP-06, WP-46, WP-48
- Parallel group: 7C
- Acceptance: SC-007, FR-049 — dialog box identical across states, checkout reported by default, dirty-checkout error and worktree retry.
- Checks: app-e2e-tsc, e2e-ui
- Reviewers: —

## WP-50 Sidebar at scale
- Phase: 8 US7 Widths, scale, global notices
- Tasks: T089
- Files: app/src/components/Sidebar.tsx, app/src/components/Sidebar.css, app/src/components/Sidebar.test.tsx, app/src/i18n/en/Sidebar.ts
- Depends on: WP-25
- Parallel group: 8A
- Acceptance: FR-056 — 6 tasks per project + «N más», «Filtrar tareas», rail counts.
- Checks: app
- Reviewers: —

## WP-51 Bandeja and Jira at width
- Phase: 8 US7 Widths, scale, global notices
- Tasks: T090, T091
- Files: app/src/components/InboxView.tsx, app/src/components/InboxView.css, app/src/components/InboxView.test.tsx, app/src/i18n/en/InboxView.ts, app/src/components/JiraView.tsx, app/src/components/JiraView.css, app/src/components/JiraView.test.tsx, app/src/i18n/en/JiraView.ts
- Depends on: WP-23, WP-37, WP-39
- Parallel group: 8A
- Acceptance: FR-056, FR-057 — inbox grouped by task and detail overlay below 1200; Jira drawer and «Filtros · N».
- Checks: app
- Reviewers: —

## WP-52 Task screen and board at width
- Phase: 8 US7 Widths, scale, global notices
- Tasks: T092, T093
- Files: app/src/components/TaskHeader.tsx, app/src/components/TaskHeader.css, app/src/components/TaskHeader.test.tsx, app/src/components/TaskScreen.tsx, app/src/components/TaskScreen.css, app/src/components/TaskScreen.test.tsx, app/src/components/BoardView.css
- Depends on: WP-23, WP-31, WP-34, WP-38
- Parallel group: 8A
- Acceptance: FR-057 — compact header at main-area ≤ 960, tree hidden below 1300, board columns min 220 px scrolling inside the board only.
- Checks: app
- Reviewers: —

## WP-53 Global notices
- Phase: 8 US7 Widths, scale, global notices
- Tasks: T094
- Files: app/src/components/GlobalNotices.tsx, app/src/components/GlobalNotices.css, app/src/components/GlobalNotices.test.tsx, app/src/components/UpdateNotice.tsx, app/src/components/UpdateNotice.test.tsx, app/src/App.tsx, app/src/i18n/en/GlobalNotices.ts
- Depends on: WP-19, WP-26
- Parallel group: 8A
- Acceptance: FR-058, FR-059 — quota and autopilot-pause notices from existing data, update banner restyled, mounted in App.
- Checks: app
- Reviewers: —

## WP-54 E2E widths
- Phase: 8 US7 Widths, scale, global notices
- Tasks: T095
- Files: app/e2e/widths.spec.ts (new)
- Depends on: WP-50, WP-51, WP-52, WP-53, WP-48
- Parallel group: 8B
- Acceptance: SC-008 — at 1024 no horizontal scroll except board columns, detail overlays below 1200, rail at 1280.
- Checks: app-e2e-tsc, e2e-ui
- Reviewers: —

## WP-55 Remove replaced code and i18n cleanup
- Phase: 9 Polish
- Tasks: T096, T097
- Files: app/src/components/PhaseBadge.tsx, app/src/components/PhaseBadge.test.tsx, app/src/components/PrBadge.tsx, app/src/components/PrBadge.test.tsx, app/src/components/AutopilotMark.tsx, app/src/components/AutopilotMark.test.tsx, app/src/components/PermissionCard.tsx, app/src/components/PermissionCard.test.tsx, app/src/components/status.ts, app/src/styles.css, app/src/inline-style.test.ts, app/src/i18n/en/legacy.ts, app/src/i18n/coverage.test.ts
- Depends on: WP-54, WP-48, WP-49, WP-35, WP-36, WP-40
- Parallel group: 9A
- Acceptance: SC-001, SC-002, SC-010 — old components and classes gone, final inline-style allow-list, orphan English keys deleted with `STRICT_ORPHANS = true`.
- Checks: app
- Reviewers: —

## WP-56 Preview seed
- Phase: 9 Polish
- Tasks: T098
- Files: scripts/seed-preview.sh
- Depends on: WP-14
- Parallel group: 9A
- Acceptance: a `--fresh` preview seeded with it shows every board state.
- Checks: e2e-api
- Reviewers: —

## WP-57 Decisions, status, README, architecture docs
- Phase: 9 Polish
- Tasks: T101, T102, T103
- Files: docs/decisions.md, docs/status.md, README.md, docs/ui-redesign.md, docs/architecture.md, docs/development.md
- Depends on: WP-14, WP-54
- Parallel group: 9A
- Acceptance: each decision says what, why and what was ruled out; status, README and architecture match the built behaviour.
- Checks: —
- Reviewers: —

## WP-58 Full checks
- Phase: 9 Polish
- Tasks: T099
- Files: (only the files of whatever area fails)
- Depends on: WP-55, WP-56, WP-57
- Parallel group: 9B
- Acceptance: SC-013 — fmt, clippy, `cargo test --workspace`, shell tests, tsc (app and e2e), `npx vitest run`, `scripts/e2e.sh`; no `not_implemented()` stub left.
- Checks: daemon, core, cli, integrations, tauri, app, e2e-api, e2e-ui
- Reviewers: contract

## WP-59 Before/after record
- Phase: 9 Polish
- Tasks: T100
- Files: app/e2e/screens.spec.ts, docs/redesign/despues/*.png
- Depends on: WP-58
- Parallel group: 9C
- Acceptance: SC-012 — 46 images with the same names as `docs/redesign/antes/`.
- Checks: e2e-ui
- Reviewers: —

## WP-60 Version bump
- Phase: 9 Polish
- Tasks: T104
- Files: Cargo.toml, app/src-tauri/Cargo.toml, app/package.json, app/src-tauri/tauri.conf.json, Cargo.lock, app/package-lock.json
- Depends on: WP-55, WP-56, WP-57, WP-58, WP-59
- Parallel group: 9D
- Acceptance: the four versions agree (minor); `node scripts/release/check.mjs` passes.
- Checks: —
- Reviewers: —
