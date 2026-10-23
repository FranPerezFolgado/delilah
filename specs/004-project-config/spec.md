# Feature Specification: Per-project configuration

**Feature Branch**: `feature/LIBRE-2`

**Created**: 2026-10-20

**Status**: Draft

**Input**: User description: "Per-project config that Delilah applies when it prepares a
task's workspace: the default workspace mode, the branch template, shared files and
folders for worktrees, a setup script run before the agent starts, and submodules to
always include or exclude. PR options are out of scope for now." Refined with the user:
the config is edited from a project settings screen and kept by Delilah, not in a
`delilah.toml` inside the repo (decision 50).

## Clarifications

### Session 2026-10-20

- Q: Does the setup command also run for tasks in the checkout? → A: No, only when a
  worktree is created.
- Q: What happens to the task when the setup fails? → A: It stops with the error and
  keeps its worktree, with "Reintentar setup" and "Lanzar sin setup".

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Configure a project from its settings screen (Priority: P1)

The user opens a project in Projects and goes to its settings. They see every setting
with its current value (or the default), change what they need, and save. Wrong values
are pointed out before saving, next to the field. Nothing is written into the project's
repo.

**Why this priority**: it is how every other story gets configured; without it the
settings cannot be set from the app.

**Independent Test**: open a registered project's settings; set a branch template and a
shared `.env`; save; reopen the screen and see the values; the project's checkout has no
new or changed files.

**Acceptance Scenarios**:

1. **Given** a project with no settings, **When** the user opens its settings, **Then**
   every field shows today's behaviour as its default (workspace unset, which means a
   worktree; no branch template, so Delilah proposes the name; no shared entries, no setup, no submodule rules).
2. **Given** the settings screen, **When** the user saves valid changes, **Then** they
   apply to the next task started for that project, and the screen confirms the save.
3. **Given** an invalid value (a shared path outside the project, a branch template
   without `{ticket}`, a submodule the parent does not have), **When** the user tries to
   save, **Then** the field shows what is wrong and nothing is saved.
4. **Given** a repo project, **When** the user opens its settings, **Then** the
   submodule rules are not shown.
5. **Given** the CLI, **When** the user runs `delilah project show <name>` or sets a
   value, **Then** they see or change the same settings as in the screen.
6. **Given** any save, **When** the user checks the project's checkout, **Then** git
   shows no changes made by Delilah.

---

### User Story 2 - Branch names follow the project's convention (Priority: P1)

The project's settings have the branch template `feature/{ticket}-{slug}`. When the user
starts a task for PROJ-1243 "Paginación por cursor en GET /invoices", Delilah creates the
branch `feature/PROJ-1243-paginacion-por-cursor-en-get-invoices` instead of
`feature/PROJ-1243`.

**Why this priority**: today every branch is `feature/<key>`, which does not match many
teams' conventions; the user renames branches by hand before opening PRs.

**Independent Test**: set a branch template on a project; start a task; the workspace is
on the branch the template gives.

**Acceptance Scenarios**:

1. **Given** the template `feature/{ticket}-{slug}`, **When** a task for PROJ-1243
   starts, **Then** its branch is `feature/PROJ-1243-<slug of the title>`.
2. **Given** no template set, **When** a task starts, **Then** its branch is Delilah's
   proposed name (as before this feature; no template is the default, see FR-007).
3. **Given** a task started with an explicit branch, **When** it starts, **Then** the
   explicit branch wins over the template.
4. **Given** a parent project, **When** a task starts, **Then** every selected submodule
   is on the branch the template gives.
5. **Given** the new-task form, **When** the user fills in the title, **Then** it shows
   the branch name the task will get.

---

### User Story 3 - Worktrees are ready to work in (Priority: P1)

A project needs a `.env` file and installed dependencies before the agent can run its
tests. Its settings list `.env` and `node_modules` as shared, and a setup command. When a
worktree task starts, Delilah brings the shared entries into the worktree, runs the setup
command, and only then launches the agent.

**Why this priority**: today a worktree task starts without the untracked files the
project needs, so the agent fails its first build or test, or spends its time installing.

**Independent Test**: share `.env` and set a setup command that writes a marker file;
start a worktree task; the worktree has `.env` and the marker before the agent's first
prompt.

**Acceptance Scenarios**:

1. **Given** `.env` shared by copy and present in the checkout, **When** a worktree task
   starts, **Then** the worktree has `.env` with the same content.
