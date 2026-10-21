---
description: "Task list for Per-project configuration"
---

# Tasks: Per-project configuration

**Input**: Design documents from `specs/004-project-config/`

**Prerequisites**: plan.md, spec.md, research.md, data-model.md, contracts/api.md, contracts/cli.md, quickstart.md

**Tests**: included, test-first. Each story writes its failing tests before the code, in
the inline `#[cfg(test)]` style of each crate (in-memory `Store`, temp dirs, fixture
repos from `scripts/make-fixture-parent.sh`) and vitest + Testing Library in the app.

**Organization**: grouped by user story (US1 settings screen and CLI, US2 branch
template, US3 worktrees ready, US4 submodule rules, US5 default workspace mode).

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies on incomplete tasks)
- **[Story]**: US1–US5 (spec.md)

## Path Conventions

- Core: `crates/delilah-core/src/{project.rs,task.rs}`, `crates/delilah-core/tests/branch_names.json`
- Git: `crates/delilah-git/src/{share.rs,lib.rs}`
- Agents: `crates/delilah-agents/src/pty.rs`
- Daemon: `crates/delilah-daemon/src/{store_projects.rs,projects.rs,run.rs,setup.rs,store_sessions.rs,api.rs,actions.rs,main.rs}`, `crates/delilah-daemon/migrations/`
- CLI: `crates/delilah-cli/src/main.rs`
- App: `app/src/api.ts`, `app/src/branchName.ts`, `app/src/status.ts`, `app/src/components/*.tsx`, `app/src/i18n/en.ts`, `app/src/styles.css`

Try everything in a preview (`scripts/preview.sh`), never against the daemon on 7420.

---

## Phase 1: Setup

- [X] T001 Add `unicode-normalization` to `crates/delilah-core/Cargo.toml` (workspace dependency in the root `Cargo.toml` if that is the convention there)

---

## Phase 2: Foundational (blocks every story)

**Purpose**: settings type and storage, the API shape, the PTY exit code and the launch split that later stories build on. No behaviour changes yet.

### Tests first

- [X] T002 [P] Write failing tests in `crates/delilah-core/src/project.rs`: `ProjectSettings::default()` equals today's behaviour (checkout, `feature/{ticket}`, no shared, no setup, empty always/never); `serde_json::from_str("{}")` gives the defaults; a JSON with every field round-trips; unknown keys are refused on input
- [X] T003 [P] Write failing tests in `crates/delilah-core/src/project.rs` for `validate(&settings, kind) -> Vec<FieldError>`: branch without `{ticket}`, unknown placeholder `{foo}`, empty branch; shared path absolute, with `..`, empty, duplicated; submodule rules on a repo project; the same submodule in always and never; each error names its field (`branch`, `shared[1].path`, `submodules.never[0]`)
- [X] T004 [P] Write a failing test in `crates/delilah-agents/src/pty.rs` that `exit_code()` returns `Some(0)` for `sh -c 'exit 0'` and `Some(3)` for `exit 3` after `wait()`
- [ ] T005 [P] Write failing store tests in `crates/delilah-daemon/src/store_projects.rs`: a project created today reads back with default settings; `set_project_settings` round-trips; settings survive a rename; an unparsable stored value logs and reads as defaults

### Implementation

- [X] T006 Implement `ProjectSettings`, `WorkspaceMode`, `SharedEntry`, `ShareMode`, `SubmoduleRules`, `FieldError` and `validate()` in `crates/delilah-core/src/project.rs`; add `settings: ProjectSettings` with `#[serde(default)]` to `Project`; re-export from `crates/delilah-core/src/lib.rs`
- [X] T007 [P] Keep the child's exit status in `crates/delilah-agents/src/pty.rs` (store it when `pump_output` waits on the child) and expose `exit_code() -> Option<u32>`
- [ ] T008 Add `crates/delilah-daemon/migrations/0011_project_settings.sql`: `ALTER TABLE projects ADD COLUMN settings TEXT NOT NULL DEFAULT '{}';`
- [ ] T009 Read `settings` into `Project` in every query of `crates/delilah-daemon/src/store_projects.rs`, and add `set_project_settings(name, &ProjectSettings)`
- [X] T010 [P] Add `ProjectSettings`, `SharedEntry` and `Project.settings` to `app/src/api.ts`, following contracts/api.md
- [X] T011 Split everything after `WorkspaceReady` in `start_run` (ticket file, notes, adopted or new primary session, `launch_session`) into `pub async fn launch_agent(app, key, ws, …)` in `crates/delilah-daemon/src/run.rs`, with no behaviour change; existing run tests stay green

