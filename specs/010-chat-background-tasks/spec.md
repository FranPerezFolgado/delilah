# Feature Specification: Show background work in chat mode

**Feature Branch**: `010-chat-background-tasks`

**Created**: 2026-10-26

**Status**: Draft

**Input**: Task LIBRE-21 "Chat mode background tasks". User note: "Chat mode is working in the background and doesn't show anything, so it looks frozen." Chat mode (spec `specs/006-chat-view`) runs a task's Claude on a structured stream instead of a terminal. While Claude works through sub-agents or background work (background commands, background sub-agents), the chat view shows nothing, and when a turn ends with background work still running the task looks idle or "Te espera" although Claude will carry on by itself. Scope: chat-mode task sessions; terminal (PTY) sessions only where it comes for free. UI text in Spanish, like the rest of the app.

## Why

In chat mode the conversation is the only window onto the agent: there is no terminal to
glance at. Today the conversation only moves when Claude writes text or a tool finishes.
Two common situations leave it still for minutes:

- **Sub-agents.** Claude hands a piece of work to a sub-agent (a search, a review, an
  implementation package). The sub-agent may run dozens of tools, but the conversation only
  shows one collapsed card that stays the same until the sub-agent finishes. Its inner steps
  are not shown at all.
- **Background work.** Claude starts a long command or a sub-agent in the background and
  ends its turn ("I'll wait for the results"). The task then looks finished or waiting for
  the user, with no sign that anything is still running. When the background work ends,
  Claude starts a new turn by itself, which the task does not treat as work either.

The activity line that exists today is only updated when a tool *finishes*, so during a
long tool it shows the previous step, or nothing. The result is the user's report: the
agent looks frozen, they cannot tell a stuck agent from a busy one, and they may stop or
switch a task that was working fine.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - See that the agent is alive and what it is doing (Priority: P1)

While a chat-mode session works, the bottom of the conversation always shows a "working"
indicator: an animated mark, how long the current turn has been running, and the current
step in plain words ("Ejecutando cargo test", "Leyendo src/main.rs", "Subagente: Revisar el
plan"). The step changes as soon as a tool *starts*, not when it ends, so a long command is
named while it runs. If nothing has happened for a while, the indicator says how long ago
the last sign of life was, so a stuck agent can be told apart from a slow one.

**Why this priority**: it is the direct fix for "it looks frozen". It is useful on its own
for every chat-mode turn, with or without sub-agents or background work.

**Independent Test**: in a chat-mode task, ask for something that runs a slow command
(e.g. a 60-second sleep). While it runs, the indicator shows the command and a running
clock; when it ends, the next step replaces it; when the turn ends, the indicator goes away.

**Acceptance Scenarios**:

1. **Given** a chat-mode session that starts a turn, **When** the user looks at the
   conversation, **Then** a working indicator with an elapsed-time clock is visible at the
   bottom within 2 seconds, even before Claude writes anything.
2. **Given** Claude starts a tool, **When** the tool is still running, **Then** the
   indicator names that tool and its subject (command, file, pattern, sub-agent description).
3. **Given** no new stream activity for 30 seconds, **When** the user looks at the
   indicator, **Then** it also shows "sin novedades desde hace N s/min" (time since the
   last sign of life), updated live.
4. **Given** the turn ends (successfully, with an error, or by "Detener"), **When** the
   result arrives, **Then** the indicator disappears (or switches to the background
   indicator of US3 if background work is still running).
5. **Given** the user opens or reloads the conversation mid-turn, **When** it loads,
   **Then** the indicator shows the current step and the elapsed time of the current turn,
   not a reset clock.

---

### User Story 2 - Follow a sub-agent from its card (Priority: P1)

When Claude runs a sub-agent, its card in the conversation shows that it is running, for how
long, how many steps it has done, and its latest step ("Leyendo crates/x.rs"), updated live.
Expanding the card lists the sub-agent's steps so far (tool and subject, one line each, the
latest last). When the sub-agent finishes, the card shows it as done (or failed) with its
step count and duration, and its result as today. Several sub-agents running at once each
update their own card.

**Why this priority**: sub-agents are where most of the silent minutes come from in
orchestrated work (the workflows Delilah itself runs launch many of them).

**Independent Test**: in a chat-mode task, ask Claude to use a sub-agent to explore the
repo. While it runs, its card shows a live step count and latest step; expanding it lists
the steps; when it ends, the card shows "Hecho · N pasos · M min".

**Acceptance Scenarios**:

1. **Given** a running sub-agent, **When** it starts a tool, **Then** its card's latest step
   and step count update within 2 seconds.
2. **Given** two sub-agents running in parallel, **When** each one works, **Then** each card
   shows only its own steps.
3. **Given** a running sub-agent's card, **When** the user expands it, **Then** they see the
   list of its steps so far (at most the last 50, with a count of earlier ones hidden).
4. **Given** a sub-agent finishes, **When** its result arrives, **Then** the card shows done
   or failed, the step count and the duration.
5. **Given** the user reloads the conversation while a sub-agent runs, **When** it loads,
   **Then** the card shows the sub-agent as running with its steps so far.

