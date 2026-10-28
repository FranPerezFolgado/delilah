# Feature Specification: Jira sub-tasks become Delilah subtasks

**Feature Branch**: `013-jira-subtasks-import`

**Created**: 2026-10-28

**Status**: Draft

**Input**: User description: "From a Jira ticket that has sub-tasks, create a Delilah parent task for the ticket and one Delilah subtask per Jira sub-task, keyed by the Jira key, in the same project. Builds on spec 012-parent-subtasks (decision 102) and the Jira view's sub-task listing. Open points: Jira sub-tasks that are done or assigned to others; keeping both sides in sync when a sub-task is added in Jira later. Update docs/status.md and docs/usage.md when it lands."

## Context

Spec `012-parent-subtasks` added parent tasks and subtasks (the integration branch
pattern), but only by hand: the user creates the parent task and types each subtask.
The Jira view already lists a ticket's Jira sub-tasks under it, yet they never become
Delilah tasks. This feature is the Jira adapter for parent tasks: one action turns a
Jira ticket with sub-tasks into a parent task keyed by the ticket and one subtask per
chosen Jira sub-task, keyed by the sub-task's Jira key, and later brings in sub-tasks
added in Jira since.

Everything about how parent tasks and subtasks behave once created (branches, PRs,
progress, archiving) is spec 012's and does not change here.

### Decisions on the open points

- **Done Jira sub-tasks**: shown but not selected by default (marked "done in Jira");
  the user can still tick them. Nothing is created for them unless ticked.
- **Sub-tasks assigned to someone else**: shown with the assignee's name and not
  selected by default; sub-tasks assigned to the user or to nobody are selected. The
  user can change any choice before confirming.
- **Sync**: one-way (Jira → Delilah) and on demand, never in the background. Delilah
  never writes to Jira. When a parent task is linked to a Jira ticket, its screen
  checks the ticket's sub-tasks when opened and offers the ones Delilah does not have
  yet ("2 new Jira sub-tasks · Add"). A Jira sub-task that was left out on purpose can
  be skipped so it stops being offered. Changes in Jira (status, title, removal) never
  change, archive or delete a Delilah task on their own.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Turn a Jira ticket into a parent task with subtasks (Priority: P1)

In the Jira view, a ticket with sub-tasks offers "Create parent task". The user picks
the project and sees the ticket's Jira sub-tasks, each with its key, title, status and
assignee, already ticked or not by the rules above. On confirming, Delilah creates a
parent task keyed and titled as the ticket, and one subtask per ticked sub-task, keyed
and titled as that sub-task, all in the chosen project and in the backlog.

**Why this priority**: It is the feature: without it, every Jira sub-task has to be
typed again by hand.

**Independent Test**: With sample Jira data, pick a ticket with three sub-tasks (one
done, one assigned to someone else, one open and unassigned), create the parent task
with the default selection, and check that the parent task and exactly one subtask
exist with the Jira keys and titles, in the chosen project and in the backlog.

**Acceptance Scenarios**:

1. **Given** a Jira ticket with sub-tasks and no Delilah task with its key, **When**
   the user chooses "Create parent task" and a project, **Then** a list of its Jira
   sub-tasks is shown with key, title, Jira status and assignee, where open sub-tasks
   assigned to the user or to nobody are ticked and the rest are not.
2. **Given** that list, **When** the user confirms, **Then** one parent task (key and
   title of the ticket) and one subtask per ticked sub-task (key and title of the
   sub-task) are created in that project, all in the backlog, with the subtasks under
   the parent task.
3. **Given** that list, **When** the user ticks a done sub-task or one assigned to
   someone else, **Then** it is created like the others.
4. **Given** the created subtasks, **When** one is started, **Then** its agent gets
   that Jira sub-task as its ticket (as for any Jira task today), and the parent
   ticket's key and title as context.
5. **Given** a project that is a parent repository (with submodules), **When** the
   parent task and subtasks are created, **Then** the user can pick the submodules once
   for the parent task, and the subtasks take the same ones.
6. **Given** a Jira ticket with no sub-tasks, **When** the user looks at it, **Then**
   "Create parent task" is not offered (the existing actions stay as they are).

---

### User Story 2 - Reuse tasks that already exist (Priority: P1)

The ticket or some of its sub-tasks may already be Delilah tasks (a normal task
created earlier from the ticket, a sub-task saved to the backlog by hand). Creating
the parent task reuses them instead of failing or duplicating keys.

**Why this priority**: Task keys are unique, and users often start a ticket as a
normal task before realising it should be split; without this the action fails in the
common case.

**Independent Test**: Create a normal backlog task with the ticket's key and another
with one sub-task's key, run "Create parent task", and check that the first became the
parent task, the second was attached as its subtask, and the rest were created.

