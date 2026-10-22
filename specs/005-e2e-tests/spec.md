# Feature Specification: End-to-end tests

**Feature Branch**: `feature/LIBRE-8`

**Created**: 2026-10-22

**Status**: Draft

**Input**: User description: "Deberíamos meter muchos tests E2E. Haz análisis de esto y veamos cómo hacerlo." (brief written by Claude in the «Idea» conversation, 2026-10-22)

## Context

Today every test is a unit test. Each crate tests itself, the daemon against an
in-memory store, and the web UI tests one component at a time in a simulated DOM.
Nothing checks that the pieces work **together**: a real daemon process with its
own state on disk, its HTTP API, the CLI, git worktrees on real repos, and the UI in
a real browser. Regressions in that glue (a route the UI calls with the wrong shape,
a hook the daemon no longer understands, a workspace that isn't created) are found
only by hand, in a preview.

Two things make full-system tests hard here:

1. **The agent is a real Claude Code session.** Launching one costs the user's
   subscription and is slow and non-deterministic. `docs/development.md` asks to keep
   those few and short. A test must never start one by accident.
2. **The user's real Delilah runs on the same Mac** (daemon on 7420, state in
   `~/.delilah`), and it runs the agents building Delilah. A test must never touch it.

This feature adds a **fake Claude** that the daemon can be told to launch instead of
the real one, and two layers of end-to-end tests built on it: one that drives a real
daemon through its API and CLI, and one that drives the UI in a browser against that
daemon. Both run from one command.

## Clarifications

### Session 2026-10-22

- Q: How close should the fake Claude's terminal output be to the real one? → A: Lifecycle hooks plus plain text, and it can show the folder-trust dialog so the daemon's auto-accept is covered; no replay of recorded sessions.
- Q: Where do the end-to-end tests run? → A: Locally only, before each PR; no CI workflow for now.

## User Scenarios & Testing *(mandatory)*

The "user" of this feature is the developer of Delilah (the user, or an agent working
on Delilah from inside Delilah).

### User Story 1 - A fake Claude the daemon launches instead of the real one (Priority: P1)

The developer starts a daemon in a throw-away home and port, telling it to use the
fake Claude. When a task starts, the daemon launches the fake one in its terminal,
exactly as it would launch Claude Code. The fake Claude reports to the daemon the
same lifecycle signals the real one does (session started, working, waiting for an
answer, asked a question, finished, failed), following a script chosen by the test,
and prints recognisable text to its terminal. No real Claude session is ever started.

**Why this priority**: every other story needs it. Without it, any test that starts a
task either launches a real session or can't check what happens after the start.

**Independent Test**: start an isolated daemon with the fake Claude, create and start
a task on a fixture repo, and check that the task moves to "working", then to
"waiting" when the fake Claude finishes its script, and that its terminal shows the
fake's output. Also check that `claude` was never run.

**Acceptance Scenarios**:

1. **Given** an isolated daemon told to use the fake Claude, **When** a task is started, **Then** the fake Claude runs in the task's workspace with the same arguments and the same clean environment the real one would get (session id, settings file with the hooks, prompt).
2. **Given** a fake Claude script "work, then stop", **When** the task starts, **Then** the daemon shows the task as working and then as waiting for the user, as it does for a real session.
3. **Given** a fake Claude script "ask a question", **When** it runs, **Then** an ask card with that question appears in the inbox, and answering it reaches the fake Claude, which can tell the test what answer it got.
4. **Given** a task whose session already ran, **When** it is resumed, **Then** the fake Claude is launched in resume mode with the same session id.
5. **Given** the fake Claude is selected but its script fails (exits with an error), **When** the task runs, **Then** the daemon reports the session as failed, exactly as with a real failure.
6. **Given** no fake Claude was selected, **When** the daemon starts, **Then** it behaves exactly as today (launches `claude`): the switch exists only for isolated daemons.

---

### User Story 2 - End-to-end tests of the daemon, its API and the CLI (Priority: P1)

The developer runs one command and gets a set of tests that each start a real daemon
process in a throw-away home and port, register the fixture repos as projects, and
walk the main flows over HTTP and with the `delilah` CLI, checking both the API's
answers and what ends up on disk (worktrees, branches, task folders).

