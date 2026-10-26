# Feature Specification: Chat mode asks for permission as often as terminal mode

**Feature Branch**: `009-chat-mode-permissions`

**Created**: 2026-10-26

**Status**: Draft

**Input**: User description: "It seems like autopilot is not working as it should with chat
mode. It stops a lot and asks permissions for everything. Analyze it and check what can we
do, maybe TDD and testing how it should work before doing anything." (task LIBRE-20)

## Background: what was observed

From the installed app's task history (2026-10-26):

- All 24 permission asks ever recorded came from chat-mode sessions. No terminal-mode
  session has ever produced one.
- LIBRE-15 and LIBRE-17 ran on autopilot in terminal mode with no permission pauses. After
  they switched to chat mode (11:29 and 11:32), autopilot paused four times with "needs
  permission: Bash". There were more asks for routine work inside the task's own
  workspace: writing `specs/…/tasks.md`, `sed -i`, scripted file edits, `git commit`, and
  running the repo's `.specify/scripts/…`.
- The same kinds of action ran without asking in terminal mode.

Working hypothesis, to be confirmed before any change:

- A terminal session gets the permission behaviour the user has set as their default in
  Claude Code. On this machine that is auto mode.
- A chat-mode session is started differently and does not get that default. It falls back
  to asking about everything outside the user's short allow list.
