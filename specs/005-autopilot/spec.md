# Feature Specification: Autopilot (a task that runs on its own up to the PR)

**Feature Branch**: `feature/LIBRE-9`
**Created**: 2026-10-22
**Status**: Draft
**Input**: Task brief LIBRE-9 (written by Claude in the «Idea» conversation, 2026-10-22): "Some tasks have a very simple idea and could run on their own to the end. Add an automatic mode that does the whole SpecKit in md on its branch, orchestrates until done, starts the preview and creates the PR." Summary: a per-task **autopilot** flag that extends saving mode (plan → implementation → review, which today stops to ask for confirmation), adds a preview and the PR, confirms the handoffs by itself, and pauses into the Bandeja when the agent gets stuck.

## Why

Saving mode already splits a task into one fresh session per step (spec and plan,
implementation, review), but the user still has to approve each spec phase, confirm the
planning → implementation handoff, open the PR by hand and start a preview to try it.
For small, well-understood tasks (a clear idea, a few files) those stops are the only
reason the user has to come back to the task before it is ready for review. Autopilot
removes them: the user writes the idea, turns autopilot on, and comes back to an open
PR with a preview running, or to a single Bandeja item telling them where and why the
task stopped.

Autopilot does not change what happens inside a step (Delilah still never drives
SpecKit's phases, decision #15 as relaxed by #33): it only tells each step's session up
front to run without asking, confirms the handoffs itself, and adds two steps at the end
that Delilah already knows how to do (open a PR, which enters the PR loop) or that it
learns here (start a preview).

## Clarifications

### Session 2026-10-22

- Q: When an autopilot step's agent asks a question, what happens? → A: Delilah answers with the option marked as recommended and goes on (logged in history); it pauses only when there is none.
- Q: Who starts and owns the preview? → A: The daemon, as a long-lived process per task, stopped with the task.
- Q: Is the PR a draft or ready for review? → A: A draft.
- Q: Where does a project's preview command come from? → A: A per-project setting in Delilah, empty by default (no file in the repo).
- Q: Can the preview be started by hand on any task? → A: Yes: "Lanzar preview" / "Parar preview" on any task of a project with a preview command; autopilot uses the same action.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - From idea to open PR without stops (Priority: P1)

The user creates a task with autopilot on and starts it. The first session ("Spec y
plan") runs the spec workflow without stopping for approvals and stops once the tasks
are written. Delilah starts the implementation session on its own (no offer), then the
review session once every task is checked, and when the review ends ready, it pushes
the branch and opens the PR with the title and description the agent wrote. The task
moves to review and enters the PR loop, and the user gets one notification: the PR is
open.

**Why this priority**: It is the feature: every other story only adds to or protects
this chain. Without the preview it already saves the user all the stops between the
idea and the PR.

**Independent Test**: On a fixture project with a stubbed agent launcher and a fake
`gh`, start an autopilot task, simulate each step's session reaching its end (tasks
written, all tasks checked, review ready with commits on the branch) and check that
each next session starts without user action and that a PR is opened and linked.

**Acceptance Scenarios**:

1. **Given** an autopilot task being started, **When** its first session launches, **Then** its instruction tells it to run the whole spec workflow (spec, plan, tasks) without asking for approvals, using the task's branch and the next free spec folder, and to stop once `tasks.md` is written.
2. **Given** that session has written `tasks.md` and its turn ends, **When** Delilah sees the boundary, **Then** it starts the implementation session on its own, as saving mode starts the review today, without offering it in the Bandeja.
3. **Given** the implementation session has checked every task, **When** its turn ends, **Then** the review session starts with an instruction to review, run the project's checks, apply the confirmed fixes, commit, and write the PR's title and description.
4. **Given** the review session ends with outcome *ready* and the branch has commits beyond its base, **When** Delilah sees it, **Then** it pushes the branch, opens the PR with the agent's title and description (falling back to today's proposal from the commit subjects if they are missing), links it, the task moves to review and the PR loop takes over.
5. **Given** the PR is open, **When** the user looks at the task, **Then** the session chain shows every step done, the PR tab shows the PR, and the history says the PR was opened by autopilot.
6. **Given** a task without autopilot, **When** it runs, **Then** nothing changes from today (saving mode or not).

