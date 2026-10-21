# Work packages — 004-agent-playground

Source: specs/004-agent-playground/tasks.md · 48 pending tasks · 15 packages

Check names used below: `agents` = `cargo test -p delilah-agents`; `daemon` = `cargo test -p delilah-daemon`; `ui` = `cd app && npx tsc --noEmit && npx vitest related --run <files>`; `tauri` = `cargo test --manifest-path app/src-tauri/Cargo.toml`; `workspace` = `cargo test --workspace`; `ui-all` = `cd app && npx tsc --noEmit && npx vitest run`.

Scheduling note: the daemon track (WP-06, WP-08, WP-09) has no code dependency on the UI track and can start right after Setup, in parallel with WP-02..WP-05. Its group ids carry phase 6 because a package never spans phases; the orchestrator should schedule by "Depends on".

## WP-01 Playground folder, credits and strings
- Phase: 1 Setup
- Tasks: T001, T002
- Files: app/src/playground/CREDITS.md, app/src/i18n/en.ts
- Depends on: —
- Parallel group: 1A
- Acceptance: FR-013 — CREDITS.md exists with the original-art statement and an empty table; all playground strings are in en.ts.
- Checks: ui
- Reviewers: —

## WP-02 Layout model, data hook and sidebar wiring
- Phase: 2 Foundational
- Tasks: T003, T004, T005, T006, T007
- Files: app/src/playground/layout.ts, app/src/playground/layout.test.ts, app/src/playground/usePlayground.ts, app/src/components/Sidebar.tsx, app/src/components/Sidebar.test.tsx, app/src/App.tsx, app/src/playground/PlaygroundView.tsx
- Depends on: WP-01
- Parallel group: 2A
- Acceptance: FR-001, FR-021 — Playground is reachable from the sidebar and lists live Claude sessions grouped by project, with stable slots.
- Checks: ui
- Reviewers: architecture

## WP-03 US1 Desks, characters and states
- Phase: 3 User Story 1
- Tasks: T008, T009, T010, T011, T012, T013, T014, T015
- Files: app/src/playground/layout.ts, app/src/playground/layout.test.ts, app/src/playground/Sprite.tsx, app/src/playground/Sprite.test.tsx, app/src/playground/sprites.ts, app/src/playground/CREDITS.md, app/src/playground/Desk.tsx, app/src/playground/PlaygroundView.tsx, app/src/playground/PlaygroundView.test.tsx, app/src/playground/playground.css
- Depends on: WP-02
- Parallel group: 3A
- Acceptance: FR-002, FR-003, FR-004, FR-005 — one desk and character per live agent with key, title and phase, a pose per state, an empty-office message, and stable slots across changes.
- Checks: ui
- Reviewers: architecture

## WP-04 US2 Waiting highlight, counter and click to open
- Phase: 4 User Story 2
- Tasks: T016, T017, T018, T019
- Files: app/src/playground/layout.ts, app/src/playground/layout.test.ts, app/src/playground/PlaygroundView.tsx, app/src/playground/PlaygroundView.test.tsx, app/src/playground/Desk.tsx, app/src/playground/playground.css
- Depends on: WP-03
- Parallel group: 4A
- Acceptance: FR-006, FR-007, FR-022 — waiting desks are highlighted (not by colour alone), the header counts them, and click or Enter calls onOpen; each character has an accessible name.
- Checks: ui
- Reviewers: —

## WP-05 Separate window (browser tab and Tauri window)
- Phase: 5 Separate window
- Tasks: T020, T021, T022, T023, T024
- Files: app/src/platform.ts, app/src/platform.test.ts, app/src-tauri/capabilities/default.json, app/src/playground/PlaygroundWindow.tsx, app/src/main.tsx, app/src/playground/PlaygroundView.tsx, app/src/playground/PlaygroundView.test.tsx, app/src/App.tsx
- Depends on: WP-04
- Parallel group: 5A
- Acceptance: FR-019, FR-020 — "Abrir en ventana" opens or reuses the playground window; a click there makes the main window open the task; the Tauri capability allows exactly what is needed.
- Checks: ui, tauri
- Reviewers: security, contract, architecture

## WP-06 Adapter hooks, action classifier and activity events (daemon track)
- Phase: 6 User Story 3
- Tasks: T025, T026
- Files: crates/delilah-agents/src/claude.rs
- Depends on: —
- Parallel group: 6A
- Acceptance: FR-008, FR-009, FR-015 — settings() registers the new hook events, action_of gives kind plus a truncated, char-safe target that never leaks Write content, and classify_activity yields the activity events while classify() is unchanged.
- Checks: agents
- Reviewers: security, contract

