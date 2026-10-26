# Feature Specification: Configurable task start

**Feature Branch**: `011-configurable-task-start`

**Created**: 2026-10-26

**Status**: Draft

**Input**: User description: "LIBRE-25 — Right now we always run feature in every task,
sometimes that is not needed. Also some users might find it useful to be able to configure
what to run when a task is started." Context gathered before specifying: every task starts
its agent with `/feature <ticket or title>` (decision 15) unless the user types a first
message in the new-task form's advanced options; that typed message is not kept on a backlog
task, and the saving-mode note is appended to it anyway, as if it were the spec workflow.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - A project starts its tasks with its own command (Priority: P1)

The user has a project where most tasks are small fixes. In the project's settings they
change "Al empezar una tarea" from the spec workflow (`/feature`) to a custom first message,
`/fix {ticket}`, and save. From then on, every task started in that project begins its agent
with `/fix PROJ-12` (the ticket filled in), followed as today by the pointer to the task's
notes. Tasks in other projects still start with `/feature`.

**Why this priority**: it is the core of the request — not every task needs `/feature`, and
choosing it once per project saves retyping a first message on every task.

**Independent Test**: set a project's start to a custom message with `{ticket}` and
`{title}`; start a task there; the agent's first message is the template with both filled
in, and no saving-mode or autopilot note is added to it.

**Acceptance Scenarios**:

1. **Given** a project whose start is the custom message `/fix {ticket}`, **When** the user
   starts the Jira task PROJ-12 there, **Then** the agent's first message is `/fix PROJ-12`
   plus the notes pointer when the task has notes.
2. **Given** a project whose start is `Revisa {ticket}: {title}`, **When** the user starts a
   free task "Arreglar el login", **Then** `{ticket}` becomes its `LIBRE-n` key and `{title}`
   becomes "Arreglar el login".
3. **Given** a project with no start setting saved (every existing project), **When** a task
   is started, **Then** it starts with `/feature …` exactly as today.
4. **Given** the settings screen, **When** the user picks a custom message and leaves it
   blank, **Then** saving is refused with the problem shown next to the field.

---

### User Story 2 - Start a task without any first message (Priority: P2)

For a chore or an exploration the user wants the agent open in the task's workspace and to
type the first message themselves. They choose "Nada" as the start (for the project, or for
this one task), start the task, and the agent opens idle, waiting for them, in chat or
terminal view as chosen.

**Why this priority**: the simplest escape from `/feature` and the one that needs no
template; useful but less common than a project-wide command.

**Independent Test**: start a task with the start set to "Nada"; the agent session is
running in the task's workspace and has received no message; the user types one and the
agent answers.

**Acceptance Scenarios**:

1. **Given** start "Nada", **When** the task starts, **Then** the agent opens with no first
   message and the task shows as waiting for the user, not as working.
2. **Given** start "Nada" and the task has notes, **When** the task starts, **Then** nothing
   is sent; the notes stay in the task's notes file as always.

---

### User Story 3 - See and override the start for one task (Priority: P2)

When creating or starting a task, the form shows what will run at start — the project's
choice — and lets the user change it for this task only: spec workflow, a custom message, or
nothing. A task saved to the backlog keeps that choice, so starting it later (from the
backlog, the CLI or autopilot-less "Empezar") runs what was chosen, not the project's current
default.

**Why this priority**: the project default covers most tasks; the per-task override (which
exists today only as a hidden "first message" field that the backlog forgets) covers the
rest.

**Independent Test**: in a project whose start is `/feature`, create a backlog task with a
custom message "Investiga {title}"; start it from the backlog; the agent gets "Investiga
<title>". Change the project's start afterwards; a task already in the backlog is unaffected.

**Acceptance Scenarios**:

1. **Given** the new-task form for a project whose start is `/fix {ticket}`, **When** it
   opens, **Then** it shows that the task will start with `/fix {ticket}` (filled in as far as
   the form knows the ticket and title).
2. **Given** the form, **When** the user switches this task to "Nada" and starts it, **Then**
   the task starts idle and the project's setting is unchanged.
3. **Given** a backlog task saved with a custom message, **When** it is started later,
   **Then** that message is used, and the backlog task's view shows it before starting.
4. **Given** the CLI, **When** the user runs `delilah run` / `delilah new` with a prompt,
   **Then** it overrides the project's start as today; with no prompt it follows the task's
   or the project's choice.

---

### User Story 4 - Saving mode and autopilot follow the start (Priority: P3)

Saving mode (one session per phase) and autopilot only make sense when the task follows the
spec workflow. When a task's start is a custom message or nothing, the form does not offer
saving mode or autopilot for it, and the daemon does not add the saving-mode or autopilot
note to its first message nor hand it off between phases.