---

### User Story 2 - Pause into the Bandeja when the agent gets stuck (Priority: P1)

Whenever the chain cannot go on by itself, autopilot pauses: the task stays where it
is, autopilot is marked as paused with the reason, the Bandeja shows one item ("Autopiloto
en pausa: <reason>") and the task goes back to the normal flow, so the user carries on
by hand (answering, confirming, opening the PR) exactly as with saving mode. The user
can also resume autopilot from that item once the cause is fixed.

**Why this priority**: Without a safe stop, an autopilot that silently loops or stops
half-way is worse than no autopilot: the user must be able to trust that a task is
either progressing or waiting for them in the Bandeja.

**Independent Test**: For each pause cause, drive an autopilot task to it with the
stubbed launcher and check the task's autopilot state, the Bandeja item and its reason,
that no next step starts, and that "Reanudar autopiloto" picks the chain up again.

**Acceptance Scenarios**:

1. **Given** an autopilot step's session asks the user a question, **When** Delilah receives it, **Then** Delilah answers with the option the agent marked as recommended and the session goes on; the question and the chosen answer are written to the task's history. If any question in it has no option (or more than one) marked as recommended, autopilot pauses with the question as its reason and it waits in the Bandeja as today (FR-013a).
2. **Given** a step's session ends its turn without reaching its step's end, a second time after one automatic nudge (FR-013b) (the plan without `tasks.md`, the implementation with unchecked tasks, the review with outcome *blocked* or no outcome, e.g. checks still failing after the agent's own retries), **When** Delilah sees it, **Then** autopilot pauses with that reason and nothing else starts.
3. **Given** a step's session fails or cannot be started, **When** the handoff fails, **Then** autopilot pauses with the failure as its reason (the existing handoff retry stays available).
4. **Given** pushing the branch or opening the PR fails, **When** Delilah sees the error, **Then** autopilot pauses with the error, and "Abrir PR" stays available to the user as today.
5. **Given** a paused autopilot task, **When** the user carries on by hand (confirms a handoff, opens the PR, types in the session), **Then** the task behaves exactly as a saving-mode task, and autopilot stays paused until the user resumes it.
6. **Given** a paused autopilot task whose cause is fixed, **When** the user chooses "Reanudar autopiloto", **Then** Delilah re-evaluates where the chain is and continues from there (starts the next step, or opens the PR) without repeating finished steps.
7. **Given** an autopilot task hits the usage limit, **When** the limit resets, **Then** the task resumes as today and autopilot continues; the limit is not a pause cause.

---

### User Story 3 - Choose autopilot when creating or starting a task (Priority: P2)

Autopilot is an option next to saving mode wherever saving mode is chosen today: the
new-task dialog, a backlog item, a running task (to turn it on for the remaining
steps or off), and the CLI. Turning autopilot on implies saving mode (it is the chain
autopilot drives). The task screen and the board show that a task is on autopilot,
and whether it is running or paused.

**Why this priority**: Needed to use the feature from the app, but the chain (US1)
and the pause (US2) can be built and tested through the API first.

**Independent Test**: Create tasks with and without autopilot from the dialog, the
backlog and the CLI; check the stored flag, that saving mode is forced on, and that the
task screen shows the autopilot state.

**Acceptance Scenarios**:

1. **Given** the new-task dialog or a backlog item, **When** the user ticks "Autopiloto", **Then** saving mode is ticked and locked on, and the task is stored with autopilot on.
2. **Given** a running saving-mode task, **When** the user turns autopilot on, **Then** the current step goes on as it is and the next boundaries are handled by autopilot (a pending planning → implementation offer is confirmed at once).
3. **Given** a running autopilot task, **When** the user turns autopilot off, **Then** the task continues as a plain saving-mode task from the next boundary.
4. **Given** the CLI, **When** the user runs `delilah run --autopilot …` (or `new`), **Then** the task starts with autopilot on; `--no-autopilot` turns it off.
5. **Given** an autopilot task, **When** the user looks at the board card or the task header, **Then** an autopilot mark shows whether it is running or paused (with the reason on hover/in the task).

---

### User Story 4 - A preview of the branch, ready when the PR opens (Priority: P2)

For projects whose settings define a preview command, autopilot starts one for the task's
workspace once the review ends ready, before opening the PR, and shows its URL on the
task (and in the PR's description). The user opens the PR notification, clicks the
preview and tries the change.

**Why this priority**: It saves the last manual step before trying the change, but a
PR without a preview is still a useful end state, and only projects with a preview
command benefit.

**Independent Test**: Register a fixture project, set its preview command to one that serves a
page on a free port, run an autopilot task to the review's end, and check that the
preview is running, its URL is on the task, and it is stopped when the task is done.

**Acceptance Scenarios**:

1. **Given** a project with a preview command, **When** the review step ends ready, **Then** Delilah starts the project's preview command for the task's workspace as a long-lived process it owns (one per task, on a free port), and its URL is shown on the task.
2. **Given** a project without a preview command, **When** the review ends ready, **Then** the preview step is skipped and the PR opens as in US1.
3. **Given** the preview fails to start (no free slot, the command fails), **When** Delilah sees it, **Then** the PR still opens and the task shows that the preview could not start, with the reason; autopilot does not pause for it.
4. **Given** a task with a running preview, **When** the task is done, archived or deleted, or the user stops the preview from the task, **Then** the preview is stopped and its URL removed.
5. **Given** any task (autopilot or not) of a project with a preview command and a workspace, **When** the user chooses "Lanzar preview" on the task, **Then** the same preview starts and its URL is shown, with "Parar preview" to stop it; for a project without one, the action is not offered.

---

### User Story 5 - Create and launch on autopilot from an idea (Priority: P3)

The proposal card at the end of an idea conversation ("Nueva tarea") offers, next to
"Crear", "Crear y lanzar en autopiloto": it creates the backlog task with the brief as
notes, as today, and starts it right away with autopilot on, in the project of the
conversation and its default work mode (checkout or worktree).

**Why this priority**: A shortcut for the case that motivated the feature (a simple
idea, straight to a PR), but creating the task and starting it with autopilot from the
backlog already works after US3.

**Independent Test**: In an idea conversation that ends with a proposal, choose
"Crear y lanzar en autopiloto" and check that the task exists with the brief as notes,
is running with autopilot on, and its first session got the brief.

**Acceptance Scenarios**:

1. **Given** a proposal card, **When** the user chooses "Crear y lanzar en autopiloto", **Then** the task is created as with "Crear" and started at once with autopilot (and saving mode) on, and the card links to the running task.
2. **Given** the start fails (e.g. the workspace cannot be created), **When** Delilah reports it, **Then** the task stays in the backlog with autopilot on and the error is shown on the card, as starting from the backlog does today.

---

### Edge Cases

- The plan session asks the spec workflow's own setup questions (output format, branch, scope, how to implement): the autopilot instruction answers them up front (markdown, the task's branch, no clarify, stop after tasks), so the session should not ask them; if it does anyway, the recommended option is taken (US2 scenario 1).
- The task's branch already has a spec folder for this task (e.g. autopilot turned on after planning by hand): the plan step is considered done and the chain starts at the step the spec says (as saving mode's boundary does).
- A pending planning → implementation offer when autopilot is turned on: it is confirmed at once (US3 scenario 2).
- The "save notes first" preference is on: each handoff still saves notes first, as in saving mode.
- The review ends ready but the branch has no commits beyond its base, or every repo already has an open PR: no PR is opened; autopilot pauses with that reason if no PR exists, or simply finishes if one does.
- Parent workspaces: the PR is opened for every repo of the task that has commits and no open PR, as "Abrir PR" does per submodule; a failure on one repo pauses autopilot with that repo named, keeping the PRs already opened.
- The daemon restarts in the middle of the chain: the chain resumes from the stored handoff state as saving mode does today, and a half-done "open PR" is re-checked against the linked PRs before retrying, so a PR is never opened twice.
- The session stops for a permission prompt or any other input in its terminal: autopilot pauses (it never approves permissions); the user answers in the terminal and resumes.
- The review ends *ready* with uncommitted changes in the workspace: autopilot pauses rather than open a PR without them.
- The user types in an autopilot session: that is allowed and does not pause autopilot; the boundaries are still evaluated when the turn ends.
- The daemon restarts while a preview runs: the preview is not adopted again; it is marked stopped (URL removed) and the user can relaunch it from the task.
- Two tasks on autopilot at once: each gets its own sessions and, if any, its own preview; previews never share a port.

## Requirements *(mandatory)*

### Functional Requirements

**Flag and state**

- **FR-001**: Each task MUST have an autopilot option, off by default (no preference for its default in this version), stored with the task and kept across restarts. It is offered wherever saving mode is (tasks whose workflow is the spec workflow); project sessions and idea conversations have no autopilot.
- **FR-002**: Autopilot on MUST imply saving mode on; turning saving mode off MUST turn autopilot off.
- **FR-003**: An autopilot task MUST have an autopilot state visible through the API and the UI: *running* or *paused* with a reason (a short user-facing message), plus the time it paused.
- **FR-004**: The user MUST be able to turn autopilot on or off on a new task, a backlog item, a running task and from the CLI (`--autopilot` / `--no-autopilot` on `run` and `new`), and to resume a paused autopilot.

**The chain**

- **FR-005**: The first session of an autopilot task MUST be told to run the spec workflow end to end without asking for approvals or setup choices (markdown output, the task's existing branch, no clarification phase, tasks generated), and to stop once `tasks.md` is written.
- **FR-006**: At the planning → implementation boundary, an autopilot task MUST start the implementation session on its own (no offer), as saving mode starts the review.
- **FR-007**: The implementation session MUST be told to implement without stopping between phases or work packages.
- **FR-008**: The review session of an autopilot task MUST be told to review the branch, run the project's checks, apply the confirmed fixes, commit, write the PR's title and description, and end with an explicit outcome Delilah can read: *ready* or *blocked* with a reason.
- **FR-009**: When the review ends *ready*, Delilah MUST (in this order) start the preview if the project has one (US4), then push the branch and open the PR through the same flow as "Abrir PR", using the agent's title and description, or today's proposal if they are missing.
- **FR-010**: The PR MUST be opened as a draft: nobody has reviewed it yet; the user marks it ready after trying it.
- **FR-011**: Once the PR is linked, autopilot MUST be finished for that task (the PR loop takes over: comments and failed CI go to the Bandeja as today, and are not fixed automatically).
- **FR-012**: Every handoff, PR opening and pause MUST be written to the task's history, so the user can follow what autopilot did.

**Pausing**

- **FR-013**: Autopilot MUST pause (not stop the session, not change the task's phase) when: a step's turn ends without reaching its end (FR-008 *blocked* included); a step's session fails or cannot start; the session waits for the user for anything autopilot cannot answer (e.g. a permission prompt in the terminal); the review ends *ready* but the workspace has uncommitted changes; pushing or opening the PR fails; or the agent asks a question that autopilot cannot answer (FR-013a).
- **FR-013b**: Before pausing because a step's turn ended without reaching its end, autopilot MUST nudge that step's session once (one typed instruction to carry on without asking, written to the history); a second such turn end in the same step pauses. The review's *blocked* outcome is never nudged.
- **FR-013a**: A question is answered by autopilot only when every question in it has exactly one option whose label is marked as recommended ("(Recommended)" or "(Recomendado)", the convention the agent's skills use); that option is chosen (for multi-select, only it). Otherwise autopilot pauses and the question waits in the Bandeja as today. Each automatic answer is written to the history with the question and the option chosen.
- **FR-014**: A pause MUST create one Bandeja item per pause with the reason and the actions "Reanudar autopiloto" and "Seguir a mano" (which turns autopilot off), and send one notification.
- **FR-015**: While paused, every manual action of saving mode (confirm or decline the handoff, retry, open the PR) MUST work as for a saving-mode task.
- **FR-016**: Resuming MUST re-evaluate the chain from the task's current state (spec files, sessions, linked PRs) and continue from the first unfinished step, never repeating a finished one or opening a second PR for a repo. If the step's session is still working, resuming only marks autopilot as running and the chain goes on at that session's next turn end.
- **FR-017**: Hitting the usage limit MUST NOT pause autopilot: the task pauses by limit as today and the chain goes on after the reset.

**Preview**

- **FR-018**: A project MAY define a preview command in its Delilah settings (empty by default, editable from the app and the API), which Delilah (not the agent) starts and owns, in the task's workspace; the command MUST print the URL it serves on its output; for this repo it is `scripts/preview.sh`, which MUST be able to run without a terminal (non-interactive, in the first free slot) and report the URL it serves.
- **FR-018a**: Delilah MUST take the first `http://` or `https://` URL the command prints as the preview's URL; if the command exits, or prints no URL within 10 minutes (a first preview may build the project), the preview failed, and the reason shown includes the last lines of its output.
- **FR-019**: The preview's URL MUST be shown on the task and included in the PR description when the preview started before the PR opened.
- **FR-020**: A task's preview MUST be stopped when the task is done, archived or deleted, or when the user stops it from the task; a preview failure MUST NOT pause autopilot.
- **FR-020a**: The user MUST be able to start and stop a task's preview by hand on any task of a project with a preview command; autopilot uses this same action.

**From an idea**

- **FR-021**: The idea conversation's proposal card MUST offer "Crear y lanzar en autopiloto", which creates the task as "Crear" does and starts it with autopilot on.

**Language**

- **FR-022**: Every instruction autopilot gives the agent, and every message it shows, MUST follow the app's language (Spanish or English), as saving mode's do.

### Key Entities

- **Autopilot state** (per task): on/off; when on, *running* or *paused*; the pause reason and time; whether it has finished (PR linked).
- **Step outcome** (per review session): *ready* or *blocked* with a reason, plus the PR title and description, written by the agent where Delilah can read it.
- **Preview** (per task, at most one): the project's preview command, the URL it serves, whether it is running, and the reason if it failed to start.
- **Project preview command** (per project, optional, a Delilah project setting): how to start a preview of a workspace without a terminal.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: A small, clear task (e.g. a copy change plus a test) goes from "start" to an open PR with zero user actions in between.
- **SC-002**: In every autopilot run, the task ends in exactly one of two states the user can see without opening a session: a linked PR, or a paused autopilot with a reason in the Bandeja. No run is left stalled without one of them.
- **SC-003**: No autopilot task ever opens more than one PR per repo, including across daemon restarts and resumes.
- **SC-004**: After a pause, the user can carry on by hand with the same actions as a saving-mode task, and resuming continues from the first unfinished step without repeating finished ones.
- **SC-005**: For projects with a preview command, the preview URL is on the task by the time the PR notification arrives in at least 9 of 10 runs; a failed preview never blocks the PR.
- **SC-006**: Tasks without autopilot behave exactly as before (all existing saving-mode and PR tests keep passing).

## Assumptions

- Autopilot builds on saving mode (specs/004-saving-mode): one session per step, the same handoff state, recovery and "save notes first" preference. It adds automatic confirmation, two final actions (preview, PR) and the pause.
- The non-interactive behaviour of each step comes from the instruction the session is given (it tells the spec and orchestrate workflows to run without stops and pre-answers their setup questions); the user's global skills are not edited by this feature.
- "Tests failing after N attempts" is detected through the step's outcome: the orchestration and review workflows already retry on their own, and a step that gives up ends without reaching its end, which pauses autopilot (FR-013). Delilah adds no retry loop of its own beyond one nudge per step (FR-013b).
- No time, token or retry limits of its own in this version: the usage limit already bounds spending, and a stuck step ends its turn (pause). A wall-clock or token cap can be added later if runs show a need.
- Bumping the version and following the repo's other rules (CLAUDE.md) is the agent's job inside the steps, as in a manual run.
- The PR description comes from the agent (the review session, e.g. with `pr-writer`); Delilah only reads it.
- The PR loop is unchanged: autopilot finishes when the PR is linked; fixing review comments or CI automatically is out of scope.
- It does not depend on LIBRE-7 (E2E with a fake Claude): the daemon side is tested with the stubbed launcher and the fake `gh` already used by saving mode and the PR loop; one manual end-to-end run with real Claude in a preview validates the instructions.
- Only the Claude Code adapter exists, so autopilot only targets it.