**Acceptance Scenarios**:

1. **Given** a normal task with the ticket's key that can become a parent task (as
   spec 012 allows), **When** the user creates the parent task, **Then** that task is
   turned into the parent task instead of creating a new one, and keeps its state.
2. **Given** a task with the ticket's key that is already a parent task, **When** the
   user runs the action, **Then** it works as "add new Jira sub-tasks" (User Story 3).
3. **Given** a task with the ticket's key that cannot become a parent task (it is a
   subtask, has an open PR or a running agent, or is archived), **When** the user runs
   the action, **Then** it is refused with a message that says why, and nothing is
   created.
4. **Given** a normal task with a sub-task's key that never started, in the same
   project, **When** the parent task is created with that sub-task ticked, **Then**
   that task is attached as a subtask instead of creating a new one.
5. **Given** a task with a sub-task's key that cannot be attached (it started, is in
   another project, or already belongs to another parent task), **When** the parent
   task is created, **Then** that sub-task is shown as "already a task" and left out,
   the rest are created, and the result says which were left out and why.
6. **Given** any failure while creating, **When** it happens, **Then** nothing is left
   half done: no parent task without the subtasks the user confirmed, no stray tasks.

---

### User Story 3 - Bring in sub-tasks added in Jira later (Priority: P2)

Some time after the parent task was created, someone adds a sub-task to the ticket in
Jira. When the user opens the parent task, it says "1 new Jira sub-task" and offers to
add it, with the same list and default selection as at creation. A sub-task the user
does not want can be skipped so it is not offered again.

**Why this priority**: Tickets grow while work is in progress; without it the two
sides drift, but the first import already delivers most of the value.

**Independent Test**: Create a parent task from a ticket, add a sub-task to the sample
Jira data, open the parent task and check the offer; add it and check the new subtask;
add another, skip it, reopen the parent task and check it is no longer offered.

**Acceptance Scenarios**:

1. **Given** a parent task linked to a Jira ticket that has sub-tasks with no Delilah
   task, **When** the user opens the parent task, **Then** it shows how many there are
   and offers to add them, listing each with its key, title, status and assignee.
2. **Given** that offer, **When** the user adds the ticked ones, **Then** they become
   subtasks of the parent task in the backlog, as at creation; the parent task's
   progress counts them from then on.
3. **Given** that offer, **When** the user skips a sub-task, **Then** it is not offered
   again for that parent task, and the parent task can show and undo the skipped ones.
4. **Given** a parent task, **When** Jira cannot be reached or is not connected,
   **Then** the parent task works as before and shows no offer and no error beyond a
   quiet note.
5. **Given** a parent task created by hand (not from Jira) whose key is a Jira ticket,
   **When** the user opens it, **Then** it offers the ticket's sub-tasks in the same way.
6. **Given** the CLI, **When** the user runs the sync command on a parent task,
   **Then** the new Jira sub-tasks are listed and can be added with the same defaults.

---

### User Story 4 - See how the subtasks stand in Jira (Priority: P3)

The parent task's subtask list shows each subtask's Jira status next to its Delilah
state, so the user notices when a sub-task was closed, reopened or removed in Jira.

**Why this priority**: Useful for the overview, but nothing breaks without it.

**Acceptance Scenarios**:

1. **Given** a parent task with subtasks from Jira, **When** the user looks at its
   subtask list, **Then** each subtask shows its current Jira status.
2. **Given** a Jira sub-task that no longer exists or no longer belongs to the ticket,
   **When** the user looks at the list, **Then** that subtask is marked "not in Jira
   any more" and is left as it is (never archived or deleted on its own).

---

### Edge Cases

- A ticket that is itself a Jira sub-task: "Create parent task" is not offered on it
  (two levels only).
- A sub-task moved in Jira to another ticket after it became a subtask: it stays where
  it is in Delilah; User Story 4 shows it as no longer under this ticket.
- The same ticket chosen for a second project: refused, since the key already exists
  (keys are unique across projects).
- A ticket with many sub-tasks (e.g. 50): all are listed and can be ticked at once.
- Jira returns an error or times out while the list is loading: the list says so and
  offers retry; nothing is created.
- Sample data (no real Jira): the whole flow works against it.
- A parent task whose Jira sub-tasks were all skipped or are all already subtasks:
  no offer is shown.
- A subtask deleted in Delilah: its Jira sub-task is offered again as new on the next
  check (it is no longer a task); an archived subtask is not offered again.

## Requirements *(mandatory)*

### Functional Requirements

**Create**

- **FR-001**: Users MUST be able to create, from a Jira ticket that has sub-tasks and
  is not itself a sub-task, a parent task with the ticket's key and title and one
  subtask per chosen Jira sub-task with that sub-task's key and title, in one project
  they choose, from the Jira view, the CLI and the API.
