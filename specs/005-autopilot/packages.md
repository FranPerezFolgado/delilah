# Work packages — 005-autopilot

Source: specs/005-autopilot/tasks.md · 55 pending tasks · 21 packages

Phase 1 Setup has no tasks (the `tokio` `process` feature check is inside T011, WP-04). Check abbreviations: `core` = `cargo test -p delilah-core`; `daemon` = `cargo test -p delilah-daemon`; `cli` = `cargo test -p delilah-cli`; `web` = `cd app && npx tsc --noEmit && npx vitest related --run <files>`; `workspace` = `cargo test --workspace`; `tauri` = `cargo test --manifest-path app/src-tauri/Cargo.toml`; `vitest-all` = `cd app && npx vitest run`.

## WP-01 Core autopilot state, events and project field
- Phase: 2 Foundational (core + storage)
- Tasks: T001, T002, T003
- Files: crates/delilah-core/src/task.rs, crates/delilah-core/src/lib.rs, crates/delilah-core/src/project.rs
- Depends on: —
- Parallel group: 2A
- Acceptance: FR-001, FR-003, FR-012 — `Autopilot` and `Event::Autopilot*` round-trip as in data-model.md, old JSON loads with `None`, new events leave statuses unchanged, `Project.preview_command` loads from old JSON.
- Checks: core
- Reviewers: contract

## WP-02 Storage and migration 0015
- Phase: 2 Foundational (core + storage)
- Tasks: T004, T005, T006, T007
- Files: crates/delilah-daemon/migrations/0015_autopilot.sql, crates/delilah-daemon/src/store.rs, crates/delilah-daemon/src/store_projects.rs
- Depends on: WP-01
- Parallel group: 2B
- Acceptance: FR-001, FR-002, FR-018 — autopilot persists with the task and forces `saving_mode`, turning saving mode off clears it, `preview_command` is stored (blank becomes NULL), new events appear in history.
- Checks: core, daemon
- Reviewers: migration

## WP-03 Reusable PR opening and preview script
- Phase: 3 Foundational (PR and preview)
- Tasks: T008, T009, T016
- Files: crates/delilah-daemon/src/pr_api/open.rs, crates/delilah-daemon/src/pr_api/support.rs (guess, FakeGh), scripts/preview.sh
- Depends on: WP-02
- Parallel group: 3A
- Acceptance: FR-009, FR-018 — `open_for` is extracted and the `POST /tasks/{key}/prs` handler still passes every test; draft flag and `has-pr` conflict work; `scripts/preview.sh` prints `PREVIEW_URL=` first.
- Checks: daemon
- Reviewers: security

## WP-04 Daemon-owned preview process and its API
- Phase: 3 Foundational (PR and preview)
- Tasks: T010, T011, T012, T013
- Files: crates/delilah-daemon/src/preview.rs, crates/delilah-daemon/src/state.rs, crates/delilah-daemon/src/lib.rs, crates/delilah-daemon/src/main.rs, crates/delilah-daemon/Cargo.toml (only if `process` or libc/nix is missing), crates/delilah-daemon/src/api.rs, crates/delilah-daemon/src/projects.rs
- Depends on: WP-02
- Parallel group: 3A
- Acceptance: FR-018, FR-018a, FR-020a — `Previews` starts, parses the URL, times out, kills the process group and stops on shutdown; `GET/POST/DELETE /tasks/{key}/preview` and `PATCH /projects/{name}` with `preview_command` behave as in contracts/api.md.
- Checks: daemon
- Reviewers: contract, security, architecture

## WP-05 Stop the preview when the task ends
- Phase: 3 Foundational (PR and preview)
- Tasks: T014, T015
- Files: crates/delilah-daemon/src/transitions.rs, crates/delilah-daemon/src/actions.rs, crates/delilah-daemon/src/state.rs (guess, `AppState::change`)
- Depends on: WP-04
- Parallel group: 3B
- Acceptance: FR-020 — mark-done, fail, archive and delete each leave no preview for the task.
- Checks: daemon
- Reviewers: —

## WP-06 Autopilot on run request and plan note
- Phase: 4 US1 From idea to open PR
- Tasks: T017, T018
- Files: crates/delilah-daemon/src/run.rs, crates/delilah-daemon/src/api.rs, crates/delilah-daemon/src/handoff.rs
- Depends on: WP-02
- Parallel group: 4A
- Acceptance: FR-001, FR-002, FR-004, FR-005, FR-022 — `RunRequest.autopilot` stores state `running` and forces saving mode (400 with `saving_mode: false`); the first prompt carries the autopilot plan note in both languages.
- Checks: daemon
- Reviewers: contract

