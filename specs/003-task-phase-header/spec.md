# Feature Specification: Task phase in the task screen header

**Feature Branch**: `feature/FEAT-11`

**Created**: 2026-10-20

**Status**: Draft

**Input**: User description: "En la pantalla de tasks poner el paso en el que se encuentra en la cabecera. Para cuando este en el claude pueda ver el estado"

## User Scenarios & Testing *(mandatory)*

### User Story 1 - See the task's phase while working with the agent (Priority: P1)

The user opens a task and works in its agent terminal. Without leaving the terminal or
opening the spec panel, they glance at the task screen header and see which step of the
workflow the task is in (Especificación, Plan, Implementación, Revisión), next to the
status it already shows (running, waiting for the user…).

**Why this priority**: it is the whole request: knowing where the task stands while the
terminal takes the screen.

**Independent Test**: open a task with a workspace whose phase is "plan"; the header
reads "Plan" next to the status, with the terminal in front.

**Acceptance Scenarios**:

1. **Given** a running task in the "implement" phase, **When** the user opens its task
   screen, **Then** the header shows "Implementación" next to the status.
2. **Given** the task screen is open on the terminal, **When** the phase changes (the
   agent reaches the next step, or the user moves the task on the board or in the spec
   panel), **Then** the header shows the new phase without the user reloading or leaving
   the screen.
3. **Given** a task, **When** the user compares the header with the board column and the
   spec panel's stepper, **Then** all three show the same phase.

---

### User Story 2 - See implementation progress in the header (Priority: P2)

While the task is in "Implementación" and its spec has a task list, the header also
shows how far it has got (for example "Implementación · 4/12").

**Why this priority**: the implement phase is the longest; the count tells whether it is
moving without opening the spec panel. Useful, but the phase alone already answers the
request.

**Independent Test**: open a task in "implement" whose task list has 4 of 12 items done;
the header reads "Implementación · 4/12".

**Acceptance Scenarios**:

1. **Given** a task in "implement" with 4 of 12 items done, **When** the user opens it,
   **Then** the header shows "4/12" next to the phase.
2. **Given** a task in another phase, or with no task list yet, **When** the user opens
   it, **Then** the header shows the phase with no count.

---

### Edge Cases

- **Backlog task** (not started): the status already says "Backlog"; no phase is shown.
- **Finished task**: the status already says it is done; no phase is shown.
- **Task without a workspace or spec information** (archived, or the spec state is
  unavailable): no phase is shown, matching the spec panel, which shows nothing then.
- **Phase recorded but no spec folder yet**: the phase recorded is shown (the same rule
  the board and the spec panel use).
- **Project screen** (no task open): the header is unchanged.
- **Narrow window / long title**: the title truncates first; the phase and the status stay
  readable.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The task screen header MUST show the task's current workflow phase, with
  the same localized name the board and the spec panel use, next to the status.
- **FR-002**: The phase shown MUST be the one the board column and the spec panel show
  for the same task (same rule for deciding it).
- **FR-003**: The header MUST update when the phase changes, with the same freshness as
  the rest of the task data, without the user reloading or leaving the screen.
- **FR-004**: The phase MUST NOT be shown for backlog tasks, finished tasks, or tasks
  with no spec information.
- **FR-005**: While the phase is "Implementación" and the spec has a task list, the
  header SHOULD show the items done out of the total (P2).
- **FR-006**: The phase indicator MUST be visually distinct from the status (the user can
  tell "which step" from "what the agent is doing").
- **FR-007**: The header's existing content (owner, title, status, file-tree toggle)
  MUST keep working; with little space the title truncates before the phase or status.
- **FR-008**: Texts MUST be translatable like the rest of the UI (Spanish in the code,
  English in the translations).

### Key Entities

- **Task**: has a status (what the agent is doing) and a phase (which workflow step it
  is in).
- **Spec summary**: per task with a workspace: phase, spec folder, items done and total.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: With the terminal open, the user can tell the task's phase in under 2
  seconds, with no clicks.
- **SC-002**: In 100% of cases the header, the board column and the spec panel show the
  same phase for a task.
- **SC-003**: A phase change shows in the header within the same time it shows on the
  board.
- **SC-004**: At the app's minimum window width, the phase and the status remain fully
  readable.

## Assumptions

- "El paso" means the workflow phase (Especificación, Plan, Implementación, Revisión),
  not the agent status, which the header already shows.
- The phase information already exists and reaches the app; no daemon or API change is
  needed.
- The phase is display-only in the header; changing it stays on the board and in the
  spec panel's stepper.
- Only the Claude Code adapter and the SpecKit workflow exist today; tasks follow that
  phase set.