- **FR-002**: Before creating, the system MUST show every Jira sub-task of the ticket
  with its key, title, Jira status and assignee, and whether it is already a Delilah
  task.
- **FR-003**: By default, sub-tasks that are not done in Jira and are assigned to the
  current Jira user or to nobody MUST be selected; done sub-tasks and sub-tasks
  assigned to someone else MUST NOT be. The user MUST be able to change any choice.
- **FR-004**: The parent task and its subtasks MUST be created in the backlog, with the
  behaviour of spec 012 (no workspace, no agent until started).
- **FR-005**: In a parent repository, the submodules chosen for the parent task MUST be
  used for the subtasks created with it.
- **FR-006**: Creation MUST be all or nothing: if any part fails, no task is created or
  changed.

**Existing tasks**

- **FR-007**: If a normal task with the ticket's key exists and can become a parent
  task under spec 012's rules, it MUST be turned into the parent task instead of
  creating one; if it is already a parent task, the action MUST add the chosen
  sub-tasks to it; otherwise the action MUST be refused with the reason.
- **FR-008**: If a task with a chosen sub-task's key exists and can be attached under
  spec 012's rules (a normal task that never started, in the same project), it MUST be
  attached as a subtask; if it already is a subtask of this parent task it MUST be left
  as is; otherwise that sub-task MUST be left out and reported, without failing the rest.

**Jira link and sync**

- **FR-009**: A parent task and the subtasks created from Jira MUST stay linked to
  their Jira keys, so that later checks can match them with Jira.
- **FR-010**: When a parent task whose key is a Jira ticket is opened, the system MUST
  check that ticket's sub-tasks and offer the ones that are neither a Delilah task
  nor skipped for this parent task, with the defaults of FR-003.
- **FR-011**: Users MUST be able to add offered sub-tasks as subtasks (with FR-004,
  FR-005 and FR-008) and to skip them; skipped sub-tasks MUST NOT be offered again for
  that parent task until the user undoes the skip.
- **FR-012**: The check MUST happen only when the user opens the parent task or asks
  for it (UI, CLI, API), never in the background, and MUST NOT block or slow the
  parent task screen when Jira is slow or unreachable.
- **FR-013**: The system MUST NOT write anything to Jira, and MUST NOT change, archive
  or delete a Delilah task because of a change in Jira.
- **FR-014**: The parent task's subtask list MUST show each subtask's Jira status when
  it is known, and mark subtasks whose Jira sub-task no longer exists or no longer
  belongs to the ticket.

**Agent context**

- **FR-015**: A subtask created from a Jira sub-task MUST give its agent that sub-task
  as its ticket, as any Jira task does today, and MUST also name the parent ticket.

**Docs**

- **FR-016**: The user docs MUST describe the flow under "Parent tasks and subtasks"
  and "Jira", and the status page MUST no longer list the Jira adapter as pending.

### Key Entities

- **Jira sub-task (as offered)**: key, title, Jira status and its category (to-do, in
  progress, done), assignee, whether it is assigned to the current user, and whether
  it is already a Delilah task (and which).
- **Parent task (from Jira)**: a spec 012 parent task whose key is a Jira ticket; holds
  the list of Jira sub-tasks the user skipped.
- **Subtask (from Jira)**: a spec 012 subtask whose key is a Jira sub-task of the
  parent task's ticket.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: A ticket with 5 sub-tasks becomes a parent task with its subtasks in one
  action and at most 3 clicks after opening the ticket, with no typing.
- **SC-002**: In 100% of cases, keys and titles of the created tasks match Jira exactly,
  and no key is duplicated.
- **SC-003**: Running the action twice on the same ticket creates nothing the second
  time unless Jira has new sub-tasks.
- **SC-004**: A sub-task added in Jira is offered the next time the parent task is
  opened, with no other step by the user.
- **SC-005**: With Jira unreachable, the parent task screen opens as fast as without
  this feature and every parent task action still works.

## Assumptions

- The current Jira user is the one whose credentials Delilah uses; "assigned to me"
  compares the sub-task's assignee with that user.
- Keys are unique across Delilah, so a ticket can be a parent task in one project only.
- The parent task and its subtasks are created in one project; per-subtask projects
  are out of scope (spec 012 requires the same project).
- Jira sub-task titles become task titles as they are; branch names follow the
  existing rule for Jira tasks (`feature/<KEY>-<short-name>`).
- Out of scope: writing to Jira (status transitions, comments), background polling,
  syncing titles or descriptions after creation, Jira issue links other than sub-tasks
  (e.g. stories under an epic), and more than two levels.
