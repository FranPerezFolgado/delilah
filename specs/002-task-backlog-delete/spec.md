# Feature Specification: Task backlog and deleting tasks

**Feature Branch**: `feature/FEAT-10`

**Created**: 2026-10-20

**Status**: Draft

**Input**: User description: "Borrar tasks y a parte poder crear tasks sin que inicien automáticamente para crearme como un backlog, lo estoy haciendo en notas"

## Context

Today every task created from the app starts right away: Delilah prepares
the workspace and launches the agent. Work Fran wants to do later has
nowhere to live in Delilah, so it ends up in a notes app, away from the
projects and tickets it belongs to. The daemon can already store a task
without starting it, but the app offers no way to do that, and a task stored
like this can't be started later.

There is also no way to get rid of a task: a task created by mistake, a
duplicate or an idea that's been dropped stays in Delilah forever. The only
option is archiving, which releases the workspace of a finished task but keeps
the task.

## Clarifications

### Session 2026-10-20

- Q: Where should the backlog be shown in the app? → A: A folded "Backlog (n)" section under each project in the sidebar, plus a "Backlog" column on the board.
- Q: Can a backlog task be saved without a project? → A: No, a project is required.
- Q: Can a task whose agent is running be deleted directly? → A: Yes, the confirmation warns that the agent will be stopped.
- Q: Should deleting be permanent? → A: Yes, there is no undo; the confirmation is the safeguard.
- (After trying it) Editing a backlog task must also let you change between checkout and worktree.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Save a task for later (Priority: P1)

In "Nueva tarea", Fran fills in the title (a Jira ticket or a free task), the
project and, optionally, some notes, and chooses "Guardar en backlog" instead of
starting it. The task appears in that project's backlog. No workspace is
prepared and no agent is launched.

**Why this priority**: it is the main request. It replaces the list Fran keeps in
notes today.

**Independent Test**: with a preview and a fixture project, create a backlog task
and check that it appears in the backlog, that no branch, worktree or
agent session was created, and that it is still there after restarting the daemon.

**Acceptance Scenarios**:

1. **Given** the "Nueva tarea" dialog with a title and a project, **When** I choose
   "Guardar en backlog", **Then** the dialog closes and the task appears in the
   project's backlog with its title and notes, and nothing is started.
2. **Given** a free task saved to the backlog, **When** I look at it, **Then** it
   already has its own key (LIBRE-n), like a free task that is started.
3. **Given** a Jira ticket from the Jira view, **When** I open "Nueva tarea" from
   it and choose "Guardar en backlog", **Then** the backlog task keeps the
   ticket's key and title.
4. **Given** several backlog tasks, **When** I look at the sidebar, **Then** they
   are in the project's folded "Backlog (n)" section, oldest first, separate
   from the tasks in progress; on the board they are in the "Backlog" column.
5. **Given** the "Nueva tarea" dialog with no project chosen, **When** I look at
   "Guardar en backlog", **Then** it is disabled, as starting is.
6. **Given** the CLI, **When** I create a task without running it, **Then** it
   appears in the app's backlog like the ones created from the dialog.

---

### User Story 2 - Start a backlog task (Priority: P1)

Fran opens a backlog task and chooses "Empezar". The usual start options
(checkout or worktree, submodules, custom prompt) appear, already filled in
with what the task has. Confirming starts it exactly like a task that was started
from "Nueva tarea".

**Why this priority**: a backlog that can't be turned into work is just
another notes app. Without this, Story 1 has no value.

**Independent Test**: start a backlog task in a preview and check that it leaves
the backlog, gets a workspace and an agent, and follows the usual phases.

**Acceptance Scenarios**:

1. **Given** a backlog task, **When** I choose "Empezar" and confirm, **Then** it
   leaves the backlog, its workspace is prepared and the agent starts with the
   task's title and notes as context.
2. **Given** a backlog task, **When** I open "Empezar", **Then** I can still
   change the project, the place (checkout or worktree), the submodules and the
   prompt before confirming.
