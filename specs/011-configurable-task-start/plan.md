# Implementation Plan: Configurable workflow commands per project

**Branch**: `011-configurable-task-start` | **Date**: 2026-10-26 | **Spec**: [spec.md](spec.md)
**Input**: Feature specification from `specs/011-configurable-task-start/spec.md`

## Summary

Every command Delilah sends by itself (start, implement hand-off, review hand-off) becomes a
per-project setting, so Delilah works for people without the author's `/feature`,
`/orchestrate` and `/review` skills. A project's workflow is either a **single start** (built-in
prompt — the default for new projects —, a custom template, or nothing) or **phased** (plan,
implement and review templates); saving mode and autopilot exist only for phased tasks.
Existing projects are migrated to a "Spec Kit (`/feature`)" preset that reproduces today's
commands. Settings offer a list of the skills Claude Code has for the project (user, repo,
plugins). Tasks store their own start (follow the project, a message, or nothing), which also
fixes the first message being lost on backlog tasks.

## Technical Context

**Language/Version**: Rust 2021 (workspace), TypeScript + React (Vite) in `app/`
**Primary Dependencies**: axum, sqlx (SQLite), serde, tokio; React, vitest
**Storage**: SQLite — `projects.settings` JSON (0016), `tasks` table; new migration 0022
**Testing**: `cargo test -p delilah-core -p delilah-daemon -p delilah-cli`; `npx vitest`; e2e
with the fake Claude (`crates/delilah-daemon/tests`, spec 005-e2e-tests)
**Target Platform**: macOS (daemon + Tauri app); the change is daemon/UI only, nothing native
**Project Type**: desktop app with a local daemon (HTTP API) and CLI
**Performance Goals**: skills listing < 200 ms for a few hundred skill files (blocking read off
the async threads)
**Constraints**: never write into the project's repo; agents spawned with a clean env and
hooks outside the repo (unchanged); old requests sending `workflow: "feature"` keep working
**Scale/Scope**: ~4 core types, 1 migration, 1 new endpoint, ~6 daemon modules, 4 UI components

## Constitution Check

`.specify/memory/constitution.md` is the unfilled template; the project's rules are in
`CLAUDE.md` and `docs/decisions.md`. Gates applied:

| Gate | Status |
|---|---|
| Settings kept by Delilah, not in the repo (decision 50) | ✅ workflow lives in `projects.settings` |
| Never edit a shipped migration | ✅ new `0024_workflow_commands.sql` |
| API ↔ `app/src/api.ts` contract kept in sync | ✅ contracts/api.md; additive plus an accepted alias |
| Hooks outside the worktree, clean env | ✅ untouched |
| Docs updated with behaviour (status, usage, decisions) | ✅ phase 6 |
| No version bump on the feature branch | ✅ none planned |

## Design

See [research.md](research.md) (R1–R9), [data-model.md](data-model.md),
[contracts/api.md](contracts/api.md), [quickstart.md](quickstart.md).

### Core (`crates/delilah-core`)

- New `src/workflow.rs`: `ProjectWorkflow`, `Start`, `TaskWorkflow` (one parser, `feature` →
  `Project`), `Vars`, `render`, `validate`, `is_phased`, the task-start resolution as a pure function
  `resolve(request, task, project) -> ResolvedStart { prompt: Option<String>, phased: bool,
  stored: (workflow, start_message) }` so precedence is unit-tested without a daemon.
- `project.rs`: `ProjectSettings.workflow: Option<ProjectWorkflow>`; `validate` includes it.
- `task.rs`: `Task.start_message: Option<String>`; `workflow` doc updated (`project|prompt|none`).

Core does not know `Lang`: `feature_preset(Lang)` and `builtin_start(Lang)` live in the daemon
(`lang.rs`), which passes the built-in text to `resolve`.

### Daemon (`crates/delilah-daemon`)

- `migrations/0024_workflow_commands.sql` (R3, data-model §Migration).
- `store.rs` / `store_projects.rs`: read/write `start_message`; settings already round-trip as
  JSON.
- `run.rs` `start_run_with`: replace the `match (&req.prompt, workflow)` block with
  `workflow::resolve`; when not phased force `saving_mode=false`, `autopilot=false` before the
  existing saving-mode/autopilot resolution writes them; `first_session` takes
  `Option<String>` and adds the note only when phased; append the notes pointer only when a
  prompt is sent; launch with `None` for "nothing".
- `autopilot/mod.rs`: `allowed(key, task_workflow, project_settings)` delegating to
  `workflow::is_phased`; update every caller (task create in `api.rs`, backlog `PATCH`, the
  autopilot endpoint, idea proposal card in `proposals.rs`). The autopilot notes
  (`autopilot/notes.rs`) name no skill today; a test keeps it that way (FR-010).
- Every internal task creator stores `TaskWorkflow` instead of `"feature"`: `actions.rs`,
  `conversation.rs`, `session_api.rs`, `asks.rs`, `pr_loop/mod.rs`, `proposals.rs`, `store.rs`
  (and test fixtures).
- `handoff.rs`: `step_prompt(lang, step, dir, &ProjectWorkflow)` renders the project's phased
  `implement`/`review`; `try_start` loads the project and fails with a localised error when the
  project is no longer phased (existing hand-off error path, spec edge case).
