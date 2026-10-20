# Feature Specification: Agent playground

**Feature Branch**: `feature/LIBRE-3`

**Created**: 2026-10-20

**Status**: Draft

**Input**: User description: "Ahora que orquestamos, hay forma de poder visualizar bien los agentes que estan trabajando? Como brag estaría guapo meter como un playground donde se vean, haz research de ejemplos que hay en internet y lo analizamos si merece la pena"

## User Scenarios & Testing *(mandatory)*

### User Story 1 - See every working agent at a glance (Priority: P1)

The user has several tasks running in parallel. They open the Playground from the
sidebar and see one character at a desk for each agent that is running right now. Each
desk carries a label with the task key, its title and the workflow phase it is in
(Especificación, Plan, Implementación, Revisión). The character's animation shows its state:
arriving at the desk while the agent starts, busy while it works, raising a hand with a
bubble while it waits for the user, and asleep while it is paused by the usage limit.

**Why this priority**: it is the core of the request: one screen that shows the whole
team working, live. Without it there is no playground.

**Independent Test**: start two tasks; open the Playground; there are two desks, each
labelled with its task and phase, and their characters' animations change as the agents
start, work and stop.

**Acceptance Scenarios**:

1. **Given** three tasks with a running agent each, **When** the user opens the
   Playground, **Then** it shows three desks, each with its task key, title and phase.
2. **Given** the Playground is open, **When** an agent goes from working to waiting for
   the user (or back), **Then** its character changes animation within a couple of
   seconds, without the user reloading or leaving the view.
3. **Given** the Playground is open, **When** a new agent starts or a running one ends,
   **Then** a desk appears for it or its desk empties, without a reload.
4. **Given** no agent is running, **When** the user opens the Playground, **Then** it
   shows an empty office with a short message saying no agent is working.

---

### User Story 2 - Notice who needs me (Priority: P1)

Among several busy agents, the ones waiting for the user stand out: the character
raises a hand with a bubble, the desk is highlighted, and a counter at the top of the
Playground says how many are waiting. Clicking any character opens that task's screen
on its agent's terminal, so the user can answer straight away.

**Why this priority**: this is what makes the view useful day to day and not only nice
to look at: the user spots the blocked agent and jumps into it in one click.

**Independent Test**: have one of two running agents ask for permission; its desk stands
out, the counter reads 1, and clicking its character opens that task on its terminal.

**Acceptance Scenarios**:

1. **Given** two running agents, one waiting for the user, **When** the user looks at the
   Playground, **Then** the waiting one is visibly highlighted and the counter shows 1.
2. **Given** the Playground, **When** the user clicks a character (or focuses it with the
   keyboard and presses Enter), **Then** the task screen opens on that agent's terminal.
3. **Given** the user answers the waiting agent and comes back, **When** the agent
   resumes work, **Then** its highlight goes away and the counter drops to 0.

---

### User Story 3 - See what each agent is doing right now (Priority: P2)

While an agent works, its character shows *what kind* of work it is doing: typing when
it edits files, reading when it reads or searches, at a terminal when it runs commands,
on the phone when it hands work to a helper agent, and thinking between actions. A small
caption under the desk names the action (for example "Editing api.rs", "Running cargo
test").

**Why this priority**: it turns "busy" into "doing X", which is what makes the view worth
watching and showing off. The view is already useful without it (stories 1 and 2), so it
comes second.

**Independent Test**: watch an agent that reads a file, edits it and runs the tests; its
character goes reading → typing → terminal, and the caption follows.

**Acceptance Scenarios**:

1. **Given** an agent that starts editing a file, **When** the user looks at its desk,
   **Then** the character is typing and the caption names the edit and the file.
2. **Given** an agent that runs a shell command, **When** the user looks at its desk,
   **Then** the character is at a terminal and the caption shows the start of the command.
3. **Given** an agent that finishes an action and has not started another, **When** the
   user looks at its desk, **Then** the character is thinking and the caption is cleared.
4. **Given** an action of an unknown kind (for example a tool from an extension), **When**
   the agent uses it, **Then** the character shows a generic "busy" animation and the
   caption names the action.

---

### User Story 4 - A view worth a screenshot (Priority: P3)

The Playground looks like a small, coherent pixel-art office: one style throughout, desks
laid out on a grid that grows with the number of agents, and a calm animation that
does not distract. It follows the app's light and dark themes and looks good in a
screenshot or a short screen recording.

**Why this priority**: the "brag" half of the request. It polishes stories 1–3 but adds no
information.

**Independent Test**: with four running agents, take a screenshot in light and in dark
theme; both read clearly, with no clipped labels or overlapping desks.

**Acceptance Scenarios**:

1. **Given** 1 to 8 running agents, **When** the user opens the Playground, **Then** every
   desk fits in the window without overlapping, and labels are not clipped.
2. **Given** the app in dark theme, **When** the user opens the Playground, **Then** the
   office uses dark colours and stays legible.
3. **Given** the system setting to reduce motion is on, **When** the user opens the
   Playground, **Then** characters show their state with still poses instead of looping
   animations.

---

### Edge Cases

- A task has more than one agent session (for example, a second Claude tab): each live
  agent session gets its own desk, all labelled with the same task; plain shell
  terminals get no desk.
- More agents than fit on screen (more than 8): the office grows and scrolls; the waiting
  counter still counts all of them.
- The daemon connection drops: the Playground keeps the last known picture, marks it as
  stale, and refreshes on reconnect.
- An agent's action is never reported as finished (the agent crashes mid-action): the
  action clears when the agent stops, fails or goes to waiting.
- A task's title is very long: the label truncates it and shows the full title on hover.
- The Playground is hidden (another view open, window minimised): animations stop; they
  resume with the current state when it is shown again.
- An agent's action carries a long or sensitive argument (a long command, a path outside
  the repo): the caption shows only a short prefix; nothing is shown beyond what the
  terminal already shows.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The app MUST offer a Playground view, reachable from the sidebar like the
  other top-level views.