3. **Given** a backlog task whose start fails (e.g. the checkout is taken by
   another task in progress), **When** the error is shown, **Then** the task
   stays in the backlog unchanged and I can try again.
4. **Given** the CLI, **When** I run an existing backlog task by its key,
   **Then** that task is started instead of failing because the key already exists.

---

### User Story 3 - Delete a task (Priority: P1)

Fran deletes a task they no longer want: from the backlog, a task in progress
or a finished one. After confirming, the task disappears from every view
(sidebar, board, inbox) and its traces in Delilah are removed.

**Why this priority**: it is the user's other explicit request. A backlog fills
up with ideas that get dropped, so it also needs a way to clean up.

**Independent Test**: in a preview, delete a backlog task, a running task and a
finished task, and check that none of them shows up in any view or in the CLI
list, and that the running task's agent stopped.

**Acceptance Scenarios**:

1. **Given** a backlog task, **When** I choose "Eliminar" and confirm, **Then** it
   disappears from the backlog.
2. **Given** a task in progress, **When** I choose "Eliminar", **Then** the
   confirmation says the agent will be stopped; after confirming, the agent
   is stopped, its workspace is released and the task disappears.
3. **Given** a task whose workspace has work that wasn't pushed, **When** I try
   to delete it, **Then** I am warned that that work would be lost and the
   task is deleted only if I choose "Eliminar igualmente".
4. **Given** a task worked in the project's checkout, **When** I delete it,
   **Then** the checkout goes back to the branch it was on before the task, as it
   does on archive, and the task's branch is kept.
5. **Given** a deleted task, **When** I list tasks in the CLI or reload the app,
   **Then** it is no longer there, and its key can be used again for a new task.
6. **Given** the CLI, **When** I delete a task by its key (with an option to
   force it when there is unpushed work), **Then** the result is the same as from
   the app.

### Edge Cases

- Deleting the task that is open in the main view: the view goes back to the
  project (or the board) and shows no error.
- Deleting a task while its agent is waiting for an answer: the pending
  question disappears from the inbox with the task.
- A Jira ticket saved to the backlog and also started from the Jira view
  ("Analizar con Claude"): the existing backlog task is started instead of
  creating a second task with the same key.
- A backlog task whose project has been removed from Delilah: it is still
  shown (under "Sin proyecto") and can be deleted; "Empezar" asks for a
  project before it can be confirmed.
- The daemon restarts while a task is being deleted: on the next start the
  task is either fully there or fully gone, never half deleted (e.g. a task
  without its workspace record but still shown as running).
- Deleting a task whose worktree folder was removed by hand: deletion still
  succeeds.
- Notes with several lines or Markdown: they are kept as written.
- The same backlog task started twice at once (two windows, or the app and the
  CLI): only the first start goes ahead; the second is told the task has
  already been started.
- Deleting a task while its workspace is still being prepared: it is refused
  with a readable message ("se está preparando; inténtalo en unos segundos").
- Deleting a task whose checkout was already taken by a newer task: there is no
  workspace left to release; only the task is removed and the checkout is not touched.
- Deleting a running task with unpushed work: the unpushed-work check happens
  before anything is stopped, so declining the warning leaves the agent running.
- The workspace can't be released when deleting (e.g. git fails): the task is not
  deleted, the error is shown, and the user can retry or force it.
- Deleting the free task with the highest number (e.g. LIBRE-7): the next free
  task does not get LIBRE-7 again, because its branch may still exist.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The "Nueva tarea" dialog MUST offer, next to starting the task, a
  "Guardar en backlog" action that stores the task without preparing any
  workspace or launching any agent.
- **FR-002**: A backlog task MUST have a project and MUST keep its origin
  (Jira key or free LIBRE-n key), title, project and optional free-text notes.
- **FR-003**: The sidebar MUST show the backlog tasks of each project in a
  "Backlog (n)" section, folded by default, separate from the tasks in progress,
  oldest first; the board MUST show them in a "Backlog" column.