**Checkpoint**: `cargo test --workspace` and `cd app && npx tsc --noEmit` pass; projects report default settings.

---

## Phase 3: User Story 1 — Configure a project from its settings screen (P1) 🎯 MVP

**Goal**: settings can be seen and changed from the app and the CLI, validated, never written into the repo.

**Independent Test**: open a project's settings, set a branch template and a shared `.env`, save, reopen: the values are there and `git status` in the checkout is clean.

### Tests first

- [ ] T012 [P] [US1] Write failing API tests in `crates/delilah-daemon/src/projects.rs`: `PUT /api/v1/projects/{name}/settings` saves and returns the project; invalid input → 422 with `fields`, nothing saved; unknown project → 404; a submodule rule naming a submodule the fixture parent does not have → 422 on that field; `GET /api/v1/projects` returns full settings for projects with none saved
- [ ] T013 [P] [US1] Write failing tests in `crates/delilah-cli/src/main.rs` for parsing `project show`, `project set <name> workspace|branch|setup <value>`, `project set <name> shared add|rm <path> [--copy|--link]`, `project set <name> submodules always|never add|rm <path>`, `project unset <name> <setting>`
- [ ] T014 [P] [US1] Write failing tests in `app/src/components/ProjectSettings.test.tsx`: fields show the current settings; parent-only fields hidden for a repo project; save sends the whole settings; 422 field errors appear under their fields; leaving with unsaved edits asks to discard

### Implementation

- [ ] T015 [US1] Implement `update_settings` in `crates/delilah-daemon/src/projects.rs` (core `validate` + submodule existence via `delilah_git::list_submodules`, localised messages with `app.lang().pick`, emit `Topic::Projects`), and route `PUT /api/v1/projects/{name}/settings` in `crates/delilah-daemon/src/api.rs`
- [ ] T016 [US1] Extend `ApiError` in `crates/delilah-daemon/src/api.rs` so a 422 can carry `fields: [{field, message}]`
- [ ] T017 [US1] Implement `project show`, `project set` and `project unset` in `crates/delilah-cli/src/main.rs` (read the project, change one field, `PUT`; print `field: message` lines and exit 1 on 422)
- [ ] T018 [P] [US1] Add `api.updateProjectSettings(name, settings)` returning field errors in `app/src/api.ts`
- [ ] T019 [US1] Build `app/src/components/ProjectSettings.tsx`: default place (WherePicker), branch template, shared entries (path + auto/copy/link, add/remove), setup command, always/never submodule pickers for parent projects; save, field errors, discard confirm
- [ ] T020 [US1] Add an "Ajustes" action per project row in `app/src/components/ProjectsView.tsx` that opens `ProjectSettings`
- [ ] T021 [P] [US1] Add the new texts to `app/src/i18n/en.ts` and styles to `app/src/styles.css`

**Checkpoint**: settings editable from the screen and the CLI; nothing else uses them yet.

---

## Phase 4: User Story 2 — Branch names follow the project's convention (P1)

**Goal**: new tasks get the branch the template gives; the form shows it.

**Independent Test**: set `feature/{ticket}-{slug}`; start a task; the workspace is on `feature/<KEY>-<slug>`.

### Tests first

- [ ] T022 [P] [US2] Create `crates/delilah-core/tests/branch_names.json` with cases `{template, key, title, branch}`: default template; accents ("Paginación" → "paginacion"); punctuation and `/`; repeated and edge hyphens; 40-character cut at a hyphen; empty title with `{slug}`; free task key `LIBRE-3`
- [ ] T023 [P] [US2] Write a failing test in `crates/delilah-core/src/project.rs` that runs every case of `tests/branch_names.json` through `branch_name()`
- [ ] T024 [P] [US2] Write a failing test in `app/src/branchName.test.ts` that runs the same JSON through `branchName()`
- [ ] T025 [P] [US2] Write failing run tests in `crates/delilah-daemon/src/run.rs`: with a template, the workspace branch follows it; an explicit `branch` in the request wins; no settings → `feature/<key>`; on the fixture parent, every selected submodule is on the templated branch

### Implementation

- [ ] T026 [US2] Implement `slug()` and `branch_name()` in `crates/delilah-core/src/project.rs`
- [ ] T027 [P] [US2] Implement `branchName()` in `app/src/branchName.ts`
- [ ] T028 [US2] Use `branch_name(settings.branch, key, title)` as the default branch in `crates/delilah-daemon/src/run.rs` (replacing `format!("feature/{}", key)`)
- [ ] T029 [US2] Show "Rama: …" from `branchName()` under the title in `app/src/components/NewTaskDialog.tsx`, updating as the user types