## WP-07 Action poses for the character
- Phase: 6 User Story 3
- Tasks: T034
- Files: app/src/playground/sprites.ts, app/src/playground/CREDITS.md
- Depends on: WP-03
- Parallel group: 6A
- Acceptance: FR-009, FR-013 — typing, reading, terminal, phone and busy poses exist with 2 frames each and are credited.
- Checks: ui
- Reviewers: —

## WP-08 Daemon activity model (daemon track)
- Phase: 6 User Story 3
- Tasks: T027, T028
- Files: crates/delilah-daemon/src/activity.rs, crates/delilah-daemon/src/lib.rs, crates/delilah-daemon/src/main.rs (guess)
- Depends on: WP-06
- Parallel group: 6B
- Acceptance: FR-008, FR-010, FR-015, FR-018 — Activity::apply/clear track the main action and ordered helpers per the unit tests, with a serde view matching contracts/api.md.
- Checks: daemon
- Reviewers: architecture

## WP-09 Daemon activity wiring: topic, hook and endpoint (daemon track)
- Phase: 6 User Story 3
- Tasks: T029, T030
- Files: crates/delilah-daemon/src/hooks.rs, crates/delilah-daemon/src/events.rs, crates/delilah-daemon/src/sessions.rs, crates/delilah-daemon/src/session_api.rs, crates/delilah-daemon/src/api.rs
- Depends on: WP-08
- Parallel group: 6C
- Acceptance: FR-005, FR-008, FR-010, FR-018 — hooks update live activity and emit Topic::Activity only on change; waiting or paused clears it; GET /api/v1/activity returns it.
- Checks: daemon, workspace
- Reviewers: contract, security

## WP-10 UI activity data and caption logic
- Phase: 6 User Story 3
- Tasks: T031, T032, T033
- Files: app/src/api.ts, app/src/live.ts, app/src/playground/layout.ts, app/src/playground/layout.test.ts, app/src/playground/usePlayground.ts
- Depends on: WP-09, WP-04
- Parallel group: 6D
- Acceptance: FR-008, FR-009 — api.activity() and the activity topic exist; deskState, caption and accessibleName use the activity; sessions carry their activity into the desk model.
- Checks: ui
- Reviewers: contract

## WP-11 Caption and action poses on the desk
- Phase: 6 User Story 3
- Tasks: T035
- Files: app/src/playground/Desk.tsx, app/src/playground/PlaygroundView.test.tsx
- Depends on: WP-10, WP-07, WP-05
- Parallel group: 6E
- Acceptance: FR-009, FR-010 — a working desk shows the pose and caption of its action (edit gives typing and "Edit · api.rs"; foreground delegate gives phone and "Supervisando"), cleared when there is none.
- Checks: ui
- Reviewers: —

## WP-12 US4 Helpers around the desk
- Phase: 7 User Story 4
- Tasks: T036, T037, T038, T039, T040
- Files: app/src/playground/layout.ts, app/src/playground/layout.test.ts, app/src/playground/sprites.ts, app/src/playground/CREDITS.md, app/src/playground/PlaygroundView.test.tsx, app/src/playground/Desk.tsx, app/src/playground/playground.css
- Depends on: WP-11
- Parallel group: 7A
- Acceptance: FR-015, FR-016, FR-017, FR-018 — up to 4 helpers at stable semicircle spots with role looks and own poses, "+N" badge for the rest, fade out on end, accessible names list helpers.
- Checks: ui
- Reviewers: —

## WP-13 US5 Stale state, pause, themes, reduced motion and art polish
- Phase: 8 User Story 5
- Tasks: T041, T042, T043, T044
- Files: app/src/playground/PlaygroundView.tsx, app/src/playground/PlaygroundView.test.tsx, app/src/playground/PlaygroundWindow.tsx, app/src/playground/playground.css, app/src/playground/sprites.ts
- Depends on: WP-12
- Parallel group: 8A
- Acceptance: FR-011, FR-012, FR-014 — offline dims and freezes the office with "Reconectando…", hidden pauses animation, reduced motion holds still, dark theme and 1024 px layouts read clearly.
- Checks: ui
- Reviewers: —

## WP-14 Docs: README, status, architecture, decisions
- Phase: 9 Polish
- Tasks: T045, T046, T047
- Files: README.md, docs/status.md, docs/architecture.md, docs/decisions.md
- Depends on: WP-13
- Parallel group: 9A
- Acceptance: FR-001, FR-008, FR-013 — the docs describe the built playground, its limits and the decisions taken.
- Checks: —
- Reviewers: —

## WP-15 Full checks and quickstart run
- Phase: 9 Polish
- Tasks: T048
- Files: — (verification only; fixes go where the failure is)
- Depends on: WP-14
- Parallel group: 9B
- Acceptance: all success criteria — workspace, tauri and ui-all pass; quickstart.md runs end to end in a preview; desktop-only items are noted.
- Checks: workspace, tauri, ui-all
- Reviewers: —
