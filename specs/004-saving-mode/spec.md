# Feature Specification: Saving mode (one fresh session per phase)

**Feature Branch**: `feature/LIBRE-5`
**Created**: 2026-10-21
**Status**: Draft
**Input**: User description: "Orchestrate says it's cheaper to continue in a new session; can Delilah manage that? Main Claude does the spec, plan and tasks (heavy context); Claude 2 implements from tasks.md; Claude 3 runs the review and commits. The more we split, the less we spend. It must be clear in the UI how it works. Maybe a 'saving mode' chosen when a task is created: pointless on the work account, useful on the personal one with a much lower limit. I like running /save-session; closing sessions must not lose that."

## Why

Every turn of a Claude session re-reads the whole conversation so far. Measured on
a real task (LIBRE-3, 153 turns): the context per turn was ~44k tokens at the start,
~141k once the plan was written, ~175k when `tasks.md` was ready and ~364k at the
end, for ~33M context tokens re-read against 130k tokens written. Starting a fresh
session at the phase boundaries, where everything the next phase needs is already
in the spec files and git, would have re-read an estimated 37 % less (one cut:
planning → implementation), 49 % less (two cuts: + before the final review) and
60 % less (cutting also between implementation phases). Each new session pays its
own base (~45–60k), so the returns diminish: only cut where the state lives in files.

On an account with a low usage limit this is the difference between finishing a
feature in one window or being paused for hours; on an account with a high limit it
only adds handoffs, so it is opt-in per task.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Implement in a fresh session after planning (Priority: P1)

The user creates a task with saving mode on. The first session ("Spec y plan") runs
the usual spec workflow with the user's approvals, and stops once the tasks are
written instead of implementing them. Delilah then starts a second session
("Implementación"), makes it the task's main session and gives it the instruction to
implement from the spec files. The first session stays in the task as a finished,
reopenable tab.

**Why this priority**: It is the cut that saves the most (~37 % on the measured
task) and on its own already delivers the value.

**Independent Test**: Create a saving-mode task on a fixture project, take it
through specify/plan/tasks, trigger the handoff, and check that a new session
starts implementing from `specs/<feature>/` while the first one is kept, stopped,
with its conversation intact.

**Acceptance Scenarios**:

1. **Given** a saving-mode task whose first session has just written `tasks.md`, **When** that session's turn ends, **Then** the Bandeja and the task screen offer "Implementar en sesión nueva".
2. **Given** that offer, **When** the user confirms it, **Then** a new session labelled "Implementación" starts in the same workspace and branch with the implementation instruction for that feature, and it becomes the session whose turns drive the task's status.
3. **Given** the handoff happened, **When** the user opens the task's sessions, **Then** "Spec y plan" is still listed, marked finished, and can be reopened with its conversation.
4. **Given** a task without saving mode, **When** `tasks.md` is written, **Then** nothing changes from today's behaviour.

### User Story 2 - See the chain of sessions (Priority: P1)

On a saving-mode task, the user can see at a glance which sessions the task goes
through, which one is working now and which are finished or still to come, and that
the task is in saving mode.

**Why this priority**: The user said the main obstacle is not knowing visually how
this would work; without it the handoffs look like sessions appearing and vanishing.

**Independent Test**: Open a saving-mode task in each phase and check the chain and
the marker in the task screen.

**Acceptance Scenarios**:

1. **Given** a saving-mode task in planning, **When** the user opens it, **Then** the sessions area shows the chain "Spec y plan → Implementación → Revisión" with the first one active and the rest pending, and the header shows a "Modo ahorro" marker.
2. **Given** a handoff has happened, **When** the user looks at the chain, **Then** finished sessions are marked done, the current one active, and the terminal shown by default is the active session's.
3. **Given** a task without saving mode, **When** the user opens it, **Then** no chain or marker is shown.

### User Story 3 - Keep the session notes before closing a session (Priority: P2)

