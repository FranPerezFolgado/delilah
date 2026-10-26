# Feature Specification: Playground redesign (per-task office + Flow view)

**Feature Branch**: `feature/LIBRE-3`

**Created**: 2026-10-26

**Status**: Draft

**Input**: User description: "Rediseño del Playground: oficina por tarea estilo pixtuoid + vista Flujo estilo agent-flow. Brief completo en specs/004-agent-playground/redesign-brief.md"

Builds on spec `004-agent-playground` (decisions #79–#81). Everything that spec requires
and this one does not change still holds (live updates, accessible names, reduced
motion, stale banner, pause while hidden, separate window, captions cut at 40
characters, desks that never move). The brief's decisions are settled: per-task floor in
the task screen, a building overview in the Playground, Office ↔ Flow toggle, and
pixtuoid's art (MIT).

## Clarifications

### Session 2026-10-26

- Q: In the separate Playground window, what does clicking a floor do? → A: It opens
  that floor (with the Oficina/Flujo toggle) inside the separate window, with a way back
  to the building; the main window is not touched. Clicking a lead character there still
  opens its terminal in the main window.
- Q: What drives the token paper stack, given tokens are not tracked per session? → A:
  Real tokens: the daemon starts tracking the tokens each agent session has used, and
  the lead desk shows a paper stack by tiers (250K / 2M / 16M).

## User Scenarios & Testing *(mandatory)*

### User Story 1 - A task's own office floor (Priority: P1)

The user opens a task that has agents running and switches to its **Oficina** tab. They
see that task's floor of a pixel-art office: the task's agent sits at the lead desk and
each helper agent it launched sits at a desk of its own nearby, like a boss and their
team. The state is on the desk, not only in the pose: the monitor glows in the colour of
the kind of tool in use (editing, reading/searching, running a command, delegating,
other), a `?` floats over an agent that waits for the user, `zzz` over one that is
paused by the usage limit, and a short caption names the current action. When a helper finishes it
stands up and walks to the lounge, and its desk frees up. Clicking the lead agent opens
its terminal.

**Why this priority**: the user does not like how the current Playground looks; a
readable, good-looking floor scoped to the task they are working on is the core of the
redesign, and the place where they will see it most.

**Independent Test**: start a task whose agent launches two helpers; open the task's
Oficina tab; the lead and two helper desks are there with glowing monitors that change
colour with the tool; when a helper ends it walks to the lounge and its desk empties.

**Acceptance Scenarios**:

1. **Given** a task with a running agent, **When** the user opens its Oficina tab,
   **Then** the floor shows the agent at the lead desk with its state, monitor colour and
   caption.
2. **Given** the floor is open, **When** the agent goes from editing to running a command,
   **Then** its monitor colour and caption change within 2 seconds.
3. **Given** the agent launches a helper, **When** the helper starts, **Then** a helper
   character takes a free desk of the floor, labelled with its role, and its monitor shows
   its own current action.
4. **Given** a helper is at a desk, **When** it finishes, **Then** it walks to the lounge
   (or, with reduce motion on, appears there directly) and its desk frees up; it leaves the
   floor after about 5 seconds.
5. **Given** the agent waits for the user, **When** the floor is shown, **Then** a `?`
   floats over it and the desk is highlighted; **Given** it is paused by the usage limit,
   **Then** it shows `zzz`.
6. **Given** a task with no live agent, **When** the user opens its Oficina tab, **Then**
   the floor is shown empty with a short message saying no agent is working on the task.
7. **Given** two tasks of the same project, **When** their floors are shown, **Then**
   their lead characters wear the same outfit colours; tasks of different projects
   usually wear different ones.
8. **Given** an agent session that has used 3M tokens, **When** its floor is shown,
   **Then** its lead desk has the medium paper stack, and hovering it says "3M tokens".

---

### User Story 2 - The Playground as a building (Priority: P1)

The Playground (the sidebar view and its separate window) becomes a building: one small
floor per task with a live agent, stacked, each labelled with the task key, title and
phase, showing its agents in miniature and how many of them wait for the user. A counter
at the top says how many agents in total wait. In the main window, clicking a floor opens
that task's screen on its Oficina tab; in the separate window, it opens the floor right
there (with the Oficina/Flujo toggle and a way back to the building).

**Why this priority**: the overview of everything running is what the Playground is for;
without it the per-task floors are only reachable task by task.

**Independent Test**: start agents on two tasks; open the Playground; it shows two floors;
click one and the main window shows that task's Oficina tab.

**Acceptance Scenarios**:

1. **Given** three tasks with live agents, **When** the user opens the Playground,
   **Then** it shows three floors, each with its task key, title, phase and characters.
2. **Given** the Playground is open, **When** an agent starts on a new task or the last
   agent of a task ends, **Then** a floor appears or goes away without a reload, and the
   other floors keep their place.
3. **Given** an agent waits for the user, **When** the Playground is shown, **Then** its
   floor is highlighted and the top counter includes it.
4. **Given** the separate Playground window, **When** the user clicks a floor, **Then**
   the window shows that floor with the Oficina/Flujo toggle and a way back to the
   building, and the main window does not change; **When** the user then clicks a lead
   character, **Then** the main window comes to the front on that agent's terminal.
5. **Given** no agent is running, **When** the user opens the Playground, **Then** it
   shows an empty building with a short message saying no agent is working.

---

### User Story 3 - Flow view of what an agent did (Priority: P2)

In the same place as the office (the task's tab, and a floor opened from the building),
a toggle switches **Oficina ↔ Flujo**. Flujo is a live graph of the task's agents: each
agent is a node; each tool call it makes appears as a node linked to it, and each helper
it launches as an agent node of its own with its tool calls linked to it. A timeline
below lists the recent events in order with their time. New events appear as they
happen; the user can pan and zoom the graph and click a node or a timeline entry to see
its details (tool, target, when it started, how long it took, whether it is still
running). Opening the view later still shows the recent past, not only what happens from
then on.

**Why this priority**: it explains what an agent has been doing, beyond the single
current action the office shows; it depends on the floor's place (US1) to live in.

**Independent Test**: run an agent that edits files, runs commands and launches a helper;
open its task's tab and switch to Flujo; the agent, its tool calls and the helper with
its own calls are in the graph, the timeline lists them in order, and clicking one shows
its details.

**Acceptance Scenarios**:

1. **Given** an agent has made tool calls in the last minutes, **When** the user opens
   Flujo for its task, **Then** the graph and timeline show those calls (up to the history
   limit), oldest first in the timeline.
2. **Given** Flujo is open, **When** the agent makes a new tool call, **Then** a node and a
   timeline entry appear within 2 seconds, marked as running until it ends.
3. **Given** an agent runs several tool calls at the same time, **When** Flujo is shown,
   **Then** each call is its own node (not only the last one).
4. **Given** the agent launched a helper, **When** Flujo is shown, **Then** the helper is
   an agent node linked to its parent, with its own tool calls linked to it.
5. **Given** the user clicks a node or a timeline entry, **When** the details open,
   **Then** they show the tool, the target (cut as captions are), the start time and the
   duration or "running".
6. **Given** the user switches back to Oficina and again to Flujo, **When** the view
   shows, **Then** the last chosen view is kept (one choice for every place on that
   device) until changed.

---

### Edge Cases

- A task has more than one live agent session (for example, a second Claude tab): each
  gets a lead desk on the task's floor and its own root node in Flujo; shell-only
  sessions get neither.
- A project-level agent session (not tied to a task): it gets a floor in the building
  named after the project, and the same office/flow view in its project screen tab.
- More helpers than free desks on a floor: the floor grows with more desk pods; the
  building's miniature shows the first 6 characters of the floor and a "+N".
- More floors than fit on screen: the building scrolls; the waiting counter still counts
  all of them.
- A helper never reports that it finished (crash mid-way): it leaves its desk when its
  agent stops, fails, waits for the user or is paused, as today.
- A helper launches its own helpers: in the office they sit at the same floor as other
  helpers; in Flujo they link to the agent that launched them when that is known, and to
  the lead agent otherwise.
- The history limit is reached: the oldest events drop off the timeline and graph; a note
  says older activity is not shown.
- The daemon restarts: the history is lost (it is kept only in memory); Flujo starts again
  from the next events, and says there is no earlier activity.
- An agent session ends: its history stays viewable in Flujo for the task until the daemon
  restarts or the session's history is replaced by newer events; the office shows no
  character for it.
- An agent started before this feature (no action hooks): office shows its state with the
  thinking pose; Flujo shows only the agent node and a note that no tool calls are
  reported.
- The connection to the daemon drops: office, building and Flujo keep the last picture,
  dimmed, under the reconnecting banner, and refresh (history included) on reconnect.
- The view is hidden (other tab, window minimised): animations and walking stop; they
  resume from the current state when shown.
- A session's usage records cannot be read (missing, unreadable, other agent): its desk
  shows no paper stack and nothing else changes.
- A long or sensitive tool argument: captions, nodes and details show only the short
  prefix already used for captions; nothing beyond what the terminal shows.

## Requirements *(mandatory)*

### Functional Requirements

**Task floor**

- **FR-001**: The task screen (and the project screen for project-level sessions) MUST
  have an **Oficina** tab showing that owner's office floor.
- **FR-002**: The floor MUST show one lead desk per live agent session of the owner and
  one desk per running helper of those agents; desks keep their place until their
  occupant leaves, and a state change never moves a desk.
- **FR-003**: Each desk MUST show its occupant's state on the desk: the monitor glows in
  the colour of the current action's kind (edit, read/search, run, delegate, other; dark
  when there is none), a `?` over an agent that waits for the user, `zzz` over one paused
  by the usage limit, and a caption of the current action (cut at 40 characters).
- **FR-004**: Characters MUST have distinct poses for at least: arriving, sitting idle,
  typing, waiting with a raised hand, sleeping, and walking.
- **FR-005**: When a helper finishes, its character MUST walk from its desk to the floor's
  lounge, stay there about 5 seconds, and leave; its desk MUST become free as soon as
  it stands up.
- **FR-006**: The lead characters of agents of the same project MUST wear the same outfit
  colours, chosen deterministically from the project; helpers keep a look per role.
- **FR-025**: The system MUST know how many tokens each live agent session has used so far
  (new input, output and cache writes, as the agent reports them; re-read cached context
  is not counted, so the tiers measure work done), reflected on screen within 10
  seconds of the agent recording it.
- **FR-026**: Each lead desk MUST show a paper stack that grows with its session's tokens:
  none below 250K, then a small, medium and large stack from 250K, 2M and 16M; the exact
  count MUST be in the desk's hover text and accessible name (for example "1.2M tokens").
- **FR-007**: Clicking a lead character (or focusing it and pressing Enter) MUST open that
  agent's terminal in the task screen.

**Building**

- **FR-008**: The Playground view and its separate window MUST show a building with one
  floor per owner (task or project) that has at least one live agent session, in a stable
  order: a new floor goes on top of the existing ones, and floors never reorder because of
  a state change.
- **FR-009**: Each floor in the building MUST show the task key, title (truncated, full on
  hover), phase, its characters in miniature with their state, and how many of its agents
  wait for the user; floors with a waiting agent MUST be highlighted.
- **FR-010**: The building MUST show the total number of agents waiting for the user.
- **FR-011**: Clicking a floor (or focusing it and pressing Enter) MUST, in the main
  window, open that owner's screen on its Oficina tab; in the separate window, show that
  floor in the window itself (with the Oficina/Flujo toggle and a control back to the
  building) without changing the main window. Clicking a lead character in the separate
  window MUST bring the main window to the front on that agent's terminal.

**Flow view**

- **FR-012**: Wherever the office floor is shown, a toggle MUST switch between Oficina and
  Flujo; the last choice MUST apply to every place where the toggle appears and be
  remembered on that device.
- **FR-013**: The system MUST keep, for each agent session, an in-memory history of its
  recent activity events (tool call started, tool call ended, helper started, helper
  ended, turn over), at most 200 per session, each with its time; it MUST NOT be stored
  across daemon restarts.
- **FR-014**: Flujo MUST show the owner's agents, their tool calls and their helpers as a
  graph (agent → tool calls, agent → helper → its tool calls), built from the history and
  updated live, with every tool call as its own node, including parallel ones.
- **FR-015**: Flujo MUST show a timeline of the same events in time order, with each
  entry's time, kind, tool and target.
- **FR-016**: The user MUST be able to pan and zoom the graph, and select a node or a
  timeline entry to see its details (tool, target, start time, duration or running);
  selecting one MUST highlight the other.
- **FR-017**: Running tool calls MUST be visibly distinct from finished ones, and tool
  call nodes MUST use the same colours per kind as the monitors in the office.
- **FR-027**: The kind of action MUST never be shown by colour alone: the caption (office)
  and label (Flujo) also name it, and the per-kind colours MUST stay distinguishable in
  both themes.
- **FR-018**: When older events have been dropped by the limit, or there are none since
  the daemon started, Flujo MUST say so.

**Art, accessibility and behaviour kept from spec 004**

- **FR-019**: The pixel art MUST be ported from pixtuoid (MIT), with its copyright and
  licence notice in a credits file next to the art; no other third-party art.
- **FR-020**: Every character, floor and flow node MUST have an accessible name (owner,
  role, state, current action or tool), and the graph's events MUST also be reachable as
  a list (the timeline) by keyboard.
- **FR-021**: Office, building and Flujo MUST update within 2 seconds of a change without a
  reload, follow the light and dark themes, use type of at least 12px, and follow the
  reduce-motion setting (still poses, no walking: characters appear at their destination,
  no pulsing).
- **FR-022**: All three MUST stop animating while not visible, and keep the last picture,
  dimmed under a reconnecting banner, while the daemon connection is lost.
- **FR-023**: Captions, node labels and details MUST show at most the short target already
  used for captions (40 characters), never full commands or file contents.
- **FR-024**: The current Playground view (desks grouped by project) is replaced by the
  building; the separate Playground window MUST open on the building.

### Key Entities

- **Office floor**: the place of one owner (task or project): lead desks for its live
  agent sessions, helper desks, a lounge; desks keep their place.
- **Building**: the stack of floors of all owners with a live agent; the Playground's
  content.
- **Activity event**: one thing an agent session did: tool call started or ended (kind,
  tool, short target, background or not), helper started or ended (role), or turn over;
  with its time and, for helpers' events, which helper.
- **Session token usage**: tokens used so far by one agent session (total, and by kind:
  input, output, cache), updated while it works.
- **Session activity history**: the last 200 activity events of one agent session, kept
  in memory only.
- **Flow graph**: agents, helpers and tool calls of one owner as nodes with their links,
  plus the timeline, derived from the histories of the owner's sessions.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: From the task screen, the user reaches the task's floor in one click and can
  tell, without reading text, which of its agents is editing, running a command, waiting
  for them or asleep.
- **SC-002**: From the Playground, the user reaches the floor of an agent that waits for
  them in one click, with 6 floors on screen.
- **SC-003**: State changes appear in office, building and Flujo within 2 seconds.
- **SC-004**: Opening Flujo on a task whose agent has been working for a while shows its
  last 200 events per session, not an empty graph.
- **SC-005**: With reduce motion on, nothing on the three views moves; with the view
  hidden, it uses no animation time.
- **SC-006**: The user, shown the new floor and building next to the current Playground,
  prefers the new look (the reason for the redesign).
- **SC-007**: A screen-reader user can learn each agent's state and action from the floor,
  and each event from the timeline.

## Assumptions

- Only Claude Code sessions report activity (the only adapter); shell sessions get no desk
  and no node.
- The art is a port of pixtuoid's sprites to the app's existing pixel-map approach
  (decision #79 stays: DOM/SVG, not a canvas); only the subset needed is ported:
  characters (seated, typing, standing, walking, waiting, sleeping) with a few hairstyles
  and outfits recoloured per project, desks with monitors, a lounge corner, plants.
- Tokens per session are read from what the agent itself records about its usage (Claude
  Code's session transcript), not estimated; a session whose usage cannot be read shows no
  stack. Helpers' tokens count towards their agent's session when the agent records them
  there.
- Ambient extras (day/night skyline, weather, pet, sound) are out of scope.
- The file heatmap of agent-flow is out of scope.
- There is no separate "idle" state: an agent that finished its turn is waiting for the
  user (`?`), as everywhere else in Delilah; `zzz` is for paused by the usage limit.
- Decision #80 is revised: activity stays in memory and on its own topic, but now keeps a
  short per-session history besides the current state.
- An ended session's history may stay in memory (at most the 50 most recently ended
  sessions) so Flujo can still show it for
  its task until the daemon restarts; the office shows only live sessions.
- Walking follows simple paths on the floor (desk ↔ lounge); full pathfinding around
  furniture is not required.