## WP-07 Autopilot chain: shape, notes, outcome file, finish
- Phase: 4 US1 From idea to open PR
- Tasks: T019, T020, T021, T022
- Files: crates/delilah-daemon/src/autopilot.rs, crates/delilah-daemon/src/handoff.rs, crates/delilah-daemon/src/lib.rs
- Depends on: WP-03, WP-05, WP-06
- Parallel group: 4B
- Acceptance: FR-006, FR-007, FR-008, FR-009, FR-010, FR-011, FR-012, FR-016 — plan to implementation to review run with no offer; a `ready` outcome opens one draft PR per repo (agent title/body or `proposal()` fallback) and ends in `done` with `AutopilotFinished`; a repeated finish opens nothing; recovery links an existing PR.
- Checks: daemon
- Reviewers: architecture, security

## WP-08 Preview step of the finish and scheduler recovery
- Phase: 4 US1 From idea to open PR
- Tasks: T023, T024
- Files: crates/delilah-daemon/src/autopilot.rs, crates/delilah-daemon/src/scheduler.rs
- Depends on: WP-07
- Parallel group: 4C
- Acceptance: FR-009, FR-019, FR-020 — with a preview command the preview starts first and the PR body gets `Preview: <url>`; a failed preview does not pause; the scheduler tick recovers `opening_pr` or `preview` stages.
- Checks: daemon
- Reviewers: —

## WP-09 Pause, resume, nudge and failure pauses
- Phase: 5 US2 Pause into the Bandeja
- Tasks: T025, T026, T027, T028
- Files: crates/delilah-daemon/src/autopilot.rs, crates/delilah-daemon/src/handoff.rs, crates/delilah-daemon/src/api.rs
- Depends on: WP-08
- Parallel group: 5A
- Acceptance: FR-003, FR-012, FR-013, FR-013b, FR-014, FR-015, FR-016 — every stall or failure pauses with a localised reason and one notification; one nudge per step; `POST /tasks/{key}/autopilot {resume|off}` continues from the first unfinished step; `409 not-paused`.
- Checks: daemon
- Reviewers: contract, architecture

## WP-10 Recommended answers in the ask hook
- Phase: 5 US2 Pause into the Bandeja
- Tasks: T029, T030
- Files: crates/delilah-daemon/src/asks.rs, crates/delilah-daemon/src/autopilot.rs
- Depends on: WP-09
- Parallel group: 5B
- Acceptance: FR-013a, FR-012 — questions with exactly one recommended option per question are answered in the hook with one history event each; any other case pauses and the ask goes on as today.
- Checks: daemon
- Reviewers: security

## WP-11 Bandeja item and permission pause
- Phase: 5 US2 Pause into the Bandeja
- Tasks: T031, T032
- Files: crates/delilah-daemon/src/inbox.rs
- Depends on: WP-09
- Parallel group: 5B
- Acceptance: FR-013, FR-014, FR-017 — a permission prompt pauses autopilot; one `autopilot` inbox item exists per pause; the usage limit leaves autopilot untouched.
- Checks: daemon
- Reviewers: contract

## WP-12 Daemon API for the autopilot option
- Phase: 6 US3 Choose autopilot
- Tasks: T033, T034
- Files: crates/delilah-daemon/src/api.rs, crates/delilah-daemon/src/handoff.rs, crates/delilah-daemon/src/autopilot.rs
- Depends on: WP-09
- Parallel group: 6A
- Acceptance: FR-001, FR-002, FR-004 — `NewTask` and `BacklogChange` accept `autopilot`; `POST /autopilot {on}` confirms an offered handoff and returns `409 not-spec-workflow` where saving mode is not allowed; turning saving mode off clears autopilot.
- Checks: daemon
- Reviewers: contract

## WP-13 CLI flags and subcommand
- Phase: 6 US3 Choose autopilot
- Tasks: T035
- Files: crates/delilah-cli/src/main.rs
- Depends on: WP-09
- Parallel group: 6A
- Acceptance: FR-004 — `--autopilot`/`--no-autopilot` on `new` and `run` (conflicting with `--no-saving-mode`); `delilah autopilot <KEY> on|off|resume`.
- Checks: cli
- Reviewers: —

