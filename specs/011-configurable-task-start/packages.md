# Work packages — 011-configurable-task-start

Source: specs/011-configurable-task-start/tasks.md · 51 pending tasks · 16 packages

## WP-01 Workflow module skeleton
- Phase: 1 Setup
- Tasks: T001
- Files: crates/delilah-core/src/workflow.rs, crates/delilah-core/src/lib.rs
- Depends on: —
- Parallel group: 1A
- Acceptance: FR-001 — empty `workflow` module exported from delilah-core and the workspace builds
- Checks: rust:delilah-core
- Reviewers: —

## WP-02 Core workflow model, validation, resolution
- Phase: 2 Foundational
- Tasks: T002, T003, T004, T005, T006, T007, T008
- Files: crates/delilah-core/src/workflow.rs, crates/delilah-core/src/project.rs, crates/delilah-core/src/task.rs
- Depends on: WP-01
- Parallel group: 2A
- Acceptance: FR-001, FR-002, FR-004, FR-007, FR-009 — ProjectWorkflow/TaskWorkflow/render/validate/is_phased/resolve behave per data-model.md; ProjectSettings.workflow optional; Task.start_message exists
- Checks: rust:delilah-core
- Reviewers: contract, architecture

## WP-03 Preset texts, migration 0022, start_message storage
- Phase: 2 Foundational
- Tasks: T009, T010, T011, T012
- Files: crates/delilah-daemon/src/lang.rs, crates/delilah-daemon/src/store.rs, crates/delilah-daemon/migrations/0024_workflow_commands.sql
- Depends on: WP-02
- Parallel group: 2B
- Acceptance: FR-002, FR-003, FR-015 — existing projects get the `/feature` preset in their language, old tasks map to project/prompt, migration idempotent, start_message round-trips
- Checks: rust:delilah-daemon
- Reviewers: migration

## WP-04 Replace hard-coded "feature" task workflow
- Phase: 2 Foundational
- Tasks: T013
- Files: crates/delilah-daemon/src/actions.rs, crates/delilah-daemon/src/conversation.rs, crates/delilah-daemon/src/session_api.rs, crates/delilah-daemon/src/asks.rs, crates/delilah-daemon/src/pr_loop/mod.rs, crates/delilah-daemon/src/proposals.rs, crates/delilah-daemon/src/store.rs
- Depends on: WP-03
- Parallel group: 2C
- Acceptance: FR-015 — `rg '"feature"' crates/delilah-daemon/src` shows only the preset and the alias parser; behaviour unchanged
- Checks: rust:delilah-core, rust:delilah-daemon
- Reviewers: —

## WP-05 Start resolution in the daemon (built-in prompt, phased gating)
- Phase: 3 US1
- Tasks: T014, T015, T016, T017, T018
- Files: crates/delilah-daemon/src/run.rs, crates/delilah-daemon/src/autopilot/mod.rs, crates/delilah-daemon/src/autopilot/notes.rs, crates/delilah-daemon/src/api.rs, crates/delilah-daemon/src/proposals.rs, crates/delilah-e2e/tests/ (guess: new file, e.g. builtin_start.rs)
- Depends on: WP-04
- Parallel group: 3A
- Acceptance: FR-002, FR-003, FR-007, FR-009, FR-010, FR-015 — fresh project's first task gets the built-in prompt with no slash command and no saving mode/autopilot; `/feature` projects unchanged; autopilot on non-phased → 409 not-spec-workflow
- Checks: rust:delilah-daemon, rust:delilah-e2e (scripts/e2e.sh api)
- Reviewers: contract

## WP-06 Skills discovery endpoint and workflow error localisation
- Phase: 4 US2
- Tasks: T019, T020, T021, T022
- Files: crates/delilah-daemon/src/skills.rs, crates/delilah-daemon/src/lib.rs, crates/delilah-daemon/src/projects.rs, crates/delilah-daemon/src/api.rs
- Depends on: WP-05
- Parallel group: 4A
- Acceptance: FR-005, FR-006, FR-001 — GET /api/v1/projects/{name}/skills lists project/user/plugin skills; PUT /settings returns localised `workflow.*` field errors
- Checks: rust:delilah-daemon
- Reviewers: contract, security, architecture

## WP-07 Single start: message and "nothing" launch
- Phase: 4 US2
- Tasks: T023, T024
- Files: crates/delilah-daemon/src/run.rs
- Depends on: WP-05
- Parallel group: 4A
- Acceptance: FR-004, FR-007, FR-008 — `/fix {ticket}` renders and sends; "nothing" launches with no message and leaves the task waiting for the user
- Checks: rust:delilah-daemon
- Reviewers: —