- **FR-004**: A backlog task MUST be startable with the same start options as a
  new task, pre-filled from the task; once started, it MUST behave exactly like
  a task started from "Nueva tarea".
- **FR-005**: If starting a backlog task fails, the task MUST stay in the
  backlog unchanged.
- **FR-006**: The title, notes, project and where it will work (the project's
  checkout or a worktree) of a backlog task MUST be editable before it is started,
  and "Empezar" MUST come with that choice made; its key can't change.
- **FR-007**: The notes of a task MUST reach the agent as context when the task
  is started, both with the default workflow and with a custom prompt.
- **FR-007a**: Backlog tasks MUST NOT count as pending work: they stay out of
  the inbox, the badge and the sidebar's urgency order; a project with no
  backlog tasks shows no backlog section, and on the board backlog tasks are always
  in the "Backlog" column, whatever spec they already have.
- **FR-008**: Any task, in any state (backlog, in progress, waiting, paused or
  finished), MUST be deletable from its view and from the CLI, after an explicit
  confirmation; deletion is permanent, with no undo.
- **FR-008a**: "Eliminar" MUST be offered in the task's view and in the context
  menu of its row in the sidebar.
- **FR-009**: Deleting a task with a live agent MUST stop the agent first, but
  only after the unpushed-work check has passed or been forced.
- **FR-010**: Deleting a task with a workspace MUST release it the same way
  archiving does (removing the worktree, or returning the checkout to its
  previous branch), keep the task's git branch, and refuse when there is
  unpushed work unless the user forces it.
- **FR-011**: Deleting a task MUST remove it, its history, its sessions, its
  pending questions and the files Delilah keeps for it, so that it appears
  nowhere and its Jira key can be used again; a delete is either complete or
  doesn't happen (after a restart the task is either fully there or fully gone).
- **FR-011a**: Free-task numbers MUST NOT repeat: a new free task never gets the
  number of a deleted one.
- **FR-012**: Deleting a task MUST NOT change anything outside Delilah (no
  change to the Jira ticket; no change to the remote branch).
- **FR-013**: Every open view MUST reflect a created, started or deleted task
  without a manual reload.
- **FR-014**: The CLI MUST let the user create a backlog task, start an
  existing backlog task by key, and delete a task by key (with a force option);
  the list shows backlog tasks with their own state; a delete refused because
  of unpushed work exits with an error that names the force option.

### Key Entities

- **Task**: gains **notes** (optional free text) and the **backlog** state: saved,
  never started, with no workspace and no agent session. It is not the
  short-lived state a task goes through while it is being started; a task leaves
  the backlog only by being started or deleted, and never goes back to it.
- **Backlog**: the backlog tasks of a project, oldest first; every backlog task
  belongs to exactly one project.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: Saving an idea to the backlog takes the same steps as starting a
  task, or fewer (title, project, one click).
- **SC-002**: 0 workspaces, branches or agent sessions are created for a task
  that is only saved to the backlog.
- **SC-003**: Starting a backlog task takes one action plus the usual
  confirmation, and the started task can't be told apart from one started directly.
- **SC-004**: After deleting a task, it appears in 0 views (sidebar, board,
  inbox, CLI list), including after restarting the daemon.
- **SC-005**: No unpushed work is lost on delete without the user explicitly
  choosing to force it.
- **SC-006**: The existing flows (start a task, archive, done, resume) keep
  working with no regressions.

## Assumptions

- Fran is the only user: there are no permissions; anyone using the app can
  delete any task.
- The task's local git branch is kept on delete (it may hold work); deleting
  it is left to the user, as on archive.
- Where it works (checkout or worktree) is saved with the backlog task and can be
  changed until it starts; submodules and the prompt are chosen when it starts.
- Reordering, prioritising, tagging or sorting the backlog manually is out of
  scope; it is listed oldest first.
- Importing the existing list from the notes app is out of scope; tasks are
  entered one by one.
- Tasks already created without running them (`delilah new`) before this
  feature become backlog tasks.
- Archiving stays as it is: it releases the workspace of a finished task and
  keeps the task; deleting removes the task too.
