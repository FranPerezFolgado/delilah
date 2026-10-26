# Feature Specification: Autopilot doesn't pause while the agent waits on background work

**Feature Branch**: `008-autopilot-background-stall`

**Created**: 2026-10-26

**Status**: Draft

**Input**: User description: "Bug in autopilot: when spec and plan closes to give way to Implement, the header shows autopilot as paused and it isn't true. We need tests that work like the real thing." (task LIBRE-17)

## Context

On autopilot, a step's session that ends its turn without reaching the step's end is a
**stall** (decision #39): the first stall of a step gets one typed nudge, the second
pauses the autopilot with the step's reason ("tasks.md not written", "N tasks
unchecked").

The workflows that autopilot drives (`/feature` for plan, `/orchestrate` for implement)
end their turn **on purpose** while background subagents run (the plan reviewer, the
task slicer, the implementers), and resume by themselves when those report back. The
agent tells the daemon so: its end-of-turn report lists the background tasks still
running. The daemon ignores that list, so every such wait counts as a stall.

Observed in LIBRE-15: the plan session paused with "tasks.md not written" four seconds
after its nudge while its architecture reviewer was still running, then went on to write
the tasks and start implementing while the header said "Autopilot paused". The implement
step paused three more times the same way ("48 / 40 / 12 tasks unchecked") with
implementers running every time. Each pause needed a manual "Resume autopilot".

Ruled out: the previous session being stopped at the plan → implement handoff. Once the
handoff records the new session as the main one, the old session's last report no longer
counts as the task's.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Autopilot keeps running while the agent waits on its subagents (Priority: P1)

A user launches a task on autopilot. The plan session runs `/feature`, starts a reviewer
in the background and ends its turn to wait for it. Later the implement session runs
`/orchestrate` and ends its turns while implementers run. The autopilot stays "running"
the whole time: no nudge is typed into the session, no pause, no "autopilot paused"
banner, notification or Bandeja item. When the background work is done and the agent
reaches the step's end, the chain goes on to the next step as it does today.

**Why this priority**: This is the bug. Today an autopilot task that uses subagents
cannot get from plan to PR without the user resuming it several times, and the UI says
something false about the task.

**Independent Test**: Run a task on autopilot with a scripted agent whose turns end with
background tasks running, then end with the step done; the autopilot never leaves
"running", no nudge is recorded, and the next step starts.

**Acceptance Scenarios**:

1. **Given** a task on autopilot in the plan step, **When** the session's turn ends with at least one background task still running and tasks.md not written, **Then** the autopilot stays running, no nudge is typed, and nothing is recorded as a stall.
2. **Given** that same wait repeated over several turns, **When** each turn ends with background tasks running, **Then** the autopilot still stays running (waiting doesn't use up the step's one nudge).
3. **Given** a task on autopilot in the implement step with unchecked tasks, **When** a turn ends with implementers running in the background, **Then** the autopilot stays running and the header shows it running.
4. **Given** the plan step's background work has finished, **When** the session ends its turn with spec, plan and tasks written, **Then** the handoff to implement happens as today and the autopilot stays running through it.

---

### User Story 2 - A real stall is still caught (Priority: P1)

A session that really stops (its turn ends with no background work running and the step
isn't done) is treated exactly as today: one nudge, then a pause with the step's reason.

**Why this priority**: The fix must not turn autopilot into something that waits forever
on a stuck agent; the nudge-then-pause rule is what makes autopilot safe to leave alone.

**Independent Test**: Run a scripted agent whose turns end with no background tasks and
the step unfinished; the first turn end nudges, the second pauses with the step's reason.

**Acceptance Scenarios**:

1. **Given** a task on autopilot in the implement step, **When** a turn ends with no background tasks running and tasks unchecked, **Then** the session gets the step's one nudge.
2. **Given** the step was already nudged, **When** a later turn ends with no background tasks and the step still unfinished, **Then** the autopilot pauses with "N tasks unchecked".
3. **Given** a session that waited on background work for several turns, **When** its first turn with nothing running in the background ends with the step unfinished, **Then** it gets its nudge (not a pause), since the waits didn't count.
4. **Given** the end-of-turn report lists background tasks but none of them is running (all finished or failed), **When** the turn ends with the step unfinished, **Then** it counts as a stall as in scenarios 1–2.
5. **Given** an agent version whose end-of-turn report carries no background-task list at all, **When** the turn ends with the step unfinished, **Then** it counts as a stall (today's behaviour).

---

### User Story 3 - Tests that reproduce the real run (Priority: P2)

A developer changing autopilot can rely on tests that drive the daemon the way a real
agent does: the scripted fake agent sends the same end-of-turn reports Claude Code
sends (shapes taken from a real hook log, background-task list included), through the
same entry point, for a whole plan → implement run on autopilot. A regression like this
one fails a test instead of being found in daily use.

**Why this priority**: The existing autopilot tests build the task state by hand and call
the stall check directly with no agent report at all, which is why they couldn't see
this. The user asked explicitly for tests that work like the real thing.

**Independent Test**: The new tests fail on the current code (the false pause appears)
and pass with the fix.

**Acceptance Scenarios**:

1. **Given** the end-to-end test environment (isolated daemon plus fake agent), **When** a scripted autopilot run goes plan → implement with background waits in both steps, **Then** the test asserts the autopilot never paused, no nudge was recorded, and the implement step started.
2. **Given** the same environment, **When** a scripted run really stalls (no background work, step unfinished, twice), **Then** the test asserts one nudge and then a pause with the step's reason.
3. **Given** the daemon's own tests, **When** a turn-end report is fed through the hook entry point with a recorded payload shape, **Then** the stall decision is checked against that payload (running, finished and missing background-task lists).
4. **Given** the fixtures, **When** they are reviewed, **Then** they hold only the payload shape (event name, background-task entries with id, type, agent type, short description, status) with neutral made-up values: no prompts, messages, paths or content from any real task or repo.

### Edge Cases

- A background task that never finishes: the autopilot keeps waiting, the same as a session that keeps working. No time cap is added (a known limit of autopilot, docs/status.md).
- Background tasks of a non-subagent kind (e.g. a background shell command) that are running: they count as background work too, since the agent will be woken when they end.
- A turn that fails (an error, not an ordinary turn end) with background tasks running: unchanged, a failed turn still pauses as today.
- A turn end of a session that is not the task's main one (e.g. a review session or a side chat): unchanged, it never drove the stall check.
- Two turn ends close together, one with background work and one without: only the one without can take the nudge, and only once (taking the nudge stays atomic).
- A task not on autopilot: nothing changes for it.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The daemon MUST read, from the agent's end-of-turn report, whether any background task is still running.
- **FR-002**: On autopilot, a turn end of the step's main session with at least one background task running MUST NOT count as a stall: no nudge, no pause, no change to the step's nudge, nothing recorded.
- **FR-003**: A turn end with no background task running (none listed, all finished or failed, or no list in the report) MUST be checked for a stall exactly as today (decision #39: one nudge per step, then a pause with the step's reason).
- **FR-004**: The step-end check (spec/plan/tasks written → hand over to implement; all tasks checked → next step) MUST run on every turn end as today, whether or not background tasks are running.
- **FR-005**: A failed turn, a usage-limit stop, and turn ends of non-main sessions MUST behave as today.
- **FR-006**: The scripted fake agent used by the end-to-end tests MUST be able to end a turn with a list of background tasks, in the same shape as Claude Code's end-of-turn report.
- **FR-007**: There MUST be end-to-end tests (isolated daemon + fake agent) covering: an autopilot plan → implement run with background waits in both steps and no pause; and a real stall giving one nudge then a pause.
- **FR-008**: There MUST be daemon tests that feed recorded end-of-turn payload shapes through the hook entry point and check the stall decision for: background task running, all finished, list absent.
- **FR-009**: Test fixtures MUST NOT contain prompts, assistant messages, paths, ids or any content copied from a real task or work repo; only the payload's field names and neutral values.
- **FR-010**: docs/decisions.md (decision #39) and docs/status.md MUST say that waiting on background work is not a stall.

### Key Entities

- **End-of-turn report**: what the agent posts when its turn ends; carries a list of background tasks, each with an id, a kind (subagent, shell…), what it is, and a status (running, completed, failed…).
- **Stall**: a turn end of the step's main session, on autopilot, with the step unfinished and no background work running.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: An autopilot run whose agent waits on background subagents in the plan and implement steps reaches the review step with 0 pauses and 0 nudges caused by those waits.
- **SC-002**: A session that really stops still gets exactly 1 nudge and then 1 pause per step, as before.
- **SC-003**: At least one new end-to-end test fails on the code before the fix (it reproduces the false "autopilot paused") and passes after it.
- **SC-004**: The header and the Bandeja never show "autopilot paused" for a task whose session is waiting on running background work.

## Assumptions

- The background-task list in Claude Code's end-of-turn report is the signal to trust (seen in every turn end of LIBRE-15's log, with status "running" while subagents ran and an empty list when none did). Agents or versions that don't send it keep today's behaviour.
- When the background work finishes, Claude Code wakes the session by itself and starts a new turn, so the daemon doesn't need to do anything to continue.
- The UI needs no change: it already shows the autopilot state the daemon reports; fixing the daemon fixes the header.

## Out of scope

- Showing in the UI that a session is waiting on N background tasks.
- How a non-autopilot task's "waiting for you" state treats a turn end with background work running (the task may still show as waiting for the user while subagents run). Worth its own task if it bothers.
- Time or token caps for autopilot.
