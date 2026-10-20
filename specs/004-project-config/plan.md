# Implementation Plan: Per-project configuration

**Branch**: `feature/LIBRE-2` | **Date**: 2026-10-20 | **Spec**: [spec.md](spec.md)

**Input**: Feature specification from `specs/004-project-config/spec.md`

## Summary

Each project gets settings kept by the daemon: a JSON `settings` column on `projects`,
typed as `ProjectSettings` in `delilah-core` with pure validation and branch naming.
The app gains a settings screen per project and the CLI gains `project show/set/unset`.
When a task starts, the daemon fills the workspace mode, branch and submodules from the
settings. In a new worktree it brings in the shared entries (`cp -cR` clones or
symlinks). If there is a setup command, it runs it as a `setup` session in a PTY, with
live output, and launches the agent only after it succeeds. A failed setup leaves the
task in a new non-terminal status, `SetupFailed`, with "Reintentar setup" and "Lanzar
sin setup". Nothing is written into the repo (decision 26). See [research.md](research.md).

## Technical Context

**Language/Version**: Rust 2024 (workspace `crates/*`); TypeScript + React (`app/`)

**Primary Dependencies**: axum, sqlx (SQLite), serde/serde_json, portable-pty, tokio,
clap (CLI); `unicode-normalization` (new, for the slug); React, Vite, vitest + Testing Library

**Storage**: SQLite: `projects.settings` (JSON text), migration `0011_project_settings.sql`

**Testing**: `cargo test -p delilah-core -p delilah-git -p delilah-agents -p delilah-daemon -p delilah-cli`;
`cd app && npx tsc --noEmit && npx vitest run`

**Target Platform**: macOS (daemon + Tauri app); the web UI in a preview for trying it

**Project Type**: desktop app with a local daemon and CLI

**Performance Goals**: no setup → start under 1 s slower than today (SC-006); shared
copies are APFS clones

**Constraints**: nothing written into the project's repo; projects without settings
behave exactly as today; a 10-minute setup limit; the start request must not wait for
the setup

**Scale/Scope**: 1 migration, ~6 Rust modules touched, 2 new; 1 new screen, 3 app
components touched

## Constitution Check

`.specify/memory/constitution.md` is still the unfilled template; the project's
ground rules (`CLAUDE.md`) serve as gates instead:

| Gate | Status |
|---|---|
| Nothing from the work repos; fixtures only | ✅ tests use `make-fixture-parent.sh` and temp repos |
| Agent hooks/settings outside the repo, clean env | ✅ the setup runs with `agent_env()`; nothing is written into the worktree but the shared entries the user chose |
| Try changes only in a preview, never 7420 / pkill | ✅ quickstart uses `scripts/preview.sh` |
| No remote operations without confirming the account | ✅ none |
| Migrations never edited once shipped | ✅ a new `0011` |
| Contracts (`api.rs` ↔ `api.ts`) | ⚠️ changed, additively: `Project.settings`, a new status, new routes ([contracts/api.md](contracts/api.md)); review with contract-reviewer |

Post-design re-check: no change.

## Project Structure

### Documentation (this feature)

```text
specs/004-project-config/
├── spec.md
├── plan.md
├── research.md
├── data-model.md
├── quickstart.md
├── contracts/{api,cli}.md
└── checklists/{requirements,workspace-settings}.md
```

### Source Code

```text
crates/delilah-core/src/project.rs           # ProjectSettings, SharedEntry, validate(), branch_name(), slug()
crates/delilah-core/src/task.rs              # TaskStatus::SetupFailed; events SetupFailed, RetrySetup, SkipSetup
crates/delilah-core/tests/branch_names.json  # shared cases (Rust + TS)
crates/delilah-git/src/share.rs              # new: share_entries(checkout, worktree, &[SharedEntry]) -> report
crates/delilah-agents/src/pty.rs             # keep the exit status: exit_code()
crates/delilah-daemon/migrations/0011_project_settings.sql
crates/delilah-daemon/src/store_projects.rs  # read/write settings
crates/delilah-daemon/src/projects.rs        # PUT settings, submodule checks, never filtered out of list/suggest
crates/delilah-daemon/src/run.rs             # defaults from settings; share; setup or launch; launch path split out
crates/delilah-daemon/src/setup.rs           # new: run the setup session, timeout, cancel, retry/skip, restart recovery
crates/delilah-daemon/src/store_sessions.rs  # SessionKind::Setup
crates/delilah-daemon/src/api.rs             # routes
crates/delilah-cli/src/main.rs               # project show/set/unset, setup retry/skip/cancel
app/src/api.ts                               # ProjectSettings, Project.settings, status, calls
app/src/branchName.ts (+ .test.ts)           # mirror of branch_name for the preview
app/src/components/ProjectSettings.tsx (+ .test.tsx)  # new: the settings screen
app/src/components/ProjectsView.tsx          # "Ajustes" per project → the screen
app/src/components/NewTaskDialog.tsx         # default where, always/never, branch preview
app/src/components/TaskScreen.tsx            # setup-failed banner + actions; setup tab
app/src/status.ts, app/src/i18n/en.ts        # labels
README.md, docs/status.md, docs/architecture.md
```

