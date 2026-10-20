# Feature Specification: Subtareas y sprint actual en la vista Jira

**Feature Branch**: `feature/FEAT-2`

**Created**: 2026-10-20

**Status**: Draft

**Input**: User description: "quiero mejorar la vista de jira. Ahora mismo filtrando por epicas, solo me salen las tareas, pero quiero poder ver tambien las subtareas, como podemos hacer esto? Tambien que se filtre por sprints, para ver el actual."

## Context

When the Jira view is grouped by epic, a sub-task's epic is not detected: its
parent is a story or task, not an epic, so the sub-task falls into "Sin épica".
On top of that, a sub-task only shows up when it passes the active filter on its
own (for example, assigned to the user), so the sub-tasks other people are
working on, which belong to the user's own tickets, are never shown.
There is no way to narrow the list to the sprint in progress either.

Decision taken with the user: sub-tasks are shown **nested under their parent
ticket**, inside the parent's group, and **all** sub-tasks of the listed
tickets are included, whoever they are assigned to.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - See a ticket's sub-tasks nested under it (Priority: P1)

Fran opens the Jira view grouped by epic. Under each story or task, its
sub-tasks appear indented, with their own status, assignee and type, so the
breakdown of each piece of work and who is doing each part are visible at a glance.

**Why this priority**: it is the main problem reported: today the sub-tasks
are lost or end up in "Sin épica".

**Independent Test**: with the sample data, group by epic and check that
each sub-task appears under its parent, inside the parent's epic, and that
none ends up in "Sin épica" when its parent has an epic.

**Acceptance Scenarios**:

1. **Given** a story in epic E with two sub-tasks, **When** grouping by epic,
   **Then** both sub-tasks appear indented under the story, inside E, and
   not in "Sin épica".
2. **Given** a sub-task of one of my tickets that is assigned to someone else,
   **When** I list "Asignados", **Then** the sub-task appears under my ticket
   with the other person's avatar.
3. **Given** grouping by status, **When** a ticket has sub-tasks, **Then** they
   appear indented under the parent, in the parent's status group, each one with
   its own status visible.
4. **Given** a ticket with sub-tasks, **When** I fold its group, **Then** the
   sub-tasks are hidden along with the parent; group counts include only the
   top-level tickets and the parent row shows how many sub-tasks it has.
5. **Given** a sub-task selected in the list, **When** I look at the detail
   panel, **Then** I see its parent ticket (key and title) and its epic, and I
   can start "Analizar con Claude" on it like any other ticket.

---

### User Story 2 - Filter by the current sprint (Priority: P1)

Fran turns on "Sprint actual" in the filter bar and the list narrows to
the tickets in the open sprint, combined with the "De quién" and "Estado"
filters that are already there.

**Why this priority**: it is the user's second explicit request and the usual
way to look at the work in progress.

**Independent Test**: with the sample data, turn on "Sprint actual" and check
that only tickets from the open sprint remain; turn it off and they all come back.

**Acceptance Scenarios**:

1. **Given** tickets in the open sprint, in a closed sprint and in the backlog,
   **When** I turn on "Sprint actual", **Then** only the ones in the open sprint remain.
2. **Given** "Sprint actual" turned on, **When** I switch between "Asignados" and
   "Reportados" or between "Abiertos" and "Hechos", **Then** the sprint filter
   stays on and combines with the others.
3. **Given** "Sprint actual" turned on, **When** a listed ticket has
   sub-tasks, **Then** they appear nested as in Story 1.
4. **Given** "Mi consulta" with a saved JQL, **When** I turn on "Sprint
   actual", **Then** the sprint condition is added to the saved query without
   losing its order.

---

### User Story 3 - See which sprint each ticket is in (Priority: P2)

Each row and the detail panel show the name of the ticket's current
sprint, so Fran can tell the sprint's work apart from the backlog without
filtering.

**Why this priority**: it complements the filter, but the filter alone already
solves "see the current one".

**Independent Test**: with the sample data, check that the tickets in a
sprint show its name in the detail panel and the backlog ones show "—".

**Acceptance Scenarios**:

1. **Given** a ticket in sprint "Sprint 42", **When** I select it, **Then** the
   panel shows "Sprint: Sprint 42".
2. **Given** a ticket that has gone through several sprints, **When** it is
   shown, **Then** the open sprint is shown (or, failing that, the most recent one).