## WP-14 Web contract, new-task option and English strings
- Phase: 6 US3 Choose autopilot
- Tasks: T036, T037, T038, T043
- Files: app/src/api.ts, app/src/components/NewTaskDialog.tsx, app/src/components/NewTaskDialog.test.tsx, app/src/components/BacklogTaskView.tsx, app/src/components/BacklogTaskView.test.tsx, app/src/i18n/en.ts
- Depends on: WP-02
- Parallel group: 6A
- Acceptance: FR-001, FR-002, FR-004, FR-022 — client types match contracts/api.md; ticking "Autopiloto" ticks and disables "Modo ahorro" and the request carries both flags; `en.ts` has strings for all US3 components (T039–T042 texts included).
- Checks: web
- Reviewers: contract

## WP-15 Autopilot mark, header and banner
- Phase: 6 US3 Choose autopilot
- Tasks: T039, T040
- Files: app/src/components/AutopilotMark.tsx, app/src/components/AutopilotMark.test.tsx, app/src/components/TaskScreen.tsx, app/src/components/BoardView.tsx, app/src/components/HandoffActions.tsx
- Depends on: WP-14
- Parallel group: 6B
- Acceptance: FR-003, FR-014 — running, paused (with reason) and done states show in the task header and board card; the paused banner has "Reanudar autopiloto" and "Seguir a mano".
- Checks: web
- Reviewers: —

## WP-16 Bandeja item and chain steps in the UI
- Phase: 6 US3 Choose autopilot
- Tasks: T041, T042
- Files: app/src/components/InboxView.tsx, app/src/components/InboxView.test.tsx, app/src/chain.ts, app/src/chain.test.ts, app/src/components/SessionChain.tsx
- Depends on: WP-14
- Parallel group: 6B
- Acceptance: FR-003, FR-014 — `autopilot` inbox items show the reason with both actions; the chain adds "Preview" (only with a preview command) and "PR" steps, and shows `waiting` when paused.
- Checks: web
- Reviewers: —

## WP-17 Preview UI and project setting
- Phase: 7 US4 Preview of the branch
- Tasks: T044, T045, T046, T047
- Files: app/src/api.ts, app/src/components/PreviewControl.tsx, app/src/components/PreviewControl.test.tsx, app/src/components/TaskScreen.tsx, app/src/components/ProjectView.tsx, app/src/components/ProjectView.test.tsx (guess), app/src/i18n/en.ts
- Depends on: WP-04, WP-15
- Parallel group: 7A
- Acceptance: FR-018, FR-019, FR-020a — "Comando de preview" saves on blur or enter and an empty value clears it; the task shows start, URL link, stop, and failure with retry; hidden without a preview command.
- Checks: web
- Reviewers: contract

## WP-18 Idea proposal launch on autopilot (daemon)
- Phase: 8 US5 Launch from an idea
- Tasks: T048, T049
- Files: crates/delilah-daemon/src/proposals.rs, crates/delilah-daemon/src/asks.rs
- Depends on: WP-06, WP-10
- Parallel group: 8A
- Acceptance: FR-021 — `Answer.launch = "autopilot"` with "Crear" creates the task with autopilot on and starts it; a start failure returns `{task, error}` and leaves the backlog task; other answers ignore `launch`.
- Checks: daemon
- Reviewers: contract

## WP-19 Idea proposal launch button (app)
- Phase: 8 US5 Launch from an idea
- Tasks: T050
- Files: app/src/components/ProposalCard.tsx, app/src/components/ProposalCard.test.tsx, app/src/api.ts, app/src/i18n/en.ts
- Depends on: WP-18, WP-17
- Parallel group: 8B
- Acceptance: FR-021 — "Crear y lanzar en autopiloto" next to "Crear"; on success it links to the running task, on error it shows the error.
- Checks: web
- Reviewers: contract

## WP-20 Documentation
- Phase: 9 Polish
- Tasks: T051, T052, T053
- Files: docs/architecture.md, docs/decisions.md, docs/status.md, docs/development.md, README.md
- Depends on: WP-19
- Parallel group: 9A
- Acceptance: Docs describe autopilot, previews, decisions taken and known limits; README covers the user-facing behaviour.
- Checks: —
- Reviewers: —

## WP-21 Version bump and full check
- Phase: 9 Polish
- Tasks: T054, T055
- Files: Cargo.toml, app/src-tauri/Cargo.toml, app/package.json, app/src-tauri/tauri.conf.json, Cargo.lock, app/src-tauri/Cargo.lock, app/package-lock.json
- Depends on: WP-20
- Parallel group: 9B
- Acceptance: Minor bump from `main` in its own commit; every check is green; quickstart.md is walked in a preview.
- Checks: workspace, tauri, vitest-all
- Reviewers: —