**Structure Decision**: existing layout. Two new daemon/git modules keep `run.rs` from
growing: `share.rs` (pure file work, tested against temp dirs) and `setup.rs` (the
setup run's lifecycle).

## Design

### 1. Settings (`delilah-core::project`)

- `ProjectSettings { workspace, branch, shared, setup, submodules }` with defaults equal
  to today's behaviour; `Project.settings: ProjectSettings` (`#[serde(default)]`).
- `validate(&self, kind) -> Vec<FieldError>`: the rules in [data-model.md](data-model.md).
  The daemon adds the "submodule exists" check.
- `branch_name(template, key, title) -> String` and `slug(title)`. Cases in
  `tests/branch_names.json`, run by both Rust and vitest.

### 2. Storage and API

- `0011`: `settings TEXT NOT NULL DEFAULT '{}'`. `store_projects` reads it into
  `Project.settings` (a parse error logs and falls back to defaults) and writes it with
  `set_project_settings(name, &settings)`.
- `PUT /api/v1/projects/{name}/settings`: validate (core + submodules) → 422 with
  `fields`, else save, emit `projects`, return the project.
- `GET …/submodules` and `POST …/suggest-submodules` drop `never`.

### 3. Starting a task (`run.rs`)

1. `in_checkout` = request → backlog choice → `settings.workspace == checkout`.
2. `branch` = request → `branch_name(settings.branch, key, title)`.
3. `submodules` = request ∪ `always`; a `never` in the request → 422 naming it; a rule
   naming a submodule the parent no longer has → 422 pointing at the settings.
4. Workspace created as today. In worktree mode only: `share_entries` (report lines go
   to the task's log), and then, if `setup` is set, `setup::start(app, key, ws)`:
   returns `201` with the task in `PreparingWorkspace`.
5. Everything after `WorkspaceReady` today (ticket, notes, session) moves into
   `launch_agent(app, key, ws, …)`, called directly when there is no setup and by
   `setup.rs` when it succeeds or is skipped.

### 4. The setup run (`setup.rs`)

- Creates a `SessionRecord { kind: Setup, label: "Setup", read_only: true }` and spawns
  `$SHELL -lc <setup>` in a PTY in the worktree with `agent_env()`; log in the task dir.
- Waits for exit or 10 minutes (then kills). Exit 0 → close the session,
  `WorkspaceReady`, `launch_agent`. Otherwise → `SetupFailed { reason: last 20 lines }`.
- `cancel` kills the PTY → `SetupFailed("cancelled")`. `retry` → `RetrySetup` and runs
  again in a new session. `skip` → `SkipSetup`, `WorkspaceReady`, `launch_agent`.
- On daemon start, a task in `PreparingWorkspace` whose setup session is not live →
  `SetupFailed("interrupted")`.
- Delete and archive keep refusing during `PreparingWorkspace` (cancel first) and accept
  `SetupFailed` like any task with a worktree.

### 5. App

- **ProjectSettings screen**: from a project row in Projects ("Ajustes"). Fields: default
  place (WherePicker), branch template with a live example, shared entries (path +
  copy/link/auto, add/remove), setup command, and, for parent projects, always/never
  submodule pickers. Save → `PUT`; field errors under each field; a "discard changes?"
  confirm when leaving with unsaved edits.
- **NewTaskDialog**: preselects `where` from `settings.workspace`; `always` checked and
  disabled; `never` not listed; shows "Rama: feature/…" from `branchName()`.
- **TaskScreen**: while preparing with a setup, the Setup session tab is shown with a
  "Cancelar setup" action. On `setup-failed`, a banner with the reason and "Reintentar
  setup" / "Lanzar sin setup".

## Checklist resolution (`checklists/workspace-settings.md`)

| Item | Resolution |
|---|---|
| CHK003, CHK004 | `validate()` rules (§1, data-model) |
| CHK005 | Discard confirm (§5) |
| CHK011 | Left as it is; noted (research: shared entries) |
| CHK015, CHK016, CHK019 | Setup session tab + cancel; banner; last 20 lines (§4, §5) |
| CHK017 | `SetupFailed` stored; restart recovery (§4) |
| CHK026 | [contracts/cli.md](contracts/cli.md) |
| CHK027 | `Project.settings` always full (§2) |

## Testing

- **core**: `validate` (each rule, parent vs repo); `branch_name`/`slug` over the shared
  JSON; the new transitions (allowed and refused).
- **git**: `share_entries` with temp dirs: copy file, link folder, auto modes, existing
  entry left alone, missing source reported, `..` never reached (validated earlier).
- **agents**: `pty` exit code for `exit 0` / `exit 3`.
- **daemon** (in-memory store, like `projects.rs` tests): settings round trip and 422
  fields; migration default `{}` → defaults; run defaults (place, branch, always/never)
  on the fixture parent; setup success → running; failure → `setup-failed` with the
  tail; retry and skip; cancel; restart recovery.
- **cli**: argument parsing for `project set` and `setup`.
- **app**: `branchName.test.ts` over the shared JSON; `ProjectSettings.test.tsx` (field
  errors shown, save payload, parent-only fields, discard prompt); NewTaskDialog
  preselection; `tsc`.

## Complexity Tracking

| Addition | Why | Simpler alternative rejected |
|---|---|---|
| New status `SetupFailed` | Keeps the worktree and both actions across restarts | `Failed` is terminal; `StartAborted` deletes the worktree |
| Setup as a PTY session | Live output, log, cancel and a tab for free | A piped `Command` needs its own streaming and UI |
| TS mirror of `branch_name` | Live preview with no request per keystroke | A preview endpoint |