---

### User Story 3 - See background work still running after the turn ends (Priority: P1)

When Claude's turn ends while background work it started is still running (a background
command or a background sub-agent), the conversation shows a "En segundo plano" strip above
the text field with one row per item: what it is (description or command), how long it has
been running, and its state (en curso, terminado, fallido, detenido). The task is shown as
working, not as waiting for the user, and no "Claude has finished" notice or Bandeja item
is raised for that turn. When the work ends and Claude carries on by itself, the new turn
appears in the conversation like any other, with the working indicator of US1. Finished
items leave the strip once Claude's follow-up turn starts.

**Why this priority**: this is the case where the task looks finished or waiting when it is
not, which leads the user to interrupt or switch a healthy task, or to wait on a task that
is really waiting for them.

**Independent Test**: in a chat-mode task, ask Claude to run a 2-minute command in the
background and wait for it. The turn ends; the strip shows the command running with a clock;
the task stays "working" in the board and sidebar; no Bandeja item appears. When the command
ends, Claude's follow-up turn appears and the task ends in its normal state.

**Acceptance Scenarios**:

1. **Given** Claude ends a turn with background work running, **When** the turn ends,
   **Then** the strip lists each running item, and the session and task stay "working".
2. **Given** that state, **When** the background work ends, **Then** its row changes to
   terminado/fallido and Claude's own follow-up turn shows with the working indicator.
3. **Given** Claude ends a turn and **no** background work is running, **When** the turn
   ends, **Then** behaviour is as today (waiting for the user, Bandeja, notices).
4. **Given** background work is running, **When** the user sends a message, **Then** it is
   delivered as for an idle session (Claude takes it now); the strip stays.
5. **Given** background work is running, **When** the user presses "Detener", **Then** the
   current turn (if any) is interrupted and the background items are asked to stop too;
   rows that do not confirm show "estado desconocido" once the session ends.
6. **Given** the agent process exits while background items are listed, **When** it exits,
   **Then** every item still "en curso" is shown as "detenido" and the task leaves the
   working state as it does today on exit.
7. **Given** background work runs for a long time, **When** autopilot or saving mode watch
   the task, **Then** they wait as they would for a working turn (no phase boundary, no
   "carry on" prompt, no review hint) until Claude's follow-up turn ends.

---

### User Story 4 - A turn Claude starts by itself is work (Priority: P2)

