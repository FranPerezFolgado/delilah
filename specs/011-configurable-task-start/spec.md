# Feature Specification: Configurable workflow commands per project

**Feature Branch**: `011-configurable-task-start`

**Created**: 2026-10-26

**Status**: Draft

**Input**: User description: "LIBRE-25 — Right now we always run feature in every task,
sometimes that is not needed. Also some users might find it useful to be able to configure
what to run when a task is started." Refined with the user: Delilah is meant to be downloaded
and used by people who do not have the author's `/feature` skill and have other workflows, so
a project is configured to run the skill the user wants. Not only the start: every command
Delilah sends on its own (today `/feature` at start, `/orchestrate <spec dir>` when saving mode
or autopilot moves to implementation, `/review …` at review) becomes a project setting.

## Clarifications

### Session 2026-10-26

- Q: Only the start, or every command Delilah sends by itself? → A: Every step: start/plan,
  implement and review are configured per project; saving mode and autopilot are offered only
  when the phased steps are configured. The author's setup becomes one preset.
- Q: What does a project with nothing configured run at start? → A: A built-in plain prompt
  (no skill): the task's ticket, title and notes, asking the agent to work on it.
- Q: How is the skill picked? → A: From a list of the skills and slash commands Delilah finds
  for the project, or typed freely as a command/message with placeholders.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - A new user's first task works without any skill (Priority: P1)

Someone downloads Delilah, registers a repo and starts a task "Arreglar el login" with some
notes. They have never configured anything and have no custom skills. The agent starts in
the task's workspace with a built-in message that names the task, points to its notes and
asks it to work on it. Nothing refers to a skill they do not have, and nothing tells the
agent to stop after writing a `tasks.md` it will never write.

**Why this priority**: today every such user's first task begins with `/feature`, which
their Claude Code does not know. Without this the app does not work out of the box for
anyone but its author.

**Independent Test**: on a fresh state, register a fixture repo and start a task with
notes; the agent's first message is the built-in prompt with the title, ticket and notes
pointer; it contains no slash command and no saving-mode note.

**Acceptance Scenarios**:

1. **Given** a newly registered project, **When** a free task "Arreglar el login" is started,
   **Then** the first message is the built-in prompt naming `LIBRE-n` and "Arreglar el login"
   in the app's language, plus the notes pointer when there are notes.
2. **Given** a newly registered project with the saving-mode preference on, **When** a task
   starts, **Then** it runs with one main session, no saving-mode note and no phase handoffs,
   because the project has no phased setup.
3. **Given** a project registered before this feature (it ran `/feature` until now),
   **When** the user upgrades, **Then** it keeps starting with `/feature`, handing off to
   `/orchestrate` and `/review` exactly as before: it is migrated to the "Spec Kit (`/feature`)"
   preset, not to the built-in prompt.

---

### User Story 2 - Pick the skill a project starts its tasks with (Priority: P1)

In a project's settings, under "Flujo de trabajo", the user sees what runs at start and
picks a different one: a list shows the skills and slash commands available to Claude Code
in that project (their own, the repo's and installed plugins', with their descriptions);
they pick `/fix` and the start becomes `/fix {ticket}`, which they can still edit. Or they
type any message with `{ticket}` and `{title}`. Or they choose "Nada" to open the agent idle.
Every task started in that project from then on runs that.

**Why this priority**: the core of the request — a user runs their own skill, not the
author's.

**Independent Test**: in a fixture project with a repo skill `fix`, open settings; the list
offers `/fix`; pick it, save; start a task PROJ-12; the agent's first message is `/fix PROJ-12`
plus the notes pointer.

**Acceptance Scenarios**:

1. **Given** skills in the user's Claude Code folder, in the project's repo and in installed
   plugins, **When** the settings list opens, **Then** it shows each by its invocable name and
   description, marked by where it comes from, and the list can be searched.
2. **Given** the start `/fix {ticket}`, **When** Jira task PROJ-12 is started, **Then** the
   first message is `/fix PROJ-12` plus the notes pointer.
3. **Given** the start "Nada", **When** a task is started, **Then** the agent opens in the
   task's workspace with no first message and the task shows as waiting for the user.
4. **Given** a typed start with `{foo}` or a blank custom start, **When** saving, **Then** it
   is refused with the problem shown next to the field.
5. **Given** a start that names a skill Delilah cannot find (e.g. the repo was moved or the
   skill deleted), **When** the settings are shown, **Then** a warning says it was not found,
   but it can still be saved and used (Claude Code is the judge).

---

### User Story 3 - A phased workflow with the user's own skills (Priority: P2)