Before a session of the chain is left behind, Delilah asks it to save its session
notes (the user's `/save-session`), so the reasoning that only lives in that
conversation reaches the project's notes. This can be turned off.

**Why this priority**: The spec files carry what was decided; the notes carry the
discussion around it. The user relies on them, but the chain works without them.

**Independent Test**: With the option on, trigger a handoff and check that the
outgoing session receives the save instruction and that the new session only starts
after that turn has ended; with it off, check the handoff goes straight on.

**Acceptance Scenarios**:

1. **Given** the option is on, **When** a handoff starts, **Then** the outgoing session is asked to save its session notes, and the next session starts only once that turn has ended.
2. **Given** the save turn fails or the session asks something, **When** that happens, **Then** the handoff waits for the user (it shows in the Bandeja) instead of going ahead or losing the request.
3. **Given** the option is off, **When** a handoff starts, **Then** the next session starts without the save step.

### User Story 4 - Review and commit in a third session (Priority: P2)

When every item in `tasks.md` is checked, Delilah hands the task to a third session
("Revisión") that reviews the branch and commits the result, again starting fresh.

**Why this priority**: The second cut adds ~12 more points of saving; the first cut
already gives most of the value.

**Independent Test**: Take a saving-mode task to all tasks checked and check that
the review session starts with the review instruction and the task moves to review.

**Acceptance Scenarios**:

1. **Given** the implementation session checked the last task and its turn ended, **When** that happens, without asking the user, **Then** a "Revisión" session starts with the review instruction and becomes the main session.
2. **Given** the review session, **When** it runs, **Then** it reviews the branch, applies the confirmed findings, updates the docs the change made stale and commits, but never pushes or opens a PR (those stay with the user).

### Edge Cases

- **Usage limit during a handoff**: a limit hit while saving notes or starting the next session pauses the task as today; after the reset the handoff resumes where it stopped (no duplicate session, no lost instruction).
- **The first session keeps going past tasks**: if it starts implementing anyway (code edits appear), the handoff is not offered for that phase; the task simply continues in that session.
- **User reopens a finished session** of the chain to ask something: it runs as an extra session and never takes back the main role or changes the task's status.
- **Daemon restart mid-handoff**: on start, a task whose handoff was under way either finishes it or shows it as pending in the Bandeja; it never ends up with two main sessions or none.
- **No spec files** (a task whose agent never writes `specs/<feature>/`): there is never a handoff; saving mode has no effect.
- **The user moves the phase by hand** (phase bar): moving forward to implement or review offers the handoff the same way; moving backwards never creates or removes sessions.
- **Saving mode changed after start**: turning it on for a running task applies from the next phase boundary; turning it off stops further handoffs and keeps the sessions already made.
- **Several boundaries crossed at once** (e.g. tasks all checked before the user acted on the first handoff): only one handoff runs, to the furthest phase reached.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: A task MUST have a saving-mode setting, chosen when the task is created (also when saved to the backlog) and visible on the task afterwards.
- **FR-002**: The default for new tasks MUST come from a user preference, so each installation (work Mac off, personal Mac on) gets the right default without choosing every time.
- **FR-003**: When a saving-mode task starts, its first session MUST be labelled "Spec y plan" and told that, in saving mode, it should stop after the tasks are written and approved instead of implementing them.
- **FR-004**: Delilah MUST detect the boundary planning → implementation (tasks written, none checked, first session's turn ended) and the boundary implementation → review (every task checked, the session's turn ended).
- **FR-005**: The planning → implementation handoff MUST wait for the user to confirm it from the Bandeja or the task screen ("Implementar en sesión nueva"); that confirmation also approves the tasks. The implementation → review handoff MUST start on its own.
- **FR-006**: A handoff MUST start a new session in the same workspace and branch, make it the task's main session (the one whose turns drive the task's status, questions and limits), and stop the previous one, keeping it listed, finished and reopenable.
- **FR-007**: The implementation session MUST be started with the instruction to implement the feature from its spec folder; the review session with the review instruction for the branch. Both instructions are in the user's language.
- **FR-008**: When the save-notes option is on, before a handoff Delilah MUST ask the outgoing session to save its session notes and wait for that turn to end; the option MUST be a user preference, on by default.
- **FR-009**: The chain of a saving-mode task (its phases, which are done, active, pending) and a saving-mode marker MUST be shown in the task screen; tasks without saving mode show neither.
- **FR-010**: Usage-limit pauses, automatic resume after a reset, intercepted questions and the Bandeja MUST keep working with whichever session is the main one at the time.
- **FR-011**: A handoff MUST be safe to retry: a restart or a pause in the middle never leaves a task with zero or two main sessions, nor starts the same phase's session twice.
- **FR-012**: The handoffs MUST be recorded in the task's history (which phase, which session, when).
- **FR-013**: Tasks without saving mode MUST behave exactly as today.

### Key Entities

- **Task saving mode**: on/off per task; its default comes from a preference.
- **Chain step**: one of the fixed steps of a saving-mode task — "Spec y plan", "Implementación", "Revisión" — each tied to the phase where it starts and, once started, to the session that runs it; state pending, active or done.
- **Save-notes preference**: on/off, whether the outgoing session saves its notes before a handoff.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: On a feature comparable to LIBRE-3, a saving-mode task re-reads at least 30 % fewer context tokens than the same work in one session (measured from the session transcripts).
- **SC-002**: A handoff, from the boundary being reached (or confirmed) to the new session working, takes under 1 minute of wall time plus the save-notes turn.
- **SC-003**: In a user check, someone opening a saving-mode task can say which session is working and what comes next without opening any session.
- **SC-004**: Zero lost work across handoffs: no uncommitted change, spec file or session conversation is lost, and every finished session can still be reopened.
- **SC-005**: Tasks without saving mode show no behaviour change (existing tests unchanged and passing).

## Out of Scope

- Cutting between implementation phases (one session per orchestrate phase) — a later iteration once the two cuts prove themselves.
- Going back to an earlier session of the chain as the main one ("return to the parent"): it reloads that session's large context, which is what saving mode avoids.
- Using another agent or account per step, budgets per task, and general declarative workflows (vision.md).

## Assumptions

- Tasks run the user's `/feature` workflow and its implementation runs with `/orchestrate`, which already resumes from the spec folder (`spec.md`, `plan.md`, `tasks.md`, `packages.md`, `progress.md`); the spec folder is committed on the branch, so the next session has everything it needs from files and git.
- "Save session notes" means the user's own `/save-session` command; on a machine without it, the option has no effect beyond the instruction being ignored.
- The chain has three fixed steps; more steps are a later iteration.
- This relaxes decision 15 ("Delilah never drives SpecKit's phases"): Delilah still never drives what happens inside a phase, it only starts a new session between phases. A new decision entry records this.
