# Research: Configurable workflow commands per project

## R1. Where the workflow lives

- **Decision**: a new `workflow` field inside `ProjectSettings` (the JSON `settings` column from
  migration 0016), edited with the rest of the settings by `PUT /api/v1/projects/{name}/settings`.
- **Rationale**: settings already have validation tied to fields (`FieldError`), a screen with
  save/discard, and a JSON column, so no table change for projects. Decision 50 (settings kept by
  Delilah, not in the repo) applies as is.
- **Alternatives**: reuse the top-level `projects.workflow` string — it is one free string
  (always `feature` today) and cannot hold three templates; it is left as a deprecated field
  (R9). A `delilah.toml` in the repo — ruled out by decision 50.

## R2. Shape of the workflow

- **Decision**: `ProjectWorkflow` is a tagged enum:
  - `single { start: Start }` with `Start` = `builtin` | `message { template }` | `nothing`;
  - `phased { plan, implement, review }` (three templates).
  `settings.workflow` absent ⇒ `single { builtin }` (FR-002 for new projects).
- **Rationale**: makes "saving mode/autopilot only when phased" a type question
  (`is_phased()`), and every invalid combination unrepresentable.
- **Alternatives**: a flat struct with optional fields (blank checks everywhere); a list of
  named steps (generic, but no consumer needs more than the three Spec Kit steps).

## R3. Migrating existing projects and tasks

- **Decision**: migration `0022_workflow_commands.sql`:
  - `UPDATE projects SET settings = json_set(settings, '$.workflow', json('<feature preset>'))
    WHERE json_extract(settings, '$.workflow') IS NULL;`
  - `ALTER TABLE tasks ADD COLUMN start_message TEXT;`
  - `UPDATE tasks SET workflow = 'project' WHERE workflow = 'feature';`
    `UPDATE tasks SET workflow = 'prompt' WHERE workflow NOT IN ('project', 'prompt', 'none');`
  The installed state today has only `feature` (16 tasks, 1 project) and `prompt` (19 tasks), so
  the last statement is a safety net; a migration test seeds a pre-upgrade DB with `feature`,
  `prompt` and `bugfix` rows and checks the result.
  Projects added afterwards get no `workflow` key and read as the built-in prompt.
- **Rationale**: SC-002 — every project that exists ran `/feature`; only new ones change.
  Running tasks keep their `saving_mode`/`autopilot` columns untouched (FR-015).
- **Alternatives**: deciding in code at read time "created before date X" — fragile; a
  per-install flag — same effect, harder to see. The SQL is idempotent (`IS NULL`).

## R4. Placeholders and validation

- **Decision**: a small renderer in `delilah-core` (`workflow.rs`): `{ticket}`, `{title}`,
  `{task}` everywhere, `{spec_dir}` only in `phased.implement`; `{{`/`}}` are literal braces.
  `validate` returns `FieldError`s with stable codes: `workflow.blank`,
  `workflow.unknown_placeholder`, `workflow.placeholder_not_allowed`, `workflow.unbalanced_brace`;
  field names like `workflow.start.template`, `workflow.implement`.
  `{task}` = the ticket for a Jira task, the title for a free task (any `LIBRE-n` key), which
  makes the preset reproduce `/feature` exactly for both kinds.
- **Rationale**: same validation for project settings and per-task overrides (FR-012) in one
  place; codes localised by the daemon as for branch templates.
