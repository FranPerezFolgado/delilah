# Feature Specification: Session tabs named after their phase

**Feature Branch**: `011-tab-phase-names`

**Created**: 2026-10-26

**Status**: Draft

**Input**: User description: "LIBRE-24: the active tab of a task now reads «Main Claude» instead of the name it should have (Implementation, Review, etc.)."

## Context

In saving mode a task runs each phase (Spec and plan → Implementation → Review) in a
fresh Claude session. Each of those sessions is the task's primary session while its
phase lasts, and it is stored with the phase it works on and a phase name. The task
screen ignores that name: every primary session's tab, and its row in the task's
Sessions list, reads «Claude principal» / «Main Claude». With several phase sessions
in one task, the user cannot tell from the tabs which one is the current phase and
which ones are older ones.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - The current phase session's tab says which phase it is (Priority: P1)

A user opens a task that runs in saving mode. The tab of the session doing the
current phase reads the phase's name («Implementación» / «Implement»,
«Revisión» / «Review», «Spec y plan» / «Spec and plan»), not «Main Claude».

**Why this priority**: This is the reported bug; the tab bar is where the user looks
to find the session that is working now.

**Independent Test**: Open a task whose primary session works on the Implementation
step; the tab bar shows «Implement» (English) or «Implementación» (Spanish) and
no «Main Claude» tab.

**Acceptance Scenarios**:

1. **Given** a task whose primary session works on the Implementation step, **When**
   the task screen opens, **Then** that session's tab reads «Implementación» in
   Spanish and «Implement» in English.
2. **Given** a task whose primary session works on the Review step, **When** the task
   screen opens, **Then** its tab reads «Revisión» / «Review».
3. **Given** a task whose primary session works on the Spec and plan step, **When**
   the task screen opens, **Then** its tab reads «Spec y plan» / «Spec and plan».
4. **Given** a task not in saving mode (its primary session has no step), **When** the
   task screen opens, **Then** the primary tab still reads «Claude principal» / «Main
   Claude».
5. **Given** the phase session's tab, **When** the user switches the UI language,
   **Then** the tab shows the phase name in the new language.
6. **Given** a phase session's tab, **When** the user hovers it, **Then** its tooltip
   shows the session's stored label, as for any session.

---

### User Story 2 - Earlier phase sessions keep their phase name (Priority: P2)

After a handoff the old phase session stays in the task (stopped, or still open). It
reads its own phase name, so the tabs of a task in Review read «Spec and plan»,
«Implement», «Review».

**Why this priority**: It makes the history of phases readable, but the bug the user
hit is the current one.

**Independent Test**: Open a task with one stopped Implementation session and a
primary Review session; the tabs read «Implement» and «Review».

**Acceptance Scenarios**:

1. **Given** a task with a former Implementation session (no longer primary) and a
   primary Review session, **When** the task screen opens, **Then** the two tabs read
   «Implement» and «Review», in that order.
2. **Given** an extra Claude or a terminal the user opened (no step), **When** the
   task screen opens, **Then** its tab keeps its own label, as today.

---

### User Story 3 - The task's Sessions list uses the same names (Priority: P2)

The «Sesiones» list on the task tab names each session the same way the tab bar does.

**Why this priority**: The two places must agree, or the user cannot match a row to
its tab.

**Independent Test**: On the task tab of a saving-mode task, the Sessions list shows
«Implement» for the implementation session and no «Main Claude» row.

**Acceptance Scenarios**:

1. **Given** a saving-mode task, **When** the user opens its task tab, **Then** each
   phase session's row in the Sessions list shows its phase name.
2. **Given** a task not in saving mode, **When** the user opens its task tab, **Then**
   the primary Claude's row reads «Claude principal» / «Main Claude», as today.

### Edge Cases

- A primary session that is a terminal (not a Claude) and has no step keeps its own
  label, in both places. The daemon never creates one today, so in practice nothing
  changes for sessions without a step.
- Two sessions with the same step (a phase session reopened as a new one): both read
  the phase name; the tooltip (the stored label) and the order of the tabs tell them
  apart.
- The tab that is opening («Iniciando…» / «Starting…») is unchanged.
- Keyboard and screen readers: the tab's accessible name is its visible name, so the
  close button of a closable phase tab reads «Cerrar Implementación» / «Close
  Implement».

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: A session that works on a saving-mode step MUST be named with that
  step's name in the UI language, whether or not it is the task's primary session and
  whether it is live or stopped.
- **FR-002**: A primary session without a step MUST keep the name «Claude principal»
  / «Main Claude».
- **FR-003**: Any other session MUST keep its own stored label.
- **FR-004**: The task's tab bar and the task tab's Sessions list MUST use the same
  rule (FR-001 to FR-003), so a session reads the same in both.
- **FR-005**: The step names MUST be the same ones the saving-mode session chain
  already shows («Spec y plan», «Implementación», «Revisión» and their English
  translations).
- **FR-006**: A session tab's tooltip MUST keep showing the session's stored label.
- **FR-007**: The change MUST NOT alter what is stored for sessions or what the daemon
  returns; old sessions are named correctly without any migration.

### Key Entities

- **Session**: a process of a task, with a stored label, whether it is the task's
  primary one, and (in saving mode) the step it works on. Only how it is named on
  screen changes.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: In a saving-mode task, 0 tabs read «Main Claude»; each phase session's
  tab shows its phase name.
- **SC-002**: In a task not in saving mode, the primary tab still reads «Main Claude»
  (no regression).
- **SC-003**: For every session of a task, the name in the tab bar and the name in the
  Sessions list are identical.
- **SC-004**: After switching the UI language, the phase tabs read in the new language,
  like every other text of the task screen.

## Assumptions

- Only sessions with a saving-mode step get a phase name. A task not in saving mode
  has one primary session for every phase; renaming it after the current phase is not
  asked for and stays «Main Claude».
- The Inbox, the Office/playground and notifications are out of scope: they don't
  show «Main Claude» for the primary session today.
- The stored label of a phase session is written in the daemon's language when the
  session starts; the UI translates the step itself, so the shown name follows the UI
  language and the stored label is only used for the tooltip.