A user who works in phases (spec → implement → review) configures the project's workflow as
phased: one command for the plan step (e.g. `/speckit-specify {title}` or `/feature
{ticket}`), one for implementation (e.g. `/speckit-implement` or `/orchestrate {spec_dir}`)
and one for review (e.g. `/review` or their own message). With that, the project's tasks
offer saving mode and autopilot, and Delilah hands off between phases with the user's
commands instead of the author's. A "Spec Kit (`/feature`)" preset fills in today's commands
in one click; a "Spec Kit (oficial)" preset fills in Spec Kit's own commands.

**Why this priority**: saving mode and autopilot are the app's distinctive features; today
they only work for someone who has `/feature`, `/orchestrate` and `/review`.

**Independent Test**: configure a fixture project as phased with three test commands;
start a task with saving mode on; its first message is the plan command plus the saving-mode
note; once the spec folder has `tasks.md`, the next session starts with the configured
implementation command, `{spec_dir}` filled in.

**Acceptance Scenarios**:

1. **Given** a phased project, **When** a task starts with saving mode on, **Then** the first
   message is the plan command plus the saving-mode note, and the hand-off to implementation
   sends the implementation command with `{spec_dir}` replaced by the task's spec folder.
2. **Given** a phased project, **When** the review step starts, **Then** it sends the review
   command configured, not `/review` unless that is what is configured.
3. **Given** a project whose workflow is not phased (single start), **When** the task form,
   the backlog view or the API is used, **Then** saving mode and autopilot are not offered,
   and an autopilot request is refused (`not-spec-workflow`).
4. **Given** the phased setup, **When** the user leaves the implementation or review command
   blank, **Then** saving is refused: a phased workflow needs all three.
5. **Given** the "Spec Kit (`/feature`)" preset, **When** applied, **Then** the commands are
   exactly the ones Delilah sends today, so the author's workflow does not change.

---

### User Story 4 - See and override the start for one task (Priority: P2)

The new-task form shows what the task will run at start (the project's choice, filled in as
far as the form knows the ticket and title) and lets the user change it for this task only:
the project's workflow, another skill or message, or nothing. A task saved to the backlog
keeps that choice; starting it later runs it, even if the project's setting changed in
between.

**Why this priority**: the project default covers most tasks; small fixes and questions in a
phased project still need a lighter start, which is the original complaint.

**Independent Test**: in a phased project, create a backlog task with start "Investiga
{title}"; change the project's start; start the backlog task; the agent gets "Investiga
<title>" and the task has no saving mode.

**Acceptance Scenarios**:

1. **Given** the new-task form for a project, **When** it opens, **Then** it shows the start
   the task will get, and choosing a skill or message for this task uses the same list as the
   project settings.
2. **Given** a task whose start differs from the project's phased workflow, **When** it is
   created, **Then** it runs as a single-start task (no saving mode, no autopilot).
3. **Given** a backlog task saved with its own start, **When** it is started from the UI or
   the CLI with no override, **Then** that start is used and shown in the backlog view.
4. **Given** the CLI, **When** `delilah run`/`delilah new` is given a prompt, **Then** it
   overrides the project's start as today.

### Edge Cases

- Resuming a task, retrying setup or reopening a session never re-sends the start; only the
  task's first session gets it, as today. A phase hand-off sends that phase's command.
- Project sessions (`@project`) and "create a task from an idea" keep their own first message;
  they are not task starts.
- A task without a project gets the built-in prompt unless the request says otherwise.
- Backlog tasks created before this feature with a typed first message (workflow `prompt`,
  message not kept) start idle when launched with no message, instead of failing.
- When a project's workflow changes while a phased task is running, the task's next
  hand-off uses the project's current commands. A task that has already started never
  switches between phased and single start; if the project is no longer phased when the
  hand-off comes, the task stops handing off and stays in its current session, with a notice
  that the project's workflow changed.
- A literal brace in a message is written doubled (`{{` and `}}`); only `{ticket}`,
  `{title}`, `{task}` and `{spec_dir}` are placeholders.
- Skill discovery failing (unreadable folder, no plugins) leaves the list shorter, never
  blocks the settings screen; free text always works.
- The workflow setting lives in Delilah, not in the project's repository.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: Each project MUST have a workflow setting that is either **single start**
  (one start: the built-in prompt, a custom command/message, or nothing) or **phased** (a
  plan, an implementation and a review command).
- **FR-002**: A project with no workflow setting MUST use the single start with the built-in
  prompt. Projects registered before this feature MUST be migrated automatically to the
  phased "Spec Kit (`/feature`)" preset so their behaviour does not change.
- **FR-003**: The built-in prompt MUST be in the app's language, name the task's ticket and
  title, ask the agent to work on the task, and contain no slash command.