- **Note**: today a free task started from the backlog gets `/feature LIBRE-n` while one started
  from the new-task form gets `/feature <title>` (it depends on the request's `free` flag);
  `{task}` uses the title for both, a deliberate small change.
- **Alternatives**: a template engine (handlebars) — far more than four names need.

## R5. Built-in prompt and review message

- **Boundary**: core does not know `Lang`. `feature_preset` and `builtin_start` live in the
  daemon (`lang.rs` owns the texts); core's `resolve` takes the built-in text as an argument.
- **Decision**: built-in start, in the app's language:
  es "Trabaja en la tarea {ticket}: {title}." / en "Work on the task {ticket}: {title}.",
  plus the notes pointer as today. The "Spec Kit (`/feature`)" preset's review is today's
  `/review de la rama…` text; the "Spec Kit (oficial)" preset's review is a plain message
  (no slash command): "Revisa los cambios de la rama de esta tarea…", same instructions.
- **Rationale**: FR-003 (no slash command), and Spec Kit has no review command.

## R6. Presets

- **Decision**: presets are fixed data in the UI (`app/src/workflowPresets.ts`) and, for the
  `/feature` one, in the daemon (`lang.rs`), where a test asserts the migration's SQL text equals
  `feature_preset(Lang::Es/En)` so the two cannot drift:
  - "Prompt integrado": `single { builtin }`.
  - "Nada": `single { nothing }`.
  - "Spec Kit (`/feature`)": `phased { plan: "/feature {task}", implement:
    "/orchestrate {spec_dir}", review: <today's text in the app's language> }`.
  - "Spec Kit (oficial)": `phased { plan: "/speckit-specify {title}", implement:
    "/speckit-implement", review: <plain review message> }` (the skills form Spec Kit installs
    for Claude today; users on the older dotted commands edit the fields).
- **Known limit**: with the official preset the plan session must run specify → plan → tasks;
  the saving-mode note ("stop once `tasks.md` is written") tells it to, but Spec Kit's specify
  alone stops after `spec.md`. Documented in usage; the user can extend the plan template.

## R7. Skill discovery

- **Decision**: `GET /api/v1/projects/{name}/skills` reads, without running anything:
  - user: `~/.claude/skills/*/SKILL.md`, `~/.claude/commands/**/*.md`;
  - project: `<project path>/.claude/skills/*/SKILL.md`, `<project path>/.claude/commands/**/*.md`;
  - plugins: `~/.claude/plugins/installed_plugins.json` entries enabled in
    `~/.claude/settings.json` `enabledPlugins`, under `installPath`: `skills/*/SKILL.md` and
    `commands/**/*.md`, named `<plugin>:<name>` (plugin = id before `@`).
  Name from the frontmatter `name` (skills) or the file stem (commands, subfolders joined with
  `:`); description from the frontmatter `description`, else the first non-empty line. Each
  entry: `{ name, description, source: user|project|plugin, plugin? }`. Unreadable files are
  skipped; the call never fails because of one. Runs in `spawn_blocking`. Reading
  `enabledPlugins` is needed so disabled plugins' skills (which Claude Code would not run) are
  not offered.
- **Rationale**: Claude Code's documented locations; the daemon already knows `HOME` and the
  project path (config.rs). Read-only, nothing stored (Key Entities).
- **Alternatives**: asking Claude Code (`claude` has no list command that works without a
  session); caching (lists are small and the screen is rarely opened).
- "Not found" warning (FR-006): the UI compares the first word of a template starting with `/`
  against the list; no daemon check.

## R8. Start precedence and storage on the task

- **Decision**: core gets `enum TaskWorkflow { Project, Prompt, None }` with one parser that
  reads `feature` as `Project` (and `as_str`); every creator of a task uses it instead of the
  string `"feature"` — `api.rs`, `run.rs`, `actions.rs`, `conversation.rs`, `session_api.rs`,
  `asks.rs`, `pr_loop/mod.rs`, `proposals.rs`, `store.rs` and the test fixtures — so no new row
  is stored as `feature` after the migration. `tasks.workflow` keeps its column with three values: `project` (follow the
  project's workflow), `prompt` (the task's own message in the new `start_message` column),
  `none` (nothing). Requests may still send `feature` (alias of `project`). At start,
  in `run.rs`: request (`workflow` + `prompt`) → stored task (`workflow` + `start_message`) →
  `project` → project's workflow → built-in. A request with `prompt` and no `workflow` means
  `prompt` (today's behaviour). Backlog creation stores `start_message` (fixes the lost message).
  Legacy `prompt` tasks with no stored message start with nothing (spec edge case).
- **Phased or not** is one function, `core::workflow::is_phased(TaskWorkflow, &ProjectSettings)`,
  used by `resolve`, by `autopilot::allowed` and mirrored once in the UI (`isPhased`). It is
  decided once at start: phased ⇔ task workflow `project` and project's workflow `phased`. Not phased ⇒ `saving_mode = false`, `autopilot = None`, no note.
  `autopilot::allowed(key, workflow)` becomes `allowed(key, task_workflow, project_workflow)`;
  every caller (task create, backlog `PATCH`, autopilot endpoint, proposal card) passes the
  project's settings.
- **Hand-offs**: `handoff::step_prompt` reads the project's current `phased` commands; if the
  project is no longer phased, `try_start` returns an error ("el flujo del proyecto ya no es por
  fases"), which the existing hand-off error path stores and shows (spec edge case).
- **"Nothing"**: `first_session` with no prompt; `launch_session_with(..., None, ...)` already
  supports it (project sessions do it); the task's status after launch is the same as after a
  first turn ends (waiting for the user).

## R9. Compatibility of the contract

- `Project.workflow` (top-level string) stays in the API and the table, deprecated and ignored
  by the start; `delilah project add --workflow` is accepted and ignored with a warning. Dropping
  it (column, API field, CLI flag) is a breaking cleanup left for a major release, noted in
  docs/status.md as known debt.
- `Task.workflow` values change from `feature` to `project` in responses. The app and CLI ship
  with the daemon (same release), and requests keep accepting `feature`; a client that compares
  against `"feature"` would stop offering autopilot — only the bundled app did, and it is updated
  in this feature. This is a behaviour change to note in the release (minor bump is the user's
  call).
- New fields are additive: `ProjectSettings.workflow`, `Task.start_message`,
  `CreateTask.prompt`, the skills endpoint.
