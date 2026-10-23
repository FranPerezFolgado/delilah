# Feature Specification: Chat view for sessions, and an experimental chat mode

**Feature Branch**: `feature/LIBRE-13`
**Created**: 2026-10-23
**Status**: Draft
**Input**: Task LIBRE-13: "Vista de chat para las sesiones de tareas, como Claude Desktop, y un «modo chat» experimental opcional por tarea." Context in [docs/spikes/2026-10-23-chat-mode-notes.md](../../docs/spikes/2026-10-23-chat-mode-notes.md). Three steps: (1) a spike on `claude -p` with stream-json in and out; (2) a chat view built from Claude's transcript for every task and project session, with the PTY, `--resume`, hooks and the terminal unchanged; (3) an opt-in per-task chat mode where the task runs on stream-json instead of the PTY. Out of scope: agents other than Claude Code.

## Why

Today only the idea conversation has a chat view, and it shows little: the user's
messages, Claude's closing message for each turn, and question cards. Every other session
(task sessions, project sessions) is a terminal. To follow what an agent is doing, the
user has to read a TUI that scrolls fast and is hard to skim. They cannot fold away a
long command output, see an edit as a diff, or scroll back to "what did it decide three
turns ago".

Claude Code already writes every message of a session to its transcript: assistant text,
tool calls and their results. Delilah can show that as a conversation without changing
how agents run. That is the first, safe step (US1, US2). The second step goes further: an
opt-in, experimental mode where Delilah talks to Claude over a structured stream instead
of a terminal. That gives live text as Claude writes it, with no TUI in between (US3, US4).
It is experimental because it replaces the runtime that every task uses today, so it
must never be the default, and a task must be able to fall back to the terminal.

Before step two is built, a spike checks that it is viable: hooks still reach Delilah,
one process takes several turns, and a session can move between the two runtimes
(US5). If the spike fails on those points, the chat mode stories are cut back and the
reasons are recorded. The chat view (US1, US2) does not depend on the spike.

## Clarifications

### Session 2026-10-23

- Q: How do approvals work in chat mode? → A: Option C: an "¿Permitir?" card in the conversation and the Bandeja, backed by Claude's permission prompt tool, with an allow list as the fallback if the spike shows the prompt tool does not work.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Read any session as a conversation (Priority: P1)

