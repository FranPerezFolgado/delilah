# Feature Specification: CI autofix

**Feature Branch**: `010-ci-autofix`

**Created**: 2026-10-26

**Status**: Draft

**Input**: Task LIBRE-22. User note: "En claude desktop, si sube la pr, se puede añadir autofix que espera al CI y si falla lo fixea solo" (in Claude Desktop, once the PR is up you can turn on an autofix that waits for CI and, if it fails, fixes it on its own). Prior art: the PR loop (`specs/003-pr-loop`) and autopilot (`specs/005-autopilot`).

## Context

The PR loop already knows each task's PR, follows its CI checks and, when CI
fails, puts a "CI failed" item in the Inbox with "Ask the agent to fix it". The
user still has to notice the item, open it and click, and then come back once the
next run ends to see whether the fix held. For most CI failures (a lint, a type
error, a test the change broke) that click is the only thing the user adds: they
don't read the log, they just forward it.

Decision #29 ruled out forwarding PR feedback automatically, for two reasons: it
spends the subscription without asking, and a reviewer's text would steer the
agent unseen. CI autofix keeps the second reason intact (it never forwards review
comments, only the output of the repo's own CI) and answers the first by being
opt-in and bounded: a fixed number of attempts per PR, visible at all times, off
whenever the user says so.

Autopilot ends today at an open draft PR. With CI autofix, an autopilot task goes
one step further: it keeps its PR green on its own, so the user comes back to a PR
that passes CI or to one Inbox item saying why it doesn't.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - A failed CI is fixed without the user (Priority: P1)

A task has CI autofix on and an open PR (draft or not). CI runs on the PR's
latest commit and a check fails. Once every check of that commit has finished,
Delilah sends the failure to the task's agent: which checks failed, their links,
and how to read the failed logs. The agent fixes the
code, commits and pushes. CI runs again; if it passes, the task is back to waiting
for review with no Inbox item left behind. The task's history shows each attempt.

**Why this priority**: it is the whole feature; the rest is control and safety
around it.

**Independent Test**: in a preview with the fake `gh` and fake Claude, a task with
autofix on and a PR whose checks report a failure: the agent receives one autofix
message; after the fake agent pushes and the fake checks report success, the
failed-CI item is gone and the history shows one attempt, fixed.

**Acceptance Scenarios**:

1. **Given** a task with autofix on, an open PR and an idle agent, **When** every
   check of the PR's latest commit has finished and at least one failed, **Then**
   within one PR check cycle the agent receives one message with the failed
   checks, their links and how to read their logs, and the history records "CI autofix:
   attempt 1 of 3".
2. **Given** that message was sent, **When** the agent pushes a new commit and
   its checks all pass, **Then** the CI-failed Inbox item is resolved, the
   attempt is recorded as fixed, and the attempt count for that PR starts again
   from zero.
3. **Given** a check of the latest commit is still running, **When** another has
   already failed, **Then** nothing is sent until every check of that commit has
   finished.
4. **Given** autofix already sent the failure of a commit, **When** the checks of
   that same commit are seen again (later polls, a restart of Delilah, a re-run
   that fails again), **Then** no second message is sent for that commit.
5. **Given** the agent is working, or waiting on the user (a question or a
   permission prompt), **When** CI fails, **Then** autofix waits and sends the
   failure only once the agent is idle, if the commit is still the PR's latest
   and still failing.

---

### User Story 2 - Autofix stops and hands over (Priority: P1)

Autofix never loops forever. After a set number of attempts on one PR (3 by
default) without a green run, or when an attempt ends without a new commit being
pushed, autofix stops for that PR and leaves one Inbox item saying so: "CI autofix
gave up after 3 attempts" or "The agent finished without pushing a fix", with
"Retry autofix". A later failing commit still gets the usual failed-CI item with
"Ask the agent to fix it". The user takes over from there.

**Why this priority**: without a limit, autofix could spend the subscription on a
failure the agent can't fix (a broken runner, a secret missing in CI, a flaky
test).

**Independent Test**: fake checks that always fail; after the third attempt no
further message is sent, one "gave up" Inbox item and one notification appear,
and the history lists three attempts.

**Acceptance Scenarios**:

1. **Given** autofix has made 3 attempts on a PR without a green run, **When**
   the fourth commit fails CI, **Then** nothing is sent to the agent, the task gets
   one "gave up" Inbox item and one notification, and the PR tab shows autofix as
   stopped for that PR.
2. **Given** an attempt was sent, **When** the agent's turn ends and no new commit
   reaches the PR, **Then** autofix stops for that PR with the "finished without
   pushing" item (the attempt counts).
