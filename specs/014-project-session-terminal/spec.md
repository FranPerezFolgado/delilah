# Feature Specification: Project Claude sessions open in the terminal

**Feature Branch**: `014-project-session-terminal`

**Created**: 2026-11-01

**Status**: Draft

**Input**: User description (LIBRE-32): "When I open a Claude session on a project (three dots in
the tab bar / 'Ask Claude'), it opens as chat and I can't change it to terminal; by default it
should be terminal."

## Context

A project's Claude sessions ("Ask Claude", the "…" button on a project, "+" on the project) always
run in a terminal; their chat is only a reading of the same conversation. Today the project
screen shows them in the chat view by default and hides the Chat | Terminal switch. The only
way to get to the terminal is a button that appears in a few chat states, and once there the
user cannot go back to the chat. The user guide already says project sessions use the terminal,
so the screen contradicts it.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - A project session opens in the terminal (Priority: P1)

The user opens a Claude session on a project from the sidebar ("…" / "Ask Claude" / "+"). The new
session's tab shows Claude's terminal straight away, as task sessions in terminal mode do.

**Why this priority**: This is the reported bug. The terminal is how project sessions run, and it is
what the user expects to see.

**Independent Test**: Open a project session in a preview. The tab shows the terminal, with no
step in between.

**Acceptance Scenarios**:

1. **Given** a registered project, **When** the user opens a Claude session on it from the sidebar,
   **Then** the session's tab shows the terminal.
2. **Given** a project with an existing Claude session (read-only or with changes allowed), **When**
   the user opens the project's screen and selects that session's tab, **Then** it shows the
   terminal unless the user picked Chat for it while this screen was open.
3. **Given** a project session shown in the terminal, **When** the user restarts it in normal mode
   ("allow changes in this session"), **Then** it still shows the terminal.

---

### User Story 2 - Switch a project session between Terminal and Chat (Priority: P1)

On a project's screen, the Chat | Terminal switch appears above the open Claude session's tab, as
it does on a task. On a project it only changes what is shown: picking Chat shows the
conversation of that same session, and picking Terminal brings the terminal back. Nothing
restarts, nothing asks for confirmation, and the choice applies only to that session while
the screen is open.

**Why this priority**: Without it the user cannot move between the two views. The report says "I
can't change it".

**Independent Test**: On a project session, pick Chat, then Terminal, then Chat again. Each pick
changes the view at once. The session keeps running, and its conversation and scrollback are
still there.

**Acceptance Scenarios**:

1. **Given** a project session shown in the terminal, **When** the user picks Chat, **Then** the tab
   shows that session's conversation, with no confirmation and no restart.
2. **Given** a project session shown in the chat, **When** the user picks Terminal, **Then** the tab
   shows its terminal with the scrollback it had.
3. **Given** Claude is working on a turn in a project session, **When** the user flips the switch,
   **Then** the view changes. The switch is not disabled, because no restart happens.
4. **Given** two project sessions, **When** the user picks Chat on one, **Then** the other keeps its
   own view.
5. **Given** the chat's own "open the terminal" button, **When** the user presses it, **Then** the
   switch shows Terminal as selected.

---

### User Story 3 - Task sessions and idea conversations keep their behaviour (Priority: P2)

On a task, the switch still changes the session's mode (chat mode or terminal), with its
confirmation and its "wait for the turn" rule. Idea conversations (the chat that turns an idea
into a task) still open in Chat, and they get the same view-only switch as other project
sessions. Plain shell terminals still have no switch.

**Why this priority**: These are regression guards. The change must not touch how tasks run.

**Independent Test**: On a task, the switch still asks before changing the mode. An idea
conversation opens in Chat and can be flipped to Terminal and back.

**Acceptance Scenarios**:

1. **Given** a task's Claude session, **When** the user picks the other view, **Then** the
   confirmation appears and the mode changes as it does today.
2. **Given** an idea conversation, **When** it opens, **Then** it shows the chat and the switch
   offers Terminal.
3. **Given** a plain terminal tab on a project or a task, **Then** no switch is shown.

### Edge Cases

- A project session whose process is not running (the daemon restarted): it opens in the terminal
  view with the existing "not running / Reopen" bar. Chat stays available through the switch.
- A project session the user moved to Chat, then the screen is closed and opened again: it opens in
  the terminal again. The choice is not stored.
- A project session turned into a task ("Turn into a task"): from then on it is a task session and
  follows the task's rules.
- A project session that reports chat mode (not expected today): it is shown in chat, as now.
- Errors: the project switch never contacts the daemon, so it has no failure message of its own.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: A project's Claude sessions, other than idea conversations, MUST show the Terminal view
  by default when their tab is shown.
- **FR-002**: Idea conversations MUST keep opening in the Chat view by default.
- **FR-003**: The project screen MUST show the Chat | Terminal switch for the Claude session of the
  open tab, in the same place as on a task.
- **FR-004**: On a project, the switch MUST only change what that session's tab shows. It MUST NOT
  restart the session, change its mode, ask for confirmation, or contact the daemon.
- **FR-005**: On a project, the switch MUST stay usable while Claude works on a turn or waits for an
  answer.
- **FR-006**: Each session's choice MUST be kept separately while the screen is open, and MUST NOT be
  stored once the screen closes.
- **FR-007**: The switch MUST always show the view the tab is showing, including after the chat's own
  "open the terminal" button is used.
- **FR-008**: On a task, the switch MUST behave as it does today (mode change with confirmation and the
  turn rule). A project session's choice MUST NOT change any task's saved view.
- **FR-009**: Plain shell terminal tabs MUST NOT show the switch.
- **FR-010**: The user guide MUST describe project sessions as opening in the terminal with a
  view-only Chat | Terminal switch, and the status document MUST record the change.

### Key Entities

- **Project session view**: the view (Chat or Terminal) of one project Claude session on the open
  screen. It starts at Terminal (Chat for idea conversations) and changes only with the switch or
  the chat's "open the terminal" button. It is gone when the screen closes.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: 100% of newly opened project Claude sessions (not idea conversations) show the terminal
  on first display, with no extra click.
- **SC-002**: A user can move a project session from Terminal to Chat and back in 2 clicks, with no
  dialog in between.
- **SC-003**: Flipping the view of a project session never restarts it: its conversation and
  terminal scrollback are the same before and after.
- **SC-004**: On tasks, nothing changes: the existing switch scenarios still pass.

## Assumptions

- Project sessions keep running in a terminal. This feature changes only what the project screen
  shows, not how sessions run.
- Idea conversations default to Chat because that flow is a conversation. The user can still pick
  Terminal for one.
- Not storing the per-session choice matches the task screen, which keeps it only while open. A
  remembered preference can come later if asked for.
- Read-only project sessions and those with changes allowed behave the same.