2. **Given** an entry shared by link, **When** a worktree task starts, **Then** the
   worktree's entry points at the checkout's, and changes in one show in the other.
3. **Given** a setup command, **When** the workspace is ready, **Then** the command runs
   in the workspace before the agent launches, and its output is kept with the task's
   logs.
4. **Given** a setup command that fails, **When** a worktree task starts, **Then** the
   agent is not launched, the worktree is kept, and the task shows that setup failed
   with the end of the output and two actions: "Reintentar setup" and "Lanzar sin
   setup".
5. **Given** a task stopped by a failed setup, **When** the user fixes the worktree by
   hand and chooses "Lanzar sin setup", **Then** the agent launches in that worktree.
6. **Given** a setup command, **When** a task starts in the checkout, **Then** the
   setup does not run.
7. **Given** a shared entry missing from the checkout, **When** a task starts, **Then**
   the task starts anyway and the task's log notes the missing entry.

---

### User Story 4 - Submodules the project always needs (Priority: P2)

In a parent project, every change needs the `shared-config` submodule, and `legacy` must
never be checked out. The settings list them. When the user starts a task,
`shared-config` is always included and `legacy` is never offered or included.

**Why this priority**: it saves a repeated manual step, but only on parent projects.

**Independent Test**: on the fixture parent set always `a` and never `c`; start a task
selecting only `b`; the workspace has `a` and `b`, not `c`.

**Acceptance Scenarios**:

1. **Given** "always" submodules, **When** a task starts with any selection, **Then** the
   workspace includes them, and the new-task form shows them selected and locked.
2. **Given** "never" submodules, **When** the user picks submodules or asks for
   suggestions, **Then** those are not offered nor suggested, and a request that names
   one is refused with a message that names it.
3. **Given** a submodule in a rule that the parent no longer has (removed after saving),
   **When** a task starts, **Then** the start is refused with a message naming it and
   pointing to the project's settings.

---

### User Story 5 - The project's default workspace mode (Priority: P2)

A project whose checkout the user keeps working in has "worktree" as its default. New
tasks for it default to a worktree; the user can still choose the checkout for one task.

**Why this priority**: it removes one choice per task, but the choice already exists.

**Independent Test**: set the default to worktree; open "new task" for the project; the
worktree option is preselected; start without changing it; the task gets a worktree.

**Acceptance Scenarios**:

1. **Given** the default "worktree", **When** the user starts a task without choosing,
   **Then** the task runs in a worktree.
2. **Given** the same project, **When** the user chooses the checkout for one task,
   **Then** that task runs in the checkout.
3. **Given** no default set (unset, `null`), **When** a task starts without choosing
   (the CLI without a flag, an idea proposal, "Crear y lanzar en autopiloto"), **Then**
   it runs in a worktree, as before this feature. The new-task form still preselects the
   checkout, as before, when the project has no default.

---

### Edge Cases

- **Project with no settings**: everything behaves as today.
- **Settings changed while tasks run**: running tasks are not affected; the next start
  uses the new values.
- **Backlog task started later**: it uses the settings at the time it starts, except
  what was chosen for it when it was saved (its workspace mode).
- **Template gives an invalid branch name** (spaces, `..`, empty slug): the slug is
  cleaned to lower-case letters, digits and hyphens; a name git still refuses stops the
  start with a message showing the name.
- **Branch from the template already exists**: the same behaviour as today for an
  existing `feature/<key>` branch.
- **Long titles**: the slug is cut to 40 characters at a word boundary.
- **Free tasks** (`LIBRE-n`): `{ticket}` is the free task's key.
- **Checkout mode**: nothing is copied or linked and the setup does not run; the
  checkout already has what it needs.
- **Failed setup, then archive or delete**: the kept worktree is released like any
  other worktree (same unpushed-work check).
- **Setup command that hangs**: stopped after 10 minutes and treated as failed.
- **Workspace reused** (resumed session, restarted agent): the setup does not run again;
  it runs only when a workspace is created.
- **Project renamed or its path changed**: the settings stay with the project.
- **Project removed**: its settings are removed with it.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: Each project MUST have settings kept by Delilah, never written into the
  project's repo or checkout.
- **FR-002**: A project with no settings saved MUST start tasks exactly as today. In
  particular `workspace` is optional (`null`): when neither the request, the backlog
  task nor the project chooses, the task runs in a worktree. The new-task form keeps
  preselecting the checkout for a project with no default.
- **FR-003**: The app MUST offer a settings screen per project, reached from Projects,
  showing every setting with its current value or default.