**Checkpoint**: US1 + US2 work on their own.

---

## Phase 5: User Story 3 — Worktrees are ready to work in (P1)

**Goal**: shared entries are brought in and the setup runs before the agent, with live output, cancel, and recovery when it fails.

**Independent Test**: share `.env`, set setup `echo ok > .setup-ran`; start a worktree task; both files are there before Claude's first prompt.

### Tests first

- [ ] T030 [P] [US3] Write failing tests in `crates/delilah-git/src/share.rs` with temp dirs: copy a file; link a folder (symlink to the checkout's absolute path); auto mode by kind; an entry already in the worktree left alone and reported; a missing source skipped and reported
- [ ] T031 [P] [US3] Write failing transition tests in `crates/delilah-core/src/task.rs`: `PreparingWorkspace` + `SetupFailed` → `SetupFailed`; `SetupFailed` + `RetrySetup`/`SkipSetup` → `PreparingWorkspace`; `SetupFailed` + `Fail`/`MarkDone` allowed; `SetupFailed` is not terminal; the new events are refused from other statuses
- [ ] T032 [P] [US3] Write failing daemon tests in `crates/delilah-daemon/src/setup.rs`: setup exit 0 → task `running` and an agent session launched; exit 3 → `setup-failed` with the last 20 lines; retry runs it again; skip launches the agent; cancel → `setup-failed` "cancelled"; a task left in `preparing-workspace` with no live setup at startup → `setup-failed` "interrupted"; checkout tasks never run the setup nor share entries

### Implementation

- [ ] T033 [P] [US3] Implement `share_entries(checkout, worktree, &[SharedEntry]) -> Vec<ShareNote>` in `crates/delilah-git/src/share.rs` (copy with `cp -cR`, link with an absolute symlink) and export it from `crates/delilah-git/src/lib.rs`
- [ ] T034 [US3] Add `TaskStatus::SetupFailed { reason }` (`setup-failed`) and the events `SetupFailed`, `RetrySetup`, `SkipSetup` with their transitions in `crates/delilah-core/src/task.rs`
- [ ] T035 [P] [US3] Add `SessionKind::Setup` (read-only, no input forwarded) in `crates/delilah-daemon/src/store_sessions.rs` and handle it where kinds are matched (`session_api.rs`, `sessions.rs`)
- [ ] T036 [US3] Implement `crates/delilah-daemon/src/setup.rs`: start a setup session (`$SHELL -lc <setup>`, worktree cwd, `agent_env()`, log in the task dir), 10-minute timeout, exit handling via `exit_code()`, `SetupFailed` with the output tail, and `cancel`, `retry`, `skip`; on success or skip apply `WorkspaceReady` and call `launch_agent`
- [ ] T037 [US3] In `crates/delilah-daemon/src/run.rs`, for worktrees only: call `share_entries` (notes to the task's log), then `setup::start` when `settings.setup` is set and return `201` with the task in `preparing-workspace`; otherwise `launch_agent` as before
- [ ] T038 [US3] Route `POST /api/v1/tasks/{key}/setup/retry|skip|cancel` in `crates/delilah-daemon/src/api.rs` (409 from the wrong status)
- [ ] T039 [US3] At daemon start, mark tasks stuck in `PreparingWorkspace` with no live setup as `SetupFailed("interrupted")`, where sessions are restored (`crates/delilah-daemon/src/main.rs` or `sessions.rs`)
- [ ] T040 [US3] Let archive and delete in `crates/delilah-daemon/src/actions.rs` treat `SetupFailed` like any task with a worktree (refusal during `PreparingWorkspace` stays)
- [ ] T041 [P] [US3] Add `delilah setup retry|skip|cancel <KEY>` to `crates/delilah-cli/src/main.rs`
- [ ] T042 [P] [US3] Add the `setup-failed` status, its label, and `api.setupRetry/Skip/Cancel` to `app/src/api.ts` and `app/src/status.ts`
- [ ] T043 [US3] In `app/src/components/TaskScreen.tsx`: show the Setup session tab with "Cancelar setup" while preparing; on `setup-failed`, a banner with the reason and "Reintentar setup" / "Lanzar sin setup"; with tests in `app/src/components/TaskScreen.test.tsx` or a small extracted `SetupBanner.tsx` + test
- [ ] T044 [P] [US3] Texts in `app/src/i18n/en.ts`; banner styles in `app/src/styles.css`

**Checkpoint**: P1 stories complete; worktree tasks start ready to work in.

---

## Phase 6: User Story 4 — Submodules the project always needs (P2)

**Goal**: "always" submodules are added to every task; "never" ones are not offered, suggested or accepted.

**Independent Test**: on the fixture parent set always `a`, never `c`; start selecting `b`; the workspace has `a` and `b`, not `c`.

### Tests first

- [ ] T045 [P] [US4] Write failing daemon tests in `crates/delilah-daemon/src/run.rs` and `crates/delilah-daemon/src/projects.rs`: always added to the request's selection; a request naming a never submodule → 422 naming it; a rule naming a submodule the parent no longer has → 422 pointing at the settings; `GET …/submodules` and `POST …/suggest-submodules` omit never
- [ ] T046 [P] [US4] Write failing tests for `app/src/components/NewTaskDialog.tsx`: always submodules checked and disabled; never ones not listed

### Implementation

- [ ] T047 [US4] Merge always/never into `WorkspaceSpec.submodules` and validate in `crates/delilah-daemon/src/run.rs`
- [ ] T048 [US4] Filter never out of `submodules` and `suggest_submodules` in `crates/delilah-daemon/src/projects.rs`
- [ ] T049 [US4] Preselect and lock always, hide never in `app/src/components/NewTaskDialog.tsx`

---

## Phase 7: User Story 5 — The project's default workspace mode (P2)

**Goal**: tasks default to the project's mode; a per-task choice still wins.

**Independent Test**: set worktree as default; open "new task": worktree is preselected; start without changing it: the task gets a worktree.

### Tests first

- [ ] T050 [P] [US5] Write failing run tests in `crates/delilah-daemon/src/run.rs`: no `in_checkout` in the request and none saved → the project's mode; request or backlog choice wins; no settings → checkout
- [ ] T051 [P] [US5] Write a failing test that `app/src/components/NewTaskDialog.tsx` preselects the project's mode in WherePicker

### Implementation

- [ ] T052 [US5] Default `in_checkout` from `settings.workspace` in `crates/delilah-daemon/src/run.rs` (after the request and the backlog choice)
- [ ] T053 [US5] Initialise `where` from `project.settings.workspace` in `app/src/components/NewTaskDialog.tsx`

---

## Phase 8: Polish & Cross-Cutting

- [ ] T054 [P] Document project settings for users in `README.md` (settings screen, CLI, setup behaviour)
- [ ] T055 [P] Update `docs/status.md` (built), `docs/architecture.md` (settings column, `setup.rs`, `share.rs`, `SetupFailed`, `SessionKind::Setup`) and `docs/vision.md` (delilah.toml → project settings; PR options still pending)
- [ ] T056 Run `cargo test --workspace`, `cargo clippy --workspace`, `cd app && npx tsc --noEmit && npx vitest run`
- [ ] T057 Walk through `specs/004-project-config/quickstart.md` in a preview (`scripts/preview.sh --fresh`)

---

## Dependencies & Execution Order

- **Setup (T001)** → **Foundational (T002–T011)** → stories.
- **US1** (settings screen) needs only Foundational. It is the MVP: settings can be set.
- **US2, US4, US5** need Foundational; they read settings that US1 lets you set (the API/CLI also work for tests without the screen). US2, US4 and US5 all touch `run.rs` and `NewTaskDialog.tsx`, so they run one after another, not in parallel.
- **US3** needs Foundational (T007 exit code, T011 `launch_agent`); it touches `run.rs` too, so it runs after US2 or before it, not alongside.
- **Polish** after all stories.

Suggested order: Foundational → US1 → US2 → US3 → US4 → US5 → Polish.

## Parallel Opportunities

- Foundational tests T002–T005 are in four files: all in parallel. T007 and T010 alongside T006.
- US1: T012, T013, T014 in parallel; then the daemon (T015–T016), CLI (T017) and app (T018–T021) tracks in parallel.
- US2: T022–T025 in parallel; T026 and T027 in parallel.
- US3: T030, T031, T032 in parallel; T033, T035, T041, T042, T044 in parallel with the daemon core (T034 → T036 → T037–T040).

## Implementation Strategy

1. MVP: Setup + Foundational + US1. Settings are visible and editable; behaviour is unchanged.
2. US2 and US3 (the rest of P1): the settings start doing something. Try it in a preview after each.
3. US4, US5 (P2), then Polish.