**Why this priority**: keeps a custom start from carrying instructions ("stop once tasks.md
is written") that do not apply to it — today a typed first message still gets that note.

**Independent Test**: with the saving-mode preference on, start a task with a custom message;
its first message has no saving-mode note and the task has a single main session.

**Acceptance Scenarios**:

1. **Given** the saving-mode preference is on, **When** a task starts with a custom message
   or nothing, **Then** the task runs with saving mode off and its first message carries no
   note.
2. **Given** a request that asks for autopilot on a task whose start is not the spec
   workflow, **When** it is sent, **Then** it is refused as today (`not-spec-workflow`).
3. **Given** the form with a non-spec start chosen, **When** it is shown, **Then** the
   saving-mode and autopilot options are hidden or disabled with a short reason.

### Edge Cases

- A template with an unknown placeholder (`{foo}`) is refused on save, naming the
  placeholder; literal text and slash commands of any name are allowed.
- A template without placeholders is valid (the same message for every task).
- A project session (`@project`) and "create task from idea" are not task starts; they keep
  their own first message.
- Resuming a task, retrying setup, or reopening a session never re-sends the start message;
  only the first session of a task gets it, as today.
- A task without a project starts with the spec workflow unless the request or the task says
  otherwise.
- Existing tasks stored with workflow `prompt` but no stored message keep working: started
  from the backlog with no message given, they start idle rather than failing.
- The setting is per project and lives in Delilah, not in the project's repository.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: Each project MUST have a start setting with three choices: spec workflow
  (`/feature`), custom message (a template), or nothing. Absent, it MUST behave as the spec
  workflow.
- **FR-002**: Custom-message templates MUST support `{ticket}` (the task key, Jira or
  `LIBRE-n`) and `{title}` (the task title) placeholders; any other `{…}` placeholder and a
  blank template MUST be refused when saving, with the error tied to the field.
- **FR-003**: When a task starts, its first message MUST be chosen in this order: the
  message given in the start request, then the choice stored on the task, then the project's
  start setting, then the spec workflow.
- **FR-004**: With the spec workflow, the first message MUST be exactly today's
  (`/feature <ticket>` or `/feature <title>` for free tasks, plus the notes pointer and the
  saving-mode/autopilot note when they apply).
- **FR-005**: With a custom message, the first message MUST be the template with placeholders
  filled in, plus the notes pointer when the task has notes; no saving-mode or autopilot note.
- **FR-006**: With "nothing", the agent MUST be started in the task's workspace without a
  first message, in the chosen view, and the task MUST show as waiting for the user.
- **FR-007**: Saving mode and autopilot MUST only apply to tasks on the spec workflow; a
  task with another start MUST run with saving mode off regardless of the global preference,
  and autopilot requests for it MUST keep being refused.
- **FR-008**: The new-task form MUST show the start the task will get (from the project) and
  let the user change it for this task among the three choices, with the custom message
  editable.
- **FR-009**: A task saved to the backlog MUST keep its start choice and custom message, and
  the backlog task view MUST show them and let them be edited before starting.
- **FR-010**: The project settings screen MUST let the user view and edit the start setting
  alongside the other project settings, with the same save/validation behaviour.
- **FR-011**: The CLI MUST keep `--prompt` as a per-task override and MUST be able to show
  a project's start setting.
- **FR-012**: Existing projects and tasks MUST keep working unchanged without any manual
  migration step: no setting means the spec workflow.
- **FR-013**: Documentation (usage, status, decisions) MUST describe the setting and amend
  the "always `/feature`" decision.

### Key Entities

- **Project start setting**: part of a project's settings; a kind (spec workflow, custom
  message, nothing) and, for custom message, the template text.
- **Task start**: what a given task will run at start; the same kind plus the message, stored
  on the task when it is created or saved to the backlog, and decided once when it starts.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: A user can change a project's start and see it applied to the next task in
  under 1 minute, without editing any file.
- **SC-002**: 100% of tasks in projects with no start setting start with the same first
  message as before this feature.
- **SC-003**: 0 tasks with a custom or empty start receive the saving-mode or autopilot note.
- **SC-004**: A task saved to the backlog with a custom start runs that start when launched,
  in 100% of launches from the UI and the CLI.

## Assumptions

- Only per-project defaults and per-task overrides are in scope; a global default across
  projects is not needed now (projects without the setting fall back to the spec workflow).
- Placeholders are limited to `{ticket}` and `{title}`; the notes are referenced by the
  existing notes pointer, not by a placeholder.
- Picking among several named, reusable workflows (a library of starts) is out of scope; one
  setting per project is enough.
- Only the Claude Code adapter exists; "nothing" means starting Claude interactively without
  an initial prompt.
- UI strings follow the app's existing Spanish-first i18n.