**Why this priority**: the daemon is where most behaviour lives, and these tests are
cheap, fast and stable (no browser). They catch broken contracts between the daemon,
git and the CLI.

**Independent Test**: run the API end-to-end suite alone. It passes on a clean
checkout, leaves no daemon process running and nothing outside its temp folders.

**Acceptance Scenarios** (the first batch of critical flows; see FR-010):

1. **Given** a fresh daemon, **When** the fixture repo and the fixture parent (with submodules) are registered, **Then** both are listed as projects with the right kind.
2. **Given** a registered repo project, **When** a task is created in worktree mode and started, **Then** a worktree exists on branch `feature/<KEY>`, the task is working, and the CLI lists it.
3. **Given** a parent project, **When** a task is started choosing some submodules, **Then** only those submodules are initialised in the worktree, all on the same branch.
4. **Given** a task in checkout mode, **When** it starts and later is archived, **Then** the project's checkout is on the task branch while it runs and is back on its previous branch afterwards.
5. **Given** a running task, **When** its state and phase change (fake Claude stops; phase moved via API/CLI; marked done), **Then** the API, the event stream and the CLI all show the same state and phase.
6. **Given** a task whose fake Claude asks a question, **When** the answer is sent through the API, **Then** the ask is resolved, leaves the inbox and the session continues.
7. **Given** a task with a PR answered by the fake `gh`, **When** the PR is opened, fails checks, gets a review and is merged (by editing the fake's files), **Then** the task shows each PR state and, once merged, is archived with its branches cleaned up as today.
8. **Given** Jira answers taken from the sample JSON, **When** Jira issues are listed and one is turned into a task, **Then** the task has that issue's key and title.
9. **Given** a backlog task, **When** it is deleted, **Then** it disappears from the API and its folders are removed.
10. **Given** a project conversation ("task from an idea") with the fake Claude, **When** it proposes a task and the proposal is confirmed, **Then** a new task exists with the proposed brief as its prompt.
11. **Given** a daemon that is stopped and started again on the same home, **When** the API is queried, **Then** the tasks, sessions and projects are still there (state survives a restart).

---

### User Story 3 - End-to-end tests of the UI in a browser (Priority: P2)

The developer runs the UI suite and a real browser opens the UI of this checkout,
pointed at an isolated daemon prepared like in Story 2, and goes through the key
screens as a person would: clicking, typing, reading what's shown.

**Why this priority**: it catches what the component tests can't (routing, the real
API shapes, live updates from the daemon), but it is slower and more fragile than the
API layer, so it comes second and covers fewer, broader flows.

**Independent Test**: run the UI suite alone against a daemon it starts itself. It
passes headless on a clean checkout.

**Acceptance Scenarios**:

1. **Given** an empty daemon, **When** the user adds the fixture repo as a project in the UI, **Then** it appears in the project list.
2. **Given** a project, **When** the user creates a task with the new-task dialog and starts it, **Then** it appears on the board in the right column and moves when the fake Claude stops.
3. **Given** a fake Claude that asks a question, **When** the user opens the inbox and answers the ask card, **Then** the card disappears and the task goes back to working.
4. **Given** a project conversation, **When** the fake Claude proposes a task and the user confirms it, **Then** the new task opens.
5. **Given** a task with a PR from the fake `gh`, **When** the user opens its PR view, **Then** the PR's state, checks and review comments are shown.
6. **Given** several tasks, **When** the user opens quick open and types part of a key or title, **Then** the matching task is offered and opening it shows that task.

---

### User Story 4 - One command, documented, safe to run anywhere (Priority: P2)

The developer (or an agent inside a Delilah task) runs a single script to run all the
end-to-end tests, or one layer of them, and reads in the development guide how they
work, how to add one, and how to write a fake Claude script.

**Why this priority**: tests nobody runs don't protect anything. It must be one
command and impossible to aim at the real Delilah.

**Independent Test**: run the script while the installed Delilah is running; it passes,
and the installed daemon, its state and its sessions are untouched.