- Autopilot then does what it was built to do with every one of those asks: it pauses
  (decision #43).

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Autopilot runs as unattended in chat mode as in terminal mode (Priority: P1)

A user starts a spec task on autopilot in chat mode, or switches a running autopilot task to
chat mode. The agent plans, implements and reviews. It writes files in its workspace, runs
the repo's scripts and tests, and commits, without autopilot pausing for permission any
more often than it would for the same work in terminal mode.

**Why this priority**: this is the reported bug. Autopilot exists to run without the user,
and in chat mode it currently stops on almost every step.

**Independent Test**: start the same autopilot task twice, once per runtime, against an
agent that performs the same sequence of routine actions. Count the permission pauses. Chat
mode must not pause more than terminal mode.

**Acceptance Scenarios**:

1. **Given** a chat-mode autopilot task, **When** its agent writes a file inside its
   workspace, runs a repo script, edits files with shell commands or commits, **Then**
   no permission card is raised and autopilot keeps running, exactly as in terminal mode.
2. **Given** a chat-mode autopilot task, **When** its agent attempts something that terminal
   mode would also stop and ask about, **Then** a permission card is raised and autopilot
   pauses with "needs permission: <tool>" (decision #43 is unchanged).
3. **Given** a running autopilot task in terminal mode, **When** the user switches it to chat
   mode between turns, **Then** the relaunched session keeps the same permission behaviour,
   and autopilot carries on without new permission pauses for work it was already doing.

---

### User Story 2 - Hands-on chat-mode work isn't interrupted by needless cards (Priority: P2)

A user works with an agent in chat mode without autopilot. The agent edits and runs things
in its workspace without a "¿Permitir?" card for every step, just as the terminal would not
prompt for them.

**Why this priority**: same cause and same fix as P1. It matters less because a user is
present to click, but today chat mode is noticeably more tiring than the terminal.

**Independent Test**: in a chat-mode task without autopilot, have the agent perform the
routine actions from US1 and count the permission cards. The count must match the terminal.

**Acceptance Scenarios**:

1. **Given** a chat-mode task without autopilot, **When** the agent performs a routine
   workspace action that the terminal would not prompt for, **Then** no permission card
   appears.
2. **Given** a chat-mode task, **When** the agent does need permission, **Then** the card
   and its answers (once, always in this task, deny) work as they do today, including
   remembered rules.

---

### User Story 3 - The behaviour is proven before it is changed, and stays proven (Priority: P1)

Before any fix, the team finds out how the real Claude CLI behaves in the way chat mode
starts it. The questions are which tool uses reach the "ask" path, with which permission
settings, and how that compares with a terminal session. The expected behaviour is then
written down as failing tests, so the fix is test-first and a future regression is caught.

**Why this priority**: the user asked for this explicitly ("TDD and testing how it should
work before doing anything"). The CLI's chat protocol is thinly documented and may change,
so the fix must rest on observed behaviour, not on assumption.

**Independent Test**: the spike's findings are recorded and repeatable. The new tests fail
on the current code and pass after the fix.

**Acceptance Scenarios**:

1. **Given** the installed Claude CLI, **When** the spike runs the same set of routine
   actions in chat-mode startup with each candidate permission setting, **Then** it records
   which actions were asked about, allowed or denied. The record includes the CLI version.
2. **Given** the current code, **When** the new tests run, **Then** they fail and show that
   a chat-mode task session does not get the permission behaviour of the terminal.
3. **Given** the fix, **When** the same tests run, **Then** they pass. The existing
   permission-card, remembered-rule and autopilot-pause tests still pass.

---

### Edge Cases

- **The user's default is not auto.** If the user's own default permission behaviour is
  "ask", terminal mode asks and chat mode should ask the same. The fix must give parity
  with the terminal. It must never grant more than the terminal would.
- **Idea conversations and read-only sessions.** These keep their stricter, explicit modes
  ("never ask", plan). This feature does not change them.
- **Project sessions and shells.** These always use a terminal. They are unaffected.
- **Something the automatic mode refuses outright.** If the chosen behaviour denies an
  action instead of asking, the agent sees the denial and carries on, as in terminal mode.
  No card is raised and autopilot does not pause, but the denial must be visible in the
  conversation.
- **Remembered "always in this task" rules.** These keep working, and a request that
  matches one is still answered without a card.
- **Older or newer CLI.** If the CLI does not accept the chosen permission setting in chat
  mode, the session must fail visibly at start (the existing start check). It must not
  silently fall back to asking for everything.
- **Switching runtime mid-task.** Sessions relaunched with resume pick up the new behaviour
  without the user doing anything.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: Before any production change, a spike MUST run the real Claude CLI the way
  chat mode starts it. It MUST record, for each candidate permission setting (at least
  "none given", which is today's behaviour, and the user's terminal default), which
  routine workspace actions are asked about, allowed or denied. The findings MUST be saved
  with the CLI version under `docs/spikes/`.
- **FR-002**: Failing automated tests MUST be written before the fix. They MUST state that
  a chat-mode task session is started with the same permission behaviour as a terminal
  session of the same task. They MUST also state that idea conversations and read-only
  sessions keep their own modes in both runtimes.
- **FR-003**: A chat-mode task session (autopilot or not) MUST be given the same permission
  behaviour as that task's terminal session would have, so that routine workspace actions
  that do not prompt in terminal mode do not raise permission asks in chat mode.
- **FR-004**: The fix MUST NOT grant more than terminal mode does. It MUST NOT bypass
  permissions, it MUST NOT widen the allow list, and idea conversations and read-only
  sessions MUST keep their current modes.
- **FR-005**: A permission ask that still reaches Delilah from a chat-mode autopilot
  session MUST keep pausing autopilot with "needs permission: <tool>" (decision #43
  unchanged).
- **FR-006**: Permission cards, their three answers, and remembered task rules (ADR-001)
  MUST keep working in chat mode.
- **FR-007**: An end-to-end test with the fake agent MUST reproduce the reported scenario:
  a chat-mode autopilot task whose agent does routine workspace work. It MUST show no
  permission pause after the fix, and still a pause for an action that needs permission.
- **FR-008**: If the CLI rejects the permission setting chat mode passes, the session MUST
  fail visibly at start with a message that names the cause.
- **FR-009**: The docs MUST describe how chat mode now chooses its permission behaviour,
  and the "Chat mode, known problems" entry MUST be updated. The decision MUST be recorded
  in `docs/decisions.md`. The affected docs are `docs/status.md`,
  `docs/architecture.md`, `docs/decisions.md`, and the README if what the user sees
  changes.

### Key Entities

- **Session permission behaviour**: how a Claude session decides whether a tool use runs,
  is refused, or is asked about. It is chosen per session kind: task session, idea
  conversation or read-only session. It must be equal across the two runtimes for the
  same kind.
- **Permission ask**: a request that reaches Delilah from a chat-mode session. It is shown
  as a card and an inbox item, and on autopilot it causes a pause.
- **Spike record**: the observed behaviour per permission setting and CLI version, used as
  the evidence for the fix.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: For the same sequence of routine workspace actions (write a file in the
  workspace, run a repo script, edit with a shell command, commit), a chat-mode session
  raises the same number of permission asks as a terminal session. On this machine's
  setup that number is 0.
- **SC-002**: A chat-mode autopilot task completes plan → implement → review with no
  "needs permission" pause caused by such routine actions.
- **SC-003**: Over a week of daily use after release, every permission ask recorded in
  history matches one the user would also have seen in terminal mode. There are none of
  the routine kind listed in the Background.
- **SC-004**: The new tests fail on the code before the fix and pass after it, and the
  whole existing test suite still passes.

## Assumptions

- "Terminal mode's behaviour" means what Claude Code applies to an interactive session that
  Delilah starts without a permission flag: the user's own default. On this machine that
  default is auto mode.
- The spike may show that chat mode has to name that behaviour explicitly, because the
  CLI's print mode does not apply the user's interactive default. In that case Delilah
  passes the same behaviour the terminal would get. The spike decides the exact mechanism;
  the requirement is parity.
- Running the spike needs a logged-in Claude CLI on the developer's machine. It runs in a
  throwaway fixture repo (`scripts/make-fixture-parent.sh`), never against work repos or
  the installed app.
- Decision #43 (permission asks always pause autopilot) stays. This feature removes asks
  that should never have happened. It does not answer real ones automatically.
- Out of scope: a UI to review or revoke remembered rules, answering permission asks
  automatically on autopilot, and chat mode for project sessions.