- **FR-002**: The Playground MUST show one desk with a character for each live agent
  session, and none for stored, ended or shell-only sessions.
- **FR-003**: Each desk MUST show the task key, the task title (truncated, full on hover)
  and the task's current phase, using the same phase names as the rest of the app.
- **FR-004**: Each character MUST show its agent's state with a distinct look: starting,
  working, waiting for the user, paused by the usage limit.
- **FR-005**: The Playground MUST update desks, states and actions live, within 2 seconds
  of the change, without a reload.
- **FR-006**: Agents waiting for the user MUST be highlighted, and the view MUST show the
  number of agents waiting.
- **FR-007**: Clicking a character, or focusing it with the keyboard and pressing Enter,
  MUST open the task screen on that agent's terminal.
- **FR-008**: The system MUST know, for each working agent, the action it is doing right
  now, if any, and its kind: editing, reading/searching, running a command, delegating
  to a helper agent, or other; and none when it is between actions.
- **FR-009**: Each working character MUST show the kind of its current action with its
  own animation and a short caption (the action plus a short, truncated target such as a
  file name or the start of a command), and a thinking pose when there is no current
  action.
- **FR-010**: The current action MUST clear when the action ends, and when the agent
  stops, fails, waits for the user or is paused.
- **FR-011**: The Playground MUST follow the app's light and dark themes and the system's
  reduce-motion setting.
- **FR-012**: The Playground MUST stop animating while it is not visible.
- **FR-013**: All graphics MUST be original or under a licence that allows shipping them
  in the app (public domain / CC0 or equivalent), and their source MUST be recorded.
- **FR-014**: When the connection to the daemon is lost, the Playground MUST show that its
  picture may be out of date and refresh when the connection returns.

### Key Entities

- **Agent at work**: a live agent session shown in the Playground: the task it belongs to
  (key, title, phase), its state (starting, working, waiting, paused) and its current
  action, if any.
- **Current action**: what a working agent is doing right now: its kind (editing,
  reading/searching, running a command, delegating, other), a short name and a short
  target; it lives only while the action runs and is not kept in the task's history.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: With 4 agents running, a user can tell which ones are waiting for them in
  under 3 seconds of opening the Playground.
- **SC-002**: A user can go from the Playground to answering a waiting agent in 1 click.
- **SC-003**: State and action changes show in the Playground within 2 seconds in 95% of
  cases.
- **SC-004**: With 8 agents on screen, the app stays responsive (typing in a terminal
  shows no noticeable lag) while the Playground is open.
- **SC-005**: In a screenshot with 4 working agents, an observer who has never seen the
  app can say what each agent is doing (editing, reading, running something, waiting)
  for at least 3 of the 4.

## Assumptions

- Only the Claude Code agent is supported, as in the rest of the app; other agents get
  a desk once their adapter reports the same states and actions.
- Helper agents launched by an agent (subagents) are not shown as their own characters in
  this version; the parent's character shows "delegating" while they run. Showing them as
  temporary characters is a later step.
- Extra gadgets (context fill, usage meters, kanban boards on the wall), an editable
  office layout, characters walking between places, and game engines are out of scope.
- The current action is live state only: it is not stored, and the Playground shows
  nothing about past actions.
- The Playground needs no settings in this version; the pixel-art office is the only
  look.
- It runs in the desktop app's web UI, so it can also be tried in a preview in a
  browser; nothing native to macOS is needed.