**Acceptance Scenarios**:

1. **Given** a checkout, **When** the developer runs the e2e script with no arguments, **Then** it builds what's needed, runs both layers and reports a pass/fail summary.
2. **Given** an argument for one layer, **When** the script runs, **Then** only that layer runs.
3. **Given** the installed Delilah running on its usual port and home, **When** the e2e suite runs, **Then** it uses other ports and its own temp homes, and the installed daemon is never contacted, restarted or stopped.
4. **Given** a test that fails or is interrupted, **When** the run ends, **Then** every daemon and UI server it started is stopped (by its own process, never by name) and its temp state is kept only when asked, for debugging.

---

### User Story 5 - An optional smoke test with the real Claude (Priority: P3)

The developer can run, on purpose, one short test that launches the **real** Claude
Code once, to check the fake still matches reality (arguments accepted, hooks
received, folder-trust dialog accepted).

**Why this priority**: the fake can drift from the real Claude; a cheap manual check
catches that. It costs the subscription, so it never runs by default.

**Independent Test**: run the smoke test explicitly; it starts one real session on a
fixture repo, sees it start and stop through the hooks, and closes it.

**Acceptance Scenarios**:

1. **Given** the normal test commands, **When** they run, **Then** the real-Claude smoke test is skipped.
2. **Given** an explicit opt-in, **When** the smoke test runs, **Then** it starts exactly one real session with a tiny prompt, checks the start and stop signals arrive, and closes the session.

### Edge Cases

- The chosen port is taken (another preview, another test running in parallel): each daemon gets a free port; parallel tests don't share a daemon unless they're written to.
- Two e2e runs at once (two worktrees, two agents): they don't share ports, homes or fixture repos.
- The fake Claude is selected but the script path doesn't exist or isn't executable: the session fails with a clear error and the test reports it, rather than hanging.
- The fake Claude never sends "stop": tests wait with a bounded timeout and fail with what the daemon last saw, never hang forever.
- A daemon crashes mid-test: the test fails, shows the daemon's log, and cleanup still runs.
- The UI dev server can't start (port taken, dependencies missing): the UI suite fails fast with a clear message.
- `gh` or Jira credentials exist on the machine: the tests never use them; only the fakes are reachable.
- The fixture repos are reused between tests: each test gets its own copy, so one test's branches and worktrees never leak into another.
- The machine has the user's real `~/.claude` settings and plugins: the fake Claude never reads them, and the smoke test runs with the same clean environment the daemon gives agents.

## Requirements *(mandatory)*

### Functional Requirements

**Fake Claude**

- **FR-001**: The daemon MUST accept, through its startup configuration, a program to launch instead of `claude` for every agent session; when it is not set, behaviour MUST be exactly as today.
- **FR-002**: The project MUST provide a fake Claude program that accepts the same arguments the daemon passes to Claude Code (new session or resume, session id, settings file, added folders, prompt) and records what it received so tests can check it.
- **FR-003**: The fake Claude MUST report its lifecycle to the daemon the same way the real one does, through the hooks in the settings file it is given, so the daemon cannot tell it apart.
- **FR-004**: The fake Claude's behaviour MUST be chosen per test from a small set of scripted steps: print text, report working, ask a question (including the confirmation card a task-from-idea conversation ends with) and wait for the answer, stop (wait for the user), fail, exit, sleep. Tests MUST be able to choose a script per task or per session.
- **FR-005**: The fake Claude MUST read what is typed into its terminal (replies, answers) and make it available to the test.
- **FR-006**: The fake Claude MUST NOT contact any network service, read the user's Claude configuration, or start the real Claude.
- **FR-006a**: The suite MUST fail, naming the test, if anything it starts tries to run the real Claude outside the opt-in smoke test (this is how SC-002 is checked).
- **FR-006b**: The fake Claude MAY skip the status-line hook (usage and context figures); usage display is not covered in this feature.
- **FR-007**: The fake Claude MUST be able to emit the folder-trust dialog, so the daemon's automatic acceptance is also covered.
  Beyond that, its terminal output is plain recognisable text: it does not replay real Claude sessions.

**Isolated environment**