3. **Given** autofix stopped for a PR, **When** the user turns autofix off and on
   again for the task, or clicks "Retry autofix" on the item, **Then** the count
   starts again from zero and the current failure (if any) is sent.
4. **Given** autofix stopped for a PR, **When** a later commit (pushed by hand or
   by the agent at the user's request) passes CI, **Then** the "gave up" item is
   resolved and autofix is armed again for that PR.

---

### User Story 3 - Turning autofix on and off (Priority: P2)

The user turns autofix on or off for a task from the task screen (next to the PR
tab's state, and in the new-task and start dialogs next to autopilot), and from
the CLI. Each project has a default: off, on for autopilot tasks only (the
default), or on for every task. Turning autofix off never interrupts a turn
already running; it only stops further attempts.

**Why this priority**: needed for the feature to be opt-in, but with US1 and a
fixed default it already works for autopilot tasks.

**Independent Test**: create one task with autopilot and one without in a project
on the default setting; only the autopilot one shows autofix on. Toggle each from
the task screen and from the CLI; the state matches in both places.

**Acceptance Scenarios**:

1. **Given** a project with the default setting, **When** a task is created with
   autopilot, **Then** autofix is on for it; **When** without autopilot, **Then**
   it is off.
2. **Given** a project set to "every task" or "off", **When** a task is created,
   **Then** autofix follows that setting, and the user can still change it for
   that task.
3. **Given** a running task, **When** the user turns autofix on and its PR's
   latest commit has already failed CI, **Then** that failure is sent as attempt
   1 (if the agent is idle).
4. **Given** an attempt is in progress, **When** the user turns autofix off,
   **Then** the agent's turn goes on, no further attempt is made, and the PR tab
   shows autofix off.
5. **Given** the CLI, **When** the user runs the autofix command on a task with
   `on` or `off`, **Then** the task's state changes as from the UI; the project
   default can be set with the project settings command.

---

### User Story 4 - Seeing what autofix is doing (Priority: P2)

On the PR tab and as a small mark next to the PR badge, the user sees autofix's
state for each PR of the task: off, watching (CI pending or green), fixing
(attempt N of 3, sent at time T), or stopped (with the reason). The task history
lists every attempt with its checks and outcome (fixed, failed again, no push,
cancelled).

**Why this priority**: autofix acts without asking, so it must be visible; but it
works without this view (the history and Inbox items cover the essentials).

**Independent Test**: drive a task through one failed and one fixed attempt and
check the PR tab state at each step and the two history entries.

**Acceptance Scenarios**:

1. **Given** autofix is fixing a PR, **When** the user opens the task, **Then**
   the PR tab says "Fixing CI: attempt 2 of 3" with the failed checks.
2. **Given** autofix stopped, **When** the user opens the PR tab, **Then** it
   shows the reason and "Retry autofix".

---

### Edge Cases

- **Usage limit:** if the agent is paused by the usage limit, autofix waits for
  the resume and then sends the failure if it still applies; it doesn't count an
  attempt it couldn't send.
- **No session to talk to** (the task's sessions all ended): autofix resumes the
  task's latest session, the same way "Ask the agent to fix it" does; if that is
  not possible, it stops with an Inbox item saying why.
- **Several repos** (a parent with submodules, one PR each): each PR has its own
  attempt count; the task has one agent, so failures of several PRs that are
  ready at the same time go in one message, and a later failure waits for the
  agent to be idle.
- **Cancelled, skipped or neutral checks** are not failures; only failed and
  timed-out checks are. A commit with no checks at all is never sent.
- **The PR is merged or closed** while an attempt is in progress: autofix stops
  for it at once and doesn't send anything further; the merge archive works as
  today.
- **The user pushes by hand** during an attempt: the new commit's CI is what
  counts; a green result ends the attempt as fixed.
- **The user already clicked "Ask the agent to fix it"** for a commit: that
  counts as the forward of that commit; autofix doesn't send it again.
- **Autopilot paused or turned off**: autofix is a setting of its own; pausing
  autopilot doesn't touch it, but autofix never answers questions or permission
  prompts (those still wait for the user, as for any task with autopilot done).
- **Task archived, deleted or moved to the backlog**: autofix stops.
- **Logs**: Delilah doesn't download logs; the message tells the agent how to read
  them with the user's `gh` (as "Ask the agent to fix it" does today), so a log the
  agent can't read is the agent's to report.
- **Delilah restarts** mid-attempt: the attempt is remembered; nothing is sent
  twice and the "no push" check still happens once the agent's turn ends.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: Each task MUST have an autofix setting (on/off), stored with the
  task and kept across restarts.
- **FR-002**: Each project MUST have an autofix default with three values: off,
  autopilot tasks only (the default for existing and new projects), every task.
  New tasks MUST take their autofix setting from it; the user MUST be able to
  override it per task at creation and at any time later.
- **FR-003**: When autofix is on for a task, Delilah MUST, for each open PR of the
  task (draft or not), detect a latest commit whose checks have all finished with
  at least one failed or timed-out check.
- **FR-004**: For such a commit, Delilah MUST send the agent one message with the
  failed checks' names and links and how to read their logs, framed as CI output
  (data, not instructions from the user), and asking it to commit and push the fix
  to the same branch, without force-pushing or opening another PR.
- **FR-005**: Delilah MUST send at most one autofix message per commit, also
  across restarts and also when the user already forwarded that commit's failure
  by hand.
- **FR-006**: Delilah MUST only send when the agent is idle (not working, not
  waiting on a question or a permission prompt, not paused by the usage limit),
  and MUST re-check that the commit is still the PR's latest and still failing
  just before sending.
- **FR-007**: Autofix MUST NOT forward review comments, review states or any text
  other than CI check results and their logs (decision #29 still holds for
  reviews).
- **FR-008**: Each PR MUST have an attempt count, reset to zero when one of its
  commits passes all checks, when the user retries, or when autofix is turned on
  again. The limit MUST be 3 attempts per PR.
- **FR-009**: Autofix MUST stop for a PR when the limit is reached with CI still
  failing, or when an attempt's agent turn ends without a new commit reaching the
  PR, and MUST then raise one Inbox item with the reason and "Retry autofix", plus
  one notification.
- **FR-010**: Turning autofix off MUST stop further attempts without interrupting
  a running turn; merging, closing the PR, archiving, deleting or moving the task
  to the backlog MUST also stop it.
- **FR-011**: When an autofix attempt ends in a green run, the CI-failed Inbox item
  of that PR MUST be resolved, as it is today for any later green commit.
- **FR-012**: When autofix sends a commit's failure, that commit's CI-failed Inbox
  item MUST be resolved as if the user had forwarded it, and the "CI failed"
  notification MUST NOT be sent for a failure autofix is going to handle (autofix
  on and not stopped for that PR); the user is notified only when autofix stops.
- **FR-013**: The task history MUST record each attempt: when it was sent, the PR,
  the commit, the failed checks, and its outcome (fixed, failed again, no push,
  cancelled).
- **FR-014**: The PR tab MUST show autofix's state per PR (off, watching, fixing
  with attempt N of the limit, stopped with the reason); the task header or PR
  badge MUST show that autofix is on.
- **FR-015**: The user MUST be able to toggle autofix per task from the task
  screen, the new-task and start dialogs, the backlog item, and the CLI; and to set
  the project default from project settings and the CLI.
- **FR-016**: Everything autofix tells the agent and shows the user MUST be in the
  app's language (Spanish and English), like the rest of what Delilah tells
  Claude.
- **FR-017**: Autofix MUST NOT answer questions or permission prompts the agent
  raises while fixing; those reach the Inbox as for any task.

### Key Entities

- **Task autofix setting**: on/off per task; its default comes from the project.
- **Project autofix default**: off / autopilot tasks only / every task.
- **Autofix state per PR**: watching, fixing (attempt number, commit, sent at),
  or stopped (reason); the attempt count; the last commit already forwarded.
- **Autofix attempt** (history): task, PR, commit, failed checks, sent at,
  outcome.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: A CI failure on a task with autofix on reaches the agent within one
  PR check cycle (about 30 seconds) of its last check finishing, when the agent is
  idle, with no action from the user.
- **SC-002**: No commit's failure is ever sent to the agent twice, and no PR gets
  more than 3 automatic attempts in a row without a green run, including across
  restarts.
- **SC-003**: An autopilot task whose first CI run fails on something the agent
  can fix ends with a green PR without the user opening Delilah.
- **SC-004**: When autofix stops, the user learns it from exactly one Inbox item
  and one notification, which say why.
- **SC-005**: Review comments never reach the agent without the user's click.

## Assumptions

- The PR loop's polling (every 30 s, the user's `gh`) and its CI state per commit
  are the source of truth; autofix adds no new polling of its own beyond reading
  failed job logs once per failed commit.
- The agent reads failed logs itself with the user's `gh` (GitHub Actions); for
  other CI providers it has the check names and links.
- A limit of 3 attempts per PR is fixed in this version (not a setting); it can be
  made configurable later.
- "On for autopilot tasks" as the project default is acceptable for existing
  installs: autopilot is already the mode where the user accepted spending the
  subscription without asking. Existing autopilot tasks get autofix on; existing
  non-autopilot tasks get it off.
- The agent is told to push to the same branch, never to force-push or open a new
  PR; Delilah doesn't push for it.
- This amends decision #29 for CI failures only; the decision log gets the
  amendment.