- New `skills.rs` + route `GET /api/v1/projects/{name}/skills` in `projects.rs` (R7).
- `projects.rs`: localise the new `workflow.*` validation codes like the branch ones.
- `api.rs`: `CreateTask.prompt` → `start_message`; validate `workflow` values (alias `feature`).

### CLI (`crates/delilah-cli`)

- `run`/`new`: `--no-prompt`; `--workflow` values. `project show` prints the workflow;
  `project unset <name> workflow`; `project add --workflow` deprecation warning. Editing the
  workflow stays in the UI for v1.

### UI (`app/`)

- `api.ts`: `ProjectWorkflow`, `Start`, `Skill`, `Task.start_message`, `api.projectSkills()`;
  `DEFAULT_PROJECT_SETTINGS` without `workflow`.
- New `src/workflowPresets.ts` (presets and `isPhased`). Template errors come from the
  daemon's `FieldError`s on save, as for the branch template; no second validator in the UI.
- New `components/WorkflowEditor.tsx`: kind (single / phased), presets, template fields, and a
  searchable skill picker (`SkillPicker`) fed by `api.projectSkills`, with the "not found"
  warning. Used by `ProjectSettings.tsx` ("Flujo de trabajo" section) and, in single-start
  mode only, by the new-task form and backlog view for the per-task override.
- `NewTaskDialog.tsx`: replace the "Primer mensaje" textarea with "Al empezar" (follow the
  project — showing what it resolves to —, another message via the picker, or nothing);
  `autopilotAllowed` = follows the project and the project is phased; saving-mode toggle hidden
  otherwise; send `workflow`/`prompt` accordingly.
- `BacklogTaskView.tsx`: show and edit the stored start; same autopilot rule.
- i18n: Spanish keys in place, English in `src/i18n/en/` (coverage test).

### Migration

SQL only (data-model §Migration): projects without `$.workflow` get the `/feature` preset with
the review text in `pref:language` (Spanish when unset); tasks `feature` → `project`, values
other than `project|prompt|none` → `prompt` (none exist in the installed state today), new
`start_message` column. Idempotent; no manual step. Tested on a seeded pre-upgrade DB, and the
preset JSON in the SQL is asserted equal to `feature_preset(Lang)`.

`Project.workflow` stays as a deprecated, ignored field; removing it is a breaking cleanup for a
later major release (known debt in docs/status.md).

## Project Structure

### Documentation (this feature)

```text
specs/011-configurable-task-start/
├── spec.md, plan.md, research.md, data-model.md, quickstart.md
├── contracts/api.md
├── checklists/requirements.md, checklists/workflow.md
└── tasks.md            # /speckit-tasks
```

### Source Code (repository root)

```text
crates/delilah-core/src/{workflow.rs (new), project.rs, task.rs, lib.rs}
crates/delilah-daemon/migrations/0024_workflow_commands.sql (new)
crates/delilah-daemon/src/{run.rs, handoff.rs, autopilot/mod.rs, api.rs, projects.rs,
                           skills.rs (new), store.rs, store_projects.rs, lang.rs, actions.rs,
                           conversation.rs, session_api.rs, asks.rs, pr_loop/mod.rs, proposals.rs}
crates/delilah-daemon/tests/  (e2e with the fake Claude)
crates/delilah-cli/src/main.rs
app/src/{api.ts, workflowPresets.ts (new), i18n/en/*}
app/src/components/{WorkflowEditor.tsx (new), SkillPicker.tsx (new), ProjectSettings.tsx,
                    NewTaskDialog.tsx, BacklogTaskView.tsx}
docs/{status.md, usage.md, decisions.md, architecture.md}, README.md
```

**Structure Decision**: existing layout; one new core module (pure logic, unit-tested), one
new daemon module (filesystem reading), two new UI components.

## Phases (for tasks)

1. **Core model**: `workflow.rs` (types, render, validate, resolve) + settings/task fields.
2. **Storage & migration**: 0022, store read/write, migration tests on a pre-upgrade DB.
3. **Start (US1/US2/US4 daemon side)**: `run.rs` resolution, notes, no-prompt launch,
   saving-mode/autopilot gating, `api.rs` create/PATCH, autopilot `allowed`.
4. **Hand-offs (US3)**: `handoff.rs` with project commands + "no longer phased" error.
5. **Skills endpoint (US2)**: `skills.rs` + route.
6. **CLI**.
7. **UI**: api.ts, presets, WorkflowEditor/SkillPicker, settings, new-task form, backlog.
8. **Docs**: decisions (amend 15 and 46, new decision for workflow commands), usage, status,
   architecture, README.

Phases 4, 5, 6 depend on 1–3 only and touch separate files; 7 depends on the contract only and
can run in parallel with 4–6.

## Complexity Tracking

| Choice | Why | Simpler alternative rejected because |
|---|---|---|
| Tagged enum for the workflow | makes phased-only features a type check | flat optional fields scatter blank checks across run/handoff/UI |
| `{task}` placeholder in addition to `{ticket}`/`{title}` | the only way the preset reproduces `/feature <ticket or title>` | dropping it changes the author's start for free tasks (SC-002) |
| Skills endpoint in the daemon | the UI runs in a webview without filesystem access to `~/.claude` | free text only — the user chose list + free text |