- **FR-008**: Every end-to-end test MUST run against a daemon in its own temporary home and on a free port that is never the installed daemon's (7420), and MUST stop only the processes it started, by their own handle. Two runs at the same time (two worktrees) MUST NOT interfere.
- **FR-009**: Each test MUST get its own copies of the fixture repos (a plain repo and a parent with submodules), the fake `gh` data and the sample Jira data; nothing from the user's real repos, credentials or work machine is used.

**API/CLI layer**

- **FR-010**: The API/CLI layer MUST cover at least the 11 flows of Story 2 in its first batch, and each one MUST check both the daemon's answers and the resulting state on disk where there is one (worktrees, branches, task folders).
- **FR-011**: Flows that have a CLI command MUST be exercised at least once through the CLI against the isolated daemon.
- **FR-012**: Waiting for asynchronous changes (session state, PR polling, events) MUST use bounded waits (no single wait longer than 30 seconds) that fail with the last observed state; no test may rely on a fixed sleep to pass.

**UI layer**

- **FR-013**: The UI layer MUST run the UI of the current checkout in a real browser, headless by default, against an isolated daemon prepared like the API layer.
- **FR-014**: The UI layer MUST cover the 6 flows of Story 3 in its first batch.
- **FR-015**: On a failure, the UI layer MUST keep a screenshot and a trace of the failing test, and the daemon's log.

**Running and docs**

- **FR-016**: One script MUST run all end-to-end tests, or one layer, building what's needed first, and report a summary; it MUST be safe to run while the installed Delilah is running.
- **FR-017**: The development guide MUST explain how the end-to-end tests work, how to run them, how to add a test and how to write a fake Claude script; the decision (fake Claude, two layers, where they run) MUST be recorded in the decisions doc.
- **FR-018**: The real-Claude smoke test MUST be skipped unless explicitly asked for, and when asked MUST start at most one short session.
- **FR-019**: The end-to-end tests run **locally only**, before each PR, by the developer or the agent working on the task; no CI workflow is added in this feature.
- **FR-020**: Native-only behaviour of the desktop shell (menu, notifications, Dock badge, global quick open shortcut) is out of scope; the UI layer tests the web UI only.

### Key Entities

- **Isolated daemon**: a real daemon process started by a test, with its own temp home, port, fixture repos and fakes; owned and stopped by that test.
- **Fake Claude script**: the ordered steps a fake Claude session follows (print, work, ask, stop, fail, exit, sleep), chosen by the test for a task or session.
- **Fake Claude record**: what a fake session received (arguments, environment, settings file, typed input, answers), readable by the test.
- **Fixture set**: the per-test copies of the fixture repos, fake `gh` data and Jira sample.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: The first batch has at least 17 end-to-end flows (11 API/CLI + 6 UI), all passing on a clean checkout.
- **SC-002**: Running the whole suite starts zero real Claude sessions (checked by the suite itself), except when the smoke test is explicitly asked for.
- **SC-003**: Running the whole suite while the installed Delilah is in use leaves it untouched: same process, same sessions, no change in its state.
- **SC-004**: The API/CLI layer runs in under 3 minutes and the UI layer in under 5 minutes on the developer's Mac, after a build.
- **SC-005**: Ten consecutive runs of the suite give the same result (no flaky test).
- **SC-006**: After any run, passed, failed or interrupted, no daemon or UI server started by it is still running.
- **SC-007**: A developer new to the suite can add a new API flow, with its fake Claude script, by following the development guide alone.

## Assumptions

- The API/CLI layer is built first (Stories 1 and 2), then the UI layer (Story 3); both are in scope of this feature.
- "Many" means a prioritised first batch of the critical flows listed above; more are added over time following the guide, not in this feature.
- The fixture repos come from the existing fixture script; the fake `gh` from the existing PR loop work; Jira from the existing sample JSON.
- The tests run on macOS (the only platform Delilah supports today).
- The existing unit tests stay as they are; end-to-end tests are an extra layer, run by their own command and not by the normal per-crate test commands, so those stay fast.
- Only the Claude Code adapter exists, so only one fake agent is needed.