The user opens a task and goes to one of its sessions. Instead of the terminal, they see
the session as a conversation. Their own messages (and the task's initial prompt) are
bubbles on one side. Claude's text is on the other side, rendered as Markdown. Each tool
Claude used is a compact card in between: a command with its output, an edit shown as a
diff, a file read with its path, a search with its pattern. Cards start collapsed to one
line and expand on click. While Claude works, new messages and cards appear on their
own, one per message (not letter by letter). The terminal is one click away and is the
same live terminal as before.

**Why this priority**: it is the core value of the feature. It works for every session
that exists today, it does not change how agents run, and it ships on its own.

**Independent Test**: start a task, let the agent run a few commands and edit a file, and
open the session. The conversation shows the prompt, Claude's text, a command card with
its output and an edit card with a diff, in order. While the agent keeps working, new
entries appear without reloading. "Terminal" shows the live terminal, and "Chat" comes back.

**Acceptance Scenarios**:

1. **Given** a running task session, **When** the user opens it, **Then** they see the
   conversation so far (their prompts, Claude's text, tool cards) in the order it
   happened, scrolled to the end.
2. **Given** the conversation is open, **When** Claude writes a message or uses a tool,
   **Then** the new entry appears within a few seconds, and the view follows it only if the
   user was already at the bottom.
3. **Given** a command card, **When** the user expands it, **Then** they see the full
   command, its output (long output truncated with a way to see all of it), and whether it
   failed.
4. **Given** an edit or write card, **When** the user expands it, **Then** they see the
   file path and the change as a diff (a new file as all added lines), and can open the
   file in Delilah's editor.
5. **Given** a tool Delilah has no specific card for (including MCP tools), **When** it
   appears, **Then** it shows as a generic card with the tool name, its input and its result.
6. **Given** the conversation view, **When** the user clicks "Terminal", **Then** they get
   the live terminal of the same session. The choice is remembered for that session.
7. **Given** a session that is waiting for a permission prompt or another dialog that only
   the terminal can answer, **When** the user looks at the conversation, **Then** a notice
   says Claude is waiting in the terminal, with a button that opens it.
8. **Given** a session that ended or a task that was reopened later, **When** the user opens
   the session, **Then** the full conversation is still there, including turns from before
   a daemon restart and from before `--resume`.

---

### User Story 2 - Talk to the agent from the conversation (Priority: P1)

From the conversation view of a task or project session, the user writes to the agent in a
text field, as they already can in the idea conversation. A message sent while Claude is
working is queued, shown as "en cola", and delivered when the turn ends. Queued messages
can be removed. "Detener" interrupts the current turn. Question cards (AskUserQuestion)
appear inline in the conversation and can be answered there, as well as from the Bandeja.

**Why this priority**: a chat view the user can only read sends them back to the terminal
for every reply. With the composer, the conversation replaces the terminal for normal work.

**Independent Test**: in a task session, send a message while the agent is idle (it starts
a turn), then one while it works (queued, then delivered at the end of the turn). Press
"Detener" mid-turn (the turn stops). When the agent asks a question, the card shows inline,
and answering it there continues the turn.

**Acceptance Scenarios**:

1. **Given** an idle session, **When** the user sends a message, **Then** it appears as
   their bubble and Claude starts a turn.
2. **Given** a working session, **When** the user sends a message, **Then** it shows as
   queued and is delivered when the turn ends. A queued message can be removed before then.
3. **Given** a working session, **When** the user presses "Detener", **Then** the turn
   stops, and queued messages are kept, not sent.
4. **Given** the agent asks a question, **When** the user answers the inline card, **Then**
   the answer reaches the agent, and the matching Bandeja item is resolved. The reverse
   also holds: an item answered in the Bandeja updates the card.
5. **Given** the idea conversation, **When** the user opens it, **Then** it keeps all its
   current behaviour (the "Nueva tarea" card, Crear) and also shows Claude's intermediate
   text and tool cards.

---

### User Story 3 - Run a task in chat mode (experimental) (Priority: P2)

When creating or starting a task, the user can turn on "Modo chat (experimental)". The
task's agent then runs without a terminal: Delilah talks to Claude over a structured stream.
The conversation looks the same as in US1 and US2, plus Claude's text appearing live as it
is written. Everything else about the task stays the same. Its states and phases still
come from the same signals, and so do the Bandeja, usage-limit pauses, saving mode
handoffs, autopilot, previews and the PR. The terminal and `delilah attach` are not
available for that task's sessions, and both say so clearly instead of failing silently.

**Why this priority**: it delivers the "like Claude Desktop" experience (live text, no TUI)
but replaces the runtime, so it is opt-in and experimental. It depends on US1/US2's view
and on the spike.

**Independent Test**: create a task with chat mode on and start it. The agent works, and
its text appears live while it writes. Tool cards and the final message are the same as
in PTY mode. The task moves through its phases and states as usual. "Terminal" is
disabled with an explanation, and `delilah attach <task>` prints that the session runs in
chat mode.

**Acceptance Scenarios**:

1. **Given** the new-task or start dialog, **When** the user looks at the options, **Then**
   there is a "Modo chat (experimental)" switch, off by default, with a one-line warning
   that it is experimental.
2. **Given** a task in chat mode, **When** Claude writes a reply, **Then** the text appears
   progressively while it is written, and once the message is complete it is the same
   entry the view would show in PTY mode. The partial text is never kept after the message
   is complete.
3. **Given** a task in chat mode, **When** the agent works, waits, asks a question, hits
   a usage limit or ends a turn, **Then** the task's state, phase, Bandeja items and
   notifications behave as for a PTY task.
4. **Given** a task in chat mode, **When** the agent needs a tool the user would normally
   approve in the terminal, **Then** the user gets an "¿Permitir?" card in the conversation and in the Bandeja,
   decides there, and the agent continues with that decision. If the spike shows this
   cannot work, the agent runs with an allow list instead, and a refused tool shows in the
   conversation with the option to switch to the terminal (US4).
5. **Given** a task in chat mode, **When** the user presses "Detener", **Then** the turn is
   interrupted, the session stays usable for the next message, and the conversation shows
   that the turn was stopped.
6. **Given** a chat-mode session, **When** the user tries "Terminal" in the app or
   `delilah attach` from the CLI, **Then** the app shows the option disabled with the
   reason, and the CLI and the API refuse with a clear message.
7. **Given** the daemon restarts while a chat-mode task is running, **When** it comes back,
   **Then** the task's session is resumed in chat mode with its conversation intact, as
   PTY sessions are today.

---

### User Story 4 - Switch a chat-mode task back to the terminal (Priority: P2)

A chat-mode task that misbehaves (stuck, waiting for something the stream cannot show)
can be switched to the terminal between turns. The same conversation continues in a PTY,
and from then on the terminal and `delilah attach` work. The user can switch it back to
chat mode between turns.

**Why this priority**: it is the escape hatch that makes an experimental runtime safe to
try on real work.

**Independent Test**: start a task in chat mode, let it finish a turn, choose "Pasar a
terminal". The terminal opens on the same conversation (Claude remembers the earlier turns),
and the chat view still shows the whole history. Switch back to chat mode. The next
message runs over the stream.

**Acceptance Scenarios**:

1. **Given** a chat-mode task between turns, **When** the user chooses "Pasar a terminal",
   **Then** the session continues in a PTY with the same conversation, and the task is no
   longer in chat mode.
2. **Given** a chat-mode task in the middle of a turn, **When** the user wants to switch,
   **Then** they are told to wait for the turn to end or press "Detener" first.
3. **Given** a PTY task between turns, **When** the user turns chat mode on, **Then** the
   next turn runs in chat mode on the same conversation.

---

### User Story 5 - Know whether chat mode is viable before building it (Priority: P1, first)

Before any chat mode work, a spike runs Claude in print mode with structured input and
output and Delilah's settings file, and answers:

- (a) do hooks still reach the daemon;
- (b) does one process take several turns from its input;
- (c) does `--resume` work across the terminal and the stream, both ways;
- (d) what happens with folder trust and with a tool that needs approval;
- (e) what an interruption looks like.

The findings are written down next to the earlier Claude PTY spike. If (a)–(c) fail, US3
and US4 are cut back or dropped, and the reason is recorded.

**Why this priority**: the cost and shape of US3/US4 depend on its answers; it goes first.

**Independent Test**: the findings document exists, answers (a)–(e) with the Claude
version used, and the plan for US3/US4 follows from it.

**Acceptance Scenarios**:

1. **Given** the spike is done, **When** someone reads the findings, **Then** each of
   (a)–(e) has a yes/no or an observed behaviour, with the Claude Code version.
2. **Given** any of (a)–(c) fails, **When** the chat mode work is planned, **Then** the
   decisions log records what was cut and why.

### Edge Cases

- **Transcript not there yet**: a session that has not had its first turn has no transcript.
  The view shows the initial prompt (if any) and the activity, and fills in once Claude
  writes.
- **Transcript format changes** (new Claude version): unknown lines, unknown content types
  and unknown fields are skipped, never fatal. Anything Delilah cannot map shows as a
  generic entry or not at all. The terminal still works.
- **Malformed or half-written lines** (Claude still writing): a line that does not parse is
  retried once it is complete. A line that never parses is skipped and logged.
- **Very large outputs** (huge command output, big file reads, long diffs): cards show a
  truncated preview. The view stays responsive with sessions of several thousand entries.
- **Thinking blocks and internal/system lines** (hook outputs, meta messages, compaction
  summaries): not shown as Claude's text. A compaction shows as a single marker
  ("conversación compactada").
- **Sub-agents** (Task/Agent tool): shown as one card with the sub-agent's prompt and final
  result, not its inner steps.
- **Images** pasted or produced in the session: shown as a placeholder with the type, not
  rendered.
- **Several sessions per task**: each has its own conversation. Saving mode handoffs start
  a new session with its own conversation, as today.
- **Shell sessions** (not Claude): no conversation; they keep the terminal.
- **Transcript missing for good** (deleted, or a session from before Claude saved
  transcripts): the view says there is no conversation to show and offers the terminal.
- **Chat mode and the Claude version**: if the installed Claude does not support what chat
  mode needs, starting a task in chat mode fails with a clear message and offers to start
  it in the terminal.
- **Chat-mode process dies mid-turn**: the session is marked as such (like a PTY that
  exits), and the user can resume it in either mode.
- **Folder trust in chat mode**: a new workspace must not leave a chat-mode task stuck on a
  dialog nobody can see (the spike finds out how print mode behaves).
- **Remote Control** on a chat-mode task: unavailable or working as documented by the
  spike, never silently broken.

## Requirements *(mandatory)*

### Functional Requirements

**Chat view (all sessions)**

- **FR-001**: Every Claude session (task and project sessions, idea conversations included)
  MUST have a conversation view built from Claude's own record of the session. Shell
  sessions keep only the terminal.
- **FR-002**: The conversation MUST show, in order: the user's messages (including the
  initial prompt and messages typed in the terminal), Claude's text as Markdown, and one
  card per tool call with its result.
- **FR-003**: Cards MUST be collapsible and start collapsed to one summary line. These
  tools MUST have a specific card: shell commands (command, output, failure), file edits
  and writes (path, diff), file reads (path, range), searches (pattern, scope, matches
  count), to-do updates, questions (the existing question card), and sub-agents (prompt
  and result). Any other tool MUST get a generic card (name, input, result).
- **FR-004**: The conversation MUST update by itself while the session runs, at least once
  per completed message, within a few seconds of Claude writing it. It MUST NOT need a
  reload.
- **FR-005**: Reading Claude's record MUST be tolerant. Unknown or malformed content is
  skipped and logged, never fatal, and never stops the session or its terminal. Delilah
  MUST NOT depend on that format for task state. State keeps coming from hooks only.
- **FR-006**: The conversation MUST survive daemon restarts, `--resume` and app restarts,
  and MUST cover sessions that existed before this feature when Claude's record of them
  is still on disk.
- **FR-007**: Each session MUST offer a one-click switch between "Chat" and "Terminal",
  remembered per session. The chat is the default for Claude sessions.
- **FR-008**: When the session waits for something only the terminal can answer (a
  permission prompt, a dialog), the conversation MUST say so and offer to open the terminal.
- **FR-009**: The conversation view MUST have the composer the idea conversation has:
  send, queue while working (shown as queued, removable), and "Detener". These work the
  same for task and project sessions.
- **FR-010**: Question cards MUST be answerable inline and stay in sync with the Bandeja
  both ways.
- **FR-011**: The idea conversation MUST keep its current behaviour ("Nueva tarea" card,
  Crear, history) and gain the richer content of FR-002/FR-003.
- **FR-012**: An edit or write card MUST let the user open the file in Delilah's editor.
- **FR-013**: Large content MUST be truncated in cards with a way to see all of it. The view
  MUST stay usable with sessions of thousands of entries.

**Spike**

- **FR-014**: Before chat mode is implemented, the spike's findings for questions (a)–(e) of
  US5 MUST be recorded in `docs/spikes/` with the Claude Code version, and US3/US4 MUST be
  adjusted to them (with a decision recorded if anything is cut).

**Chat mode (experimental, opt-in per task)**

- **FR-015**: A task MUST be able to run its Claude sessions in "chat mode" instead of the
  terminal. The flag MUST be per task, off by default, chosen in the new-task and start
  dialogs (and in `delilah new` / `delilah run`), and labelled experimental.
- **FR-016**: In chat mode, Claude's text MUST appear live while it is written. Once a
  message is complete, the conversation MUST hold the same entries it would hold in PTY
  mode. Partial text MUST NOT be stored.
- **FR-017**: In chat mode, task states, phases, Bandeja items, notifications, usage-limit
  pauses and resumes, saving mode handoffs, autopilot, previews and the PR flow MUST behave
  as for a PTY task, driven by the same signals.
- **FR-018**: In chat mode, sending a message, queueing, "Detener" and answering questions
  MUST work as in FR-009/FR-010. "Detener" MUST leave the session usable for the next
  message.
- **FR-019**: In chat mode, the terminal view and `delilah attach` MUST be unavailable. The
  app MUST show why, and the CLI and API MUST refuse with a clear message naming chat mode.
- **FR-020**: In chat mode, a tool that needs the user's approval MUST NOT leave the task
  silently stuck. It MUST raise an "¿Permitir?" card in the conversation and the Bandeja
  (allow once / deny). The answer reaches the agent, and the card and the Bandeja item stay
  in sync. Fallback, only if the spike shows this cannot work: an allow list, with refused
  tools shown in the conversation and the switch to the terminal offered.
- **FR-021**: A chat-mode task MUST be switchable to the terminal between turns, and back,
  continuing the same conversation. Switching mid-turn MUST be refused with a message
  saying to wait or press "Detener".
- **FR-022**: A chat-mode session MUST be resumed in chat mode after a daemon restart or a
  usage-limit pause, as PTY sessions are.
- **FR-023**: If the installed Claude cannot run chat mode, starting a chat-mode task MUST
  fail with a clear message and offer to start it in the terminal instead.

**Docs**

- **FR-024**: The new decisions MUST be recorded in `docs/decisions.md`:
  - Claude's transcript as a display source, amending #8 (state still from hooks only) and
    the transcript rejection in 004's research;
  - a second runtime for chat mode, amending #36;
  - the permission option chosen.

  `docs/status.md`, `docs/architecture.md` and the README MUST describe the chat view and
  chat mode.

### Key Entities

- **Conversation entry**: one item of a session's conversation, with its time and kind:
  a user message (with its delivery state), Claude's text, a tool call with its result,
  a question card, the "Nueva tarea" card, or a marker (turn stopped, conversation
  compacted). Tool entries carry the tool name, a summary for the collapsed line, the
  input, the result, and whether it failed.
- **Session runtime**: how a session's Claude runs, either a terminal (today) or chat
  mode. It decides which actions are available (terminal, attach, live text).
- **Task chat-mode flag**: the per-task choice of runtime for its Claude sessions. It
  persists, and it changes when the user switches between turns.
- **Live text**: the partial text of the message Claude is writing in chat mode. Shown
  only while it is being written, never stored.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: For any running Claude session, a new message or tool call shows in the
  conversation within 3 seconds of Claude writing it.
- **SC-002**: The user can read what an agent did in its last turn (commands, edits,
  closing text) without opening the terminal, in 100% of task and project sessions with a
  transcript on disk.
- **SC-003**: A session of 3,000 entries opens and scrolls without visible lag.
- **SC-004**: An unknown or malformed line never breaks the view or the session. A recorded
  session from a newer Claude with extra fields and content types still renders everything
  Delilah knows.
- **SC-005**: In chat mode, Claude's text starts to appear within 1 second of Claude
  producing it.
- **SC-006**: A chat-mode task completes the same flow as a PTY task (start → phases → PR)
  with no step that needs the terminal, apart from what FR-020 decides.
- **SC-007**: Switching a chat-mode task to the terminal between turns takes one action
  and keeps the full conversation, both in Claude's memory and in the view.
- **SC-008**: Turning chat mode off for all tasks leaves Delilah behaving exactly as before
  the feature, apart from the new chat view.

## Assumptions

- Claude Code keeps writing a per-session transcript, and hooks keep giving its path. The
  format is not a contract, which is why FR-005 exists.
- The chat view is the default for Claude sessions. Users who prefer the terminal switch
  once per session (FR-007). A global "always open the terminal" preference is not part of
  this feature.
- Messages typed directly in the terminal show up in the conversation as user messages,
  because they reach Claude's record too.
- Thinking is not shown. Delilah's own instructions to Claude (system prompt additions,
  hook outputs) are not shown as user messages.
- Live text (token by token) exists only in chat mode. PTY sessions update per message.
- Chat mode applies to every Claude session of the task: the primary session, saving mode
  steps and the review step. It does not apply to project sessions or idea conversations
  in this feature.
- Remote Control and other terminal-only extras may be unavailable in chat mode. The spike
  says which, and the UI says so.
- Only Claude Code is supported.

## Dependencies

- The spike (US5) gates US3/US4. US1/US2 do not depend on it.
- US3's view is US1/US2's view plus live text: chat mode is built after the chat view.
- Hooks with `--settings` and the clean agent environment (decisions #8, #10) are kept
  as they are.