When Claude starts a turn without a user message (after background work ends, or any other
self-started turn), the session and task move to "working" at once, as for a typed message,
and back to their normal end-of-turn state when it ends. The history and the conversation
make clear the turn was started by Claude, not by the user ("Claude continúa: tarea en
segundo plano terminada").

**Why this priority**: it is required by US3 to end correctly, and on its own it fixes a
task that stays "Te espera" while Claude is actually working.

**Independent Test**: produce a self-started turn (as in US3's test). The task goes to
"working" when the turn starts, without any message from the user, and to its normal state
when the turn ends.

**Acceptance Scenarios**:

1. **Given** a session waiting for the user, **When** Claude starts a turn by itself,
   **Then** the session and task become "working" within 2 seconds, and any Bandeja
   question raised by the previous turn's end is resolved as no longer needed.
2. **Given** a self-started turn, **When** it ends, **Then** the usual end-of-turn effects
   apply (waiting for the user, Bandeja, autopilot, saving mode), unless background work
   is still running (US3).

---

### Edge Cases

- A sub-agent that itself starts sub-agents: inner steps are attributed to the outermost
  sub-agent card that the conversation shows (no nested cards).
- Background work started in one turn and still running when the user switches the task
  to the terminal: the switch is refused as for a working session ("espera a que termine
  o pulsa Detener").
- A usage-limit pause while background work runs: the pause wins; the strip keeps the rows
  and marks them "detenido" if the process exits.
- Events about a background item Delilah never saw start (after a daemon restart): the row
  appears from the first event seen, with an unknown start time ("desde hace ?").
- A daemon restart: in-memory progress (steps, strip) is lost; the conversation rebuilt from
  the transcript still shows sub-agent cards with their final results. Items running in a
  process that did not survive the restart are not listed.
- Claude versions that emit none of the background or sub-agent signals: the working
  indicator of US1 still works from tool starts and turn boundaries; the strip and live
  sub-agent steps simply stay empty, never wrong.
- Many events per second (a sub-agent running fast tools): the view updates at most a few
  times per second and never drops the turn's own text.
- Terminal (PTY) sessions: US1's "step at tool start" also applies to their chat view where
  the same signal is available; US2–US4 are chat mode only.

## Requirements *(mandatory)*

### Functional Requirements

**Working indicator (US1)**

- **FR-001**: While a chat-mode session is in a turn, the conversation MUST show a working
  indicator with the time since the turn started, visible from the turn's start.
- **FR-002**: The indicator MUST name the current step from the moment a tool starts
  (tool and subject: command, file, pattern, sub-agent description), and keep the last
  named step while Claude only writes or thinks.
- **FR-003**: When no sign of life (text, tool start or end, sub-agent step, status event)
  has arrived for 30 seconds, the indicator MUST show the time since the last one.
- **FR-004**: The turn start time, the current step and the time of the last sign of life
  MUST be available when the conversation is (re)loaded mid-turn.

**Sub-agents (US2)**

- **FR-005**: For each running sub-agent, the system MUST keep its steps (tool and subject),
  step count, start time and current state (en curso, hecho, fallido) while the session is live.
- **FR-006**: The sub-agent card MUST show state, elapsed time, step count and latest step,
  updated live; expanded, it MUST list the last 50 steps and how many earlier ones are hidden.
- **FR-007**: Steps MUST be attributed to the sub-agent that ran them; parallel sub-agents
  MUST not mix.

**Background work (US3)**

- **FR-008**: The system MUST track background items started by the agent (background
  commands and background sub-agents): id, kind, description, start time, state (en curso,
  terminado, fallido, detenido, desconocido) and end time.
- **FR-009**: While at least one background item is running, a turn end MUST NOT be treated
  as the agent waiting for the user: the session and task stay working; no end-of-turn
  Bandeja question, notice, review hint, autopilot step or saving-mode phase boundary is
  triggered for it.
- **FR-010**: The conversation MUST show the "En segundo plano" strip while any item is
  listed, with one row per item (description, elapsed or total time, state). Finished items
  MUST leave the strip when Claude's next turn starts, or 60 seconds after they end if none
  starts.
- **FR-011**: "Detener" MUST interrupt the current turn and ask the agent to stop its
  background items; when the process exits, items still running MUST become "detenido".
- **FR-012**: If the last background item ends and Claude does not start a follow-up turn
  within 30 seconds, the session MUST fall back to the normal end-of-turn behaviour
  (waiting for the user, Bandeja, autopilot), so a task can never stay "working" forever
  on background work that is gone.
- **FR-013**: Switching the task's runtime MUST be refused while background items run, as
  for a working session.

**Self-started turns (US4)**

- **FR-014**: A turn that starts without a user message MUST put the session and task into
  "working", with the same effects as a typed message's turn start (inbox items of the
  previous turn end resolved, task back to running), and be recorded in the task history
  as started by Claude.
- **FR-015**: The conversation MUST mark a self-started turn with a short divider naming
  why, when known ("tarea en segundo plano terminada").

**General**

- **FR-016**: All progress data in this feature is live state only: it MUST NOT require new
  persisted data, and losing it on a daemon restart MUST leave the conversation correct
  (built from the transcript, as today).
- **FR-017**: Signals the system does not recognise MUST be ignored without breaking the
  conversation or the live text; the absence of background or sub-agent signals MUST leave
  the US1 indicator working.
- **FR-018**: Live updates MUST be throttled so the view refreshes at most about 4 times per
  second per session, without losing the latest state.
- **FR-019**: All new UI text MUST be in Spanish and go through the app's translation
  function, like the rest of the UI.

### Key Entities

- **Turn progress**: per live session; when the current turn started, whether Claude or the
  user started it (and why), the current step, the time of the last sign of life.
- **Sub-agent progress**: per running sub-agent of a session; the tool call it belongs to,
  description, state, start and end time, step count, the last 50 steps (tool, subject, time).
- **Background item**: per session; id, kind (command, sub-agent, other), description,
  state, start and end time, the tool call that started it when known.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: During any chat-mode turn, the conversation shows a moving sign of activity
  (clock or step) at all times; a 5-minute sub-agent or command never leaves the view
  unchanged for more than 1 second of clock time.
- **SC-002**: A tool, sub-agent step or background item state change is visible in the
  conversation within 2 seconds of the agent reporting it.
- **SC-003**: In 10 out of 10 runs of the US3 test, the task is never shown as waiting for
  the user (board, sidebar, Bandeja) while background work is running, and ends in the
  correct state after Claude's follow-up turn.
- **SC-004**: In the US3 and US4 tests, no Bandeja item or notice is raised by a turn that
  ended with background work running.
- **SC-005**: With background or sub-agent signals absent (older Claude), every existing
  chat-mode test still passes and the working indicator still shows.

## Assumptions

- The agent's structured stream reports tool starts, sub-agent messages (tagged with the
  tool call they belong to) and background work start, progress and end. The exact shapes
  of the background signals were not observed in the chat-mode spike (2026-10-24) and are
  checked by a short capture before building on them; whatever is missing falls back to
  FR-012 and FR-017.
- Claude starts a follow-up turn on its own when background work it waits for ends, in the
  structured-stream runtime as in the terminal.
- Sub-agent inner steps are shown as one-line steps (tool and subject), not as full nested
  conversations with text and tool results; that stays a known limit, as thinking blocks do.
- The idea conversation and project sessions keep running in a terminal (out of scope, as in
  `specs/006-chat-view`); in terminal sessions this feature only improves the chat view's
  activity line where the same signal exists.
- No new persisted data, no migration; the HTTP API and the live socket get additive fields
  and messages only, so older clients keep working.
- Thresholds (30 s "sin novedades", 30 s fallback, 60 s to clear finished rows, 50 steps,
  ~4 updates/s) are defaults that can be tuned during implementation.