### Edge Cases

- A sub-task that passes the filter on its own but whose parent is not in the list
  (the parent is assigned to someone else, or is already done): it is shown as a
  row with a "↳ PARENT-KEY title" hint, inside the parent's epic or status group.
- A sub-task whose parent has no epic: it goes, with its parent, to "Sin épica".
- A ticket with many sub-tasks (more than 20): they are all shown; the
  parent can be folded.
- Text filter: if a sub-task matches, it is shown along with its parent (as
  context) even if the parent doesn't match; if the parent matches, all of its
  sub-tasks are shown.
- The Jira site has no sprints (no Jira Software, or a Kanban project): the
  "Sprint actual" control is still offered; turning it on with no open sprints
  shows the empty state "No hay tickets para esta consulta." and no technical error.
- Jira rejects the sprint condition (the site doesn't recognise the field): the
  view shows a readable message and the user can turn the filter off.
- Several open sprints at once (parallel boards): "Sprint actual" covers
  all of them.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The system MUST identify a sub-task's epic through its parent
  ticket, so the sub-task belongs to the same epic as its parent.
- **FR-002**: The system MUST include in the list all the sub-tasks of every
  top-level ticket it lists, whoever they are assigned to.
- **FR-003**: The list MUST show each sub-task indented under its parent,
  in the parent's group, both when grouping by epic and when grouping by status.
- **FR-004**: A sub-task whose parent is not in the list MUST be shown as a
  top-level row with a visible reference to its parent.
- **FR-005**: Group counts MUST count only top-level rows; each parent row
  MUST show its number of sub-tasks.
- **FR-006**: The detail panel for a sub-task MUST show its parent ticket and
  its epic; every existing action ("Analizar con Claude", "Ver la tarea en
  Delilah") MUST work the same for sub-tasks.
- **FR-007**: The filter bar MUST offer a "Sprint" control with the options
  "Todos" and "Actual", combinable with "De quién" and "Estado"; "Todos"
  keeps today's behaviour.
- **FR-008**: With "Actual", the system MUST list only the tickets that belong
  to an open sprint (covering every open sprint the user can see).
- **FR-009**: The system MUST show the ticket's sprint name in the detail
  panel and as a tag on the row; with no sprint it shows "—" in the panel and
  nothing on the row.
- **FR-010**: The sample data MUST include sub-tasks (some assigned to someone
  else, one whose parent is not in the list) and tickets in an open sprint,
  a closed sprint and the backlog, so every scenario can be tested with no access
  to a real Jira.
- **FR-011**: This MUST work the same on Jira Cloud and on Jira Server / Data
  Center.
- **FR-012**: The sprint filter selection SHOULD be kept while the view is
  in use, like the other filters.

### Key Entities

- **Ticket**: gains a **parent** (key and title, when it is a sub-task), its
  list of **sub-tasks** and its **sprint** (name and state: open/closed/future).
- **Sprint**: name and state; a ticket can have gone through several; the
  one that counts is the open one, or else the most recent.
- **Filter**: gains the **sprint** dimension (all / current) next to "who" and "status".

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: With the sample data grouped by epic, 100% of the sub-tasks
  whose parent has an epic appear inside that epic (0 in "Sin épica").
- **SC-002**: Going from "Todos" to "Sprint actual" refreshes the list as fast
  as the other filters do today (one click, no extra steps).
- **SC-003**: Every sub-task of a ticket that is listed is visible in the list
  (none missing because of who it is assigned to).
- **SC-004**: The existing scenarios in the Jira view (who/status filters,
  grouping, text filter, Analizar) keep working with no regressions.

## Assumptions

- In Jira, a sub-task always belongs to the same sprint as its parent, so the
  sprint filter applies to the parents and their sub-tasks are carried along.
- The sub-tasks of a listed ticket are shown whatever their status (done or
  not), with their status visible; the "Abiertos/Hechos" filter applies to the
  top-level tickets.
- "Sprint actual" means "any open sprint" for the user; choosing a specific
  sprint or board is out of scope for this iteration.
- The per-request ticket limit (50 today) applies to top-level tickets;
  their sub-tasks don't count towards it.
- Nesting is a single level (Jira doesn't allow sub-tasks of sub-tasks).
- No new permissions or credentials are needed: everything comes from the same
  connection already set up.