- **FR-004**: Command/message templates MUST support `{ticket}`, `{title}` and `{task}` (the
  ticket for a Jira task, the title for a task without one — what `/feature` gets today); the
  implementation command MUST also support `{spec_dir}`. Any other `{…}` placeholder, or a
  blank command where one is required, MUST be refused on save with the error tied to the
  field.
- **FR-005**: Delilah MUST list the skills and slash commands available to Claude Code for a
  project — from the user's Claude Code configuration, the project's repository and installed
  plugins — with their invocable name (plugin skills as `plugin:skill`), description and
  source. When the same name exists in more than one source, the list shows each with its
  source; the command is the name, as Claude Code would resolve it. Picking one fills in the
  command; the field stays editable and free text is always accepted.
- **FR-006**: A command naming a skill not found MUST show a warning but MUST NOT block
  saving or starting.
- **FR-007**: At task start the first message MUST be chosen in this order: the message in
  the start request, then the start stored on the task, then the project's workflow, then the
  built-in prompt; the notes pointer MUST be appended whenever a message is sent and the task
  has notes. A start request (UI, API or CLI) MUST be able to say "follow the project's
  workflow", "this message" or "nothing".
- **FR-008**: "Nothing" MUST start the agent in the task's workspace without a first message,
  in the chosen view, with the task shown as waiting for the user.
- **FR-009**: Saving mode and autopilot MUST be offered and applied only to tasks that follow
  a phased workflow; any other task MUST run with saving mode off regardless of the global
  preference, receive no saving-mode or autopilot note, and get no phase hand-offs; autopilot
  requests for it MUST be refused as today.
- **FR-010**: Phase hand-offs (implementation and review sessions, including autopilot's)
  MUST send the project's configured commands; no command name MUST be hard-coded in
  Delilah outside the "Spec Kit (`/feature`)" preset.
- **FR-011**: The settings screen MUST offer presets: "Prompt integrado" (default), "Nada",
  "Spec Kit (`/feature`)" (today's `/feature`, `/orchestrate {spec_dir}`, review message)
  and "Spec Kit (oficial)" (Spec Kit's own commands); applying one fills in the fields, which
  stay editable.
- **FR-012**: The new-task form MUST show the start the task will get and let the user
  override it for this task with the same picker and the same validation as the project
  settings; the backlog MUST keep the task's start and show and edit it before starting.
- **FR-013**: The CLI MUST keep `--prompt` as a per-task override and MUST show a project's
  workflow.
- **FR-015**: Tasks created before this feature MUST keep working: tasks on the `feature`
  workflow follow their project's workflow (the migrated preset), tasks with a typed first
  message keep it if it was stored and start idle otherwise, and running tasks keep their
  saving mode and autopilot.
- **FR-014**: Documentation (usage, status, decisions, README for newcomers) MUST describe
  the setting and amend decision 15 ("always `/feature`") and decision 46 ("autopilot means
  the `feature` workflow").

### Key Entities

- **Project workflow**: part of a project's settings. Kind (single start or phased); for
  single start, the start (built-in, custom template, nothing); for phased, the plan,
  implementation and review templates.
- **Task start**: what a task runs first, stored on the task when created or saved to the
  backlog: follow the project's workflow, a custom template, or nothing.
- **Available skill**: a skill or slash command Claude Code can run in the project — name,
  description, source (user, project, plugin). Read, never stored.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: On a fresh install with no custom skills, the first task of a newly
  registered project starts with a message the agent can act on, with zero configuration.
- **SC-002**: 100% of projects registered before this feature run the same commands at
  start and at every hand-off as before. One deliberate exception: a free task started from the
  backlog gets its title after `/feature`, as one started from the new-task form already does,
  instead of its `LIBRE-n` key.
- **SC-003**: A user can switch a project to one of their own skills, from the list, and see
  it used on the next task in under 1 minute, without editing any file.
- **SC-004**: 0 tasks outside a phased workflow receive the saving-mode or autopilot note or
  a phase hand-off.
- **SC-005**: No Delilah-sent message names a skill the user did not configure or pick from a
  preset.

## Assumptions

- Only per-project workflows and per-task start overrides are in scope; no global default
  across projects (new projects get the built-in prompt).
- Phased workflows keep reading phases from Spec Kit's `specs/` folder (`spec.md`,
  `plan.md`, `tasks.md`): the plan command must produce those files for saving mode and
  autopilot to hand off. Non-Spec Kit phased workflows are out of scope.
- Autopilot's own notes (answer setup questions, approve phases, write the review outcome)
  stay as they are; only the commands become configurable.
- Skill discovery covers Claude Code's standard locations (user and project `skills/` and
  `commands/`, installed plugins); other agents are out of scope (only the Claude Code
  adapter exists).
- A library of reusable named workflows shared across projects is out of scope; presets are
  fixed in the app.