## WP-08 UI contract types, presets, SkillPicker
- Phase: 4 US2
- Tasks: T025, T026, T027, T028
- Files: app/src/api.ts, app/src/workflowPresets.ts, app/src/workflowPresets.test.ts, app/src/components/SkillPicker.tsx, app/src/components/SkillPicker.test.tsx
- Depends on: WP-04
- Parallel group: 4A
- Acceptance: FR-005, FR-006, FR-011 — client types match contracts/api.md; presets and isPhased available; SkillPicker lists, filters, allows free text
- Checks: app (tsc --noEmit, vitest related)
- Reviewers: contract

## WP-09 WorkflowEditor (single) and project settings section
- Phase: 4 US2
- Tasks: T029, T030, T031, T032
- Files: app/src/components/WorkflowEditor.tsx, app/src/components/WorkflowEditor.test.tsx, app/src/components/ProjectSettings.tsx, app/src/components/ProjectSettings.test.tsx, app/src/i18n/en/ (guess: files per component), app/src/i18n/coverage.test.ts
- Depends on: WP-08
- Parallel group: 4B
- Acceptance: FR-001, FR-006, FR-011 — "Flujo de trabajo" section lets a project pick built-in/message/nothing or a preset, warns on unknown skill, shows daemon field errors
- Checks: app
- Reviewers: —

## WP-10 Phased hand-offs with project commands
- Phase: 5 US3
- Tasks: T033, T034, T035
- Files: crates/delilah-daemon/src/handoff.rs, crates/delilah-daemon/src/run.rs, crates/delilah-daemon/src/projects.rs
- Depends on: WP-03, WP-07
- Parallel group: 5A
- Acceptance: FR-010, FR-011 — hand-off sends the project's implement/review commands; `/feature` preset output identical to today; non-phased project hand-off fails with the localised error; phased settings missing `review` rejected
- Checks: rust:delilah-daemon
- Reviewers: —

## WP-11 Phased UI editing and gating of saving mode/autopilot
- Phase: 5 US3
- Tasks: T036, T037, T038
- Files: app/src/components/WorkflowEditor.tsx, app/src/components/WorkflowEditor.test.tsx, app/src/components/NewTaskDialog.tsx, app/src/components/NewTaskDialog.test.tsx, app/src/components/BacklogTaskView.tsx, app/src/components/BacklogTaskView.test.tsx, app/src/components/ProposalCard.tsx, app/src/components/ProposalCard.test.tsx
- Depends on: WP-09
- Parallel group: 5A
- Acceptance: FR-009, FR-011 — phased mode with three skill fields; saving mode and autopilot shown only for phased tasks, with a reason when hidden
- Checks: app
- Reviewers: —

## WP-12 Per-task start override in the API
- Phase: 6 US4
- Tasks: T039, T040, T041
- Files: crates/delilah-daemon/src/api.rs, crates/delilah-daemon/src/run.rs
- Depends on: WP-10
- Parallel group: 6A
- Acceptance: FR-007, FR-012, FR-015 — create/PATCH store workflow and start_message; invalid templates 400 with field errors; backlog task keeps its message; legacy prompt task without message starts with none
- Checks: rust:delilah-daemon
- Reviewers: contract, security

## WP-13 CLI: --no-prompt, workflow display and unset
- Phase: 6 US4
- Tasks: T042
- Files: crates/delilah-cli/src/main.rs
- Depends on: WP-04
- Parallel group: 6A
- Acceptance: FR-013 — `--no-prompt`, new `--workflow` values, `project show` prints workflow, `project unset <name> workflow`, deprecation warning on `project add --workflow`
- Checks: rust:delilah-cli
- Reviewers: contract

## WP-14 New-task and backlog start UI
- Phase: 6 US4
- Tasks: T043, T044, T045, T046
- Files: app/src/components/NewTaskDialog.tsx, app/src/components/NewTaskDialog.test.tsx, app/src/components/BacklogTaskView.tsx, app/src/components/BacklogTaskView.test.tsx, app/src/components/WorkflowEditor.tsx, app/src/i18n/en/ (guess)
- Depends on: WP-11
- Parallel group: 6A
- Acceptance: FR-012 — "Al empezar" shows the resolved start, can be overridden by skill or "Nada"; backlog tasks show and edit their start via PATCH
- Checks: app
- Reviewers: —

## WP-15 Documentation
- Phase: 7 Polish
- Tasks: T047, T048, T049
- Files: docs/decisions.md, docs/usage.md, docs/status.md, docs/architecture.md, README.md
- Depends on: WP-12, WP-13, WP-14
- Parallel group: 7A
- Acceptance: FR-014 — decisions 15/46 amended and new decision added; usage, README, status, architecture describe the feature
- Checks: —
- Reviewers: —

## WP-16 Full verification and quickstart walk
- Phase: 7 Polish
- Tasks: T050, T051
- Files: —
- Depends on: WP-15
- Parallel group: 7B
- Acceptance: SC all — workspace, Tauri shell, web UI and e2e green; quickstart §1–§5 walked in a preview
- Checks: rust:workspace, app/src-tauri, app (tsc, tsc -p e2e, vitest run), scripts/e2e.sh api
- Reviewers: —