- **FR-004**: The settings screen MUST validate before saving and show each problem next
  to its field; nothing MUST be saved while a value is invalid. Leaving the screen with
  unsaved changes MUST ask whether to discard them.
- **FR-005**: The CLI MUST show a project's settings (`delilah project show <name>`)
  and change each of them (`delilah project set <name> <setting> <value>`, and `unset`
  to go back to the default), with the same validation as the screen.
- **FR-006**: The settings MAY include the default workspace mode (checkout or
  worktree; unset means worktree); a choice made for one task MUST win over it.
- **FR-007**: The settings MAY include a branch template with the placeholders
  `{ticket}` (required; the task key) and `{slug}` (optional; Claude's short proposal,
  or the title, lower-case, letters, digits and hyphens, at most 40 characters, when
  Claude gives none); any other placeholder is invalid; an explicit branch for one task
  MUST win over it. *Note (after merging task branch names, spec 004-task-branch-names):*
  the template is optional and `null` by default; with none, the branch is the one
  Delilah proposes (Claude's descriptive name, `feature/` prefix, `-2`/`-3` when taken).
  Precedence: typed branch, then template, then proposal. See decision 49.
- **FR-008**: The settings MUST include shared entries (files or folders relative to the
  project root, never outside it), each copied or linked into a new worktree; linking
  MUST be the default for folders and copying for files. An entry that already exists
  in the new worktree (because git tracks it) MUST be left as it is and noted in the
  task's log.
- **FR-009**: The settings MUST include an optional setup command that runs inside a
  newly created worktree (never in the checkout), after shared entries are in place and
  before the agent launches, with the same clean environment agents get, and a 10-minute
  limit.
- **FR-010**: A setup failure (non-zero exit or timeout) MUST stop the start before the
  agent launches, keep the worktree, show the failure on the task with the end of the
  output, and offer "Reintentar setup" (runs it again) and "Lanzar sin setup" (launches
  the agent as the worktree is). "The end of the output" is its last 20 lines. The
  failure and both actions MUST still be there after the daemon restarts.
- **FR-011**: The setup command's full output MUST be kept with the task's logs.
- **FR-012**: For parent projects, the settings MUST include submodules to always
  include and to never include; "always" is added to every selection and "never" is
  removed from what is offered and suggested, and refused when requested. A submodule
  MUST NOT be in both lists.
- **FR-013**: The new-task form MUST preselect the project's default workspace mode (the checkout when it has none) and
  its "always" submodules, hide its "never" submodules, and show the branch name the
  task will get.
- **FR-014**: The settings MUST leave room for PR options, to be added with the PR
  handling work.
- **FR-015**: Texts MUST be translatable like the rest of Delilah (Spanish in the code,
  English in the translations).
- **FR-016**: While the setup runs, the task MUST show that it is preparing the
  workspace, with the setup's output visible live, and the user MUST be able to cancel
  it (handled like a failure).
- **FR-017**: The API and the CLI MUST report the effective settings of every project,
  defaults included, so a project with no settings saved needs no special case.

### Key Entities

- **Project settings**: per project: default workspace mode, branch template, shared
  entries, setup command, submodules always and never. Kept by Delilah with the project.
- **Shared entry**: a path relative to the project root and how it reaches a worktree
  (copy or link).
- **Setup run**: one execution of the setup command for one worktree: start, end, exit
  status, output.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: The user can configure a project's settings from the app in under 1
  minute, without editing any file.
- **SC-002**: For a project with a branch template, 100% of new tasks are on a branch
  that follows it, with no manual renaming.
- **SC-003**: For a project with shared entries and a setup command, the agent's first
  build or test in a new worktree needs no manual setup.
- **SC-004**: Saving settings never leaves changes in the project's checkout (0 files
  changed by Delilah).
- **SC-005**: Projects without settings see no change in how tasks start.
- **SC-006**: Starting a task with settings but no setup command takes under 1 second
  more than today.

## Assumptions

- The settings are personal to this Delilah install; sharing them with other people
  (for example through a file in the repo) is not needed now (decision 50).
- The setup command is trusted like the rest of the project's code: the user typed it,
  and the agent already runs commands in the project.
- The slug length (40) and the setup limit (10 minutes) are fixed in this feature.
- The project's name, path, kind and workflow keep being edited where they are today;
  the settings screen may show them but does not have to move them.
- PR options come with the PR handling work (`feature/LIBRE-1`).
