# Feature Specification: Readable background-task notifications in the chat view

**Feature Branch**: `011-readable-task-notifications`

**Created**: 2026-10-26

**Status**: Draft

**Input**: Task LIBRE-23. User note (Spanish, translated): "Messages in the chat sometimes come out in this format: `<task-notification> <task-id>… <tool-use-id>… <output-file>… <status>completed</status> <summary>Agent "WP-07 update status.md" finished</summary> <note>…</note> <result>## WP-07 — done …</result> <usage>…</usage> </task-notification>` — that's chaos, we have to see what to do."

## Why

When a background command or a background sub-agent finishes, Claude Code tells the
session by sending a message *as if the user had typed it*: an XML block with the task's
id, the id of the tool call that started it, an output file path, its status, a summary
line, an explanatory note, the sub-agent's full report and its usage. In the installed app
(v1.1.1) the chat view shows that block raw, as a user bubble, in the middle of the
conversation. With an orchestrated feature that means a dozen walls of XML the user did not
write.

`main` already went one step further (spec `010-chat-background-tasks`, not yet released):
the block becomes one muted line, "Tarea en segundo plano terminada · Agent "X" finished".
That fixes the noise but throws the content away:

- the sub-agent's **report** (often the most useful text of the turn) is no longer
  visible anywhere in the chat;
- a task that **failed** or was **stopped** looks exactly like one that completed;
- how long it took and how much it did (tool uses, tokens) is lost;
- when the report is too long for the notification, Claude Code sends it in a separate,
  hidden **hand-back** message ("This agent's report was delivered to you as a message
  from …"); Delilah drops hidden messages, so that report is never shown at all.

The user wants the chat to read like a conversation: one compact, readable line per
finished background task, with its outcome, and the report one click away.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - A finished background task reads as one clear item, never as XML (Priority: P1)

When a background command or sub-agent finishes, the conversation shows a single compact
item instead of the raw block: what finished (the agent's description or the command), its
outcome (terminada / falló / detenida), and, when known, how long it ran and how many tool
uses and tokens it took. Its status is visible at a glance (a failed or stopped one is
styled differently from a completed one). Nothing of the XML (tags, task id, output file
path, the boilerplate note) appears in the chat.

**Why this priority**: it is the direct fix for the user's report and the base the other
stories build on.

**Independent Test**: open the conversation of a session whose transcript contains task
notifications (the fixtures in `crates/delilah-agents/tests/fixtures/stream-background*.jsonl`,
or a real orchestrated session). Each notification is one item with label, outcome and
metrics; no `<task-notification>` text is visible.

**Acceptance Scenarios**:

1. **Given** a transcript with a completed background-agent notification with usage,
   **When** the conversation is shown, **Then** one item reads like "Subagente terminado ·
   WP-07 update status.md · 1 min 30 s · 2 herramientas · 12,3 k tokens", and no user
   bubble with that text exists.
2. **Given** a notification for a background command ("Background command "Sleep 25
   seconds" completed (exit code 0)"), **When** shown, **Then** the item names the command
   and its outcome (exit code 0 shown as success).
3. **Given** a notification with status `failed`, `stopped` or `killed`, or a command that
   ended with a non-zero exit code, **When** shown, **Then** the item says so in words
   ("falló", "detenida") and is styled as a warning, not as a success.
4. **Given** a notification without usage or summary, **When** shown, **Then** the item
   still appears with a generic label ("Tarea en segundo plano terminada") and no empty
   placeholders.

---

### User Story 2 - Read the sub-agent's report from the chat (Priority: P1)

The item of a finished background sub-agent can be expanded to show the sub-agent's report,
formatted the same way Claude's own messages are (headings, lists, code). It starts
collapsed so the conversation stays short. When the report was delivered separately as a
hand-back message, the item shows that report instead of the "delivered as a message"
placeholder, and the hand-back is not shown a second time anywhere else.

**Why this priority**: the report is the content the user actually needs; without it the
item is just a status line. It ranks with US1 because the current `main` behaviour hides it
entirely.

**Independent Test**: in a conversation with a background agent that returned a report
inline, expand its item: the report is rendered as formatted text. In one where the report
came as a hand-back, expanding shows the hand-back's report.

**Acceptance Scenarios**:

1. **Given** a notification whose result holds the report, **When** the user expands the
   item, **Then** the report is shown formatted; collapsing hides it again.
2. **Given** a notification whose result says the report was delivered as a message from
   the agent, and a hand-back from that agent exists in the session, **When** the user
   expands the item, **Then** the hand-back's report is shown, without the hand-back's
   preamble ("[Subagent hand-back] The text below is the final report…") or its indentation.
3. **Given** the same case but the hand-back cannot be found, **When** the user expands the
   item, **Then** it says the report is not available instead of showing the placeholder
   sentence.
4. **Given** a background command (no report), **When** shown, **Then** the item has no
   expand control.
5. **Given** a long report, **When** expanded, **Then** it does not push the rest of the
   conversation out of reach (the user can collapse it again from its top, and selecting /
   copying the text works as for Claude's messages).

---

### User Story 3 - No raw notification anywhere else (Priority: P2)

Any other place in the app that shows text "from the user" for a session — the user
bubbles of the conversation, the task's last-message preview, the inbox, notifications —
never shows a raw `<task-notification>` or hand-back block, also in less common shapes:
several notifications in one message, a notification preceded by other text, or a
notification inside a message with several parts.

**Why this priority**: the main path is covered by US1; these are the shapes and surfaces
where the raw text could still leak.

**Independent Test**: feed a transcript with each odd shape and check the conversation and
the task's previews.

**Acceptance Scenarios**:

1. **Given** one message with two notifications, **When** shown, **Then** there are two
   items, one per notification, in their order.
2. **Given** a message with the user's own text followed by a notification (or the other
   way round), **When** shown, **Then** the user's text is a normal user bubble and the
   notification is an item; no XML in the bubble.
3. **Given** a session whose last "user" message is a notification or a hand-back, **When**
   the task list or inbox shows the last thing the user said, **Then** it shows the user's
   real last message (as today for the hooks-based preview), not the notification.

---

### Edge Cases

- The same task id notifies more than once (a sub-agent the session resumed): each
  notification is its own item, in time order; each one shows the report of that run.
- A notification for a task Delilah never saw start (started before Delilah attached, or in
  a resumed session): the item still shows from the notification alone.
- Unknown status values: shown as finished with the raw status word, neutral style.
- Malformed or truncated XML (missing closing tags): whatever fields can be read are
  shown; if nothing can be read the item falls back to the generic label; the raw text is
  never shown.
- Report text containing HTML-like or XML-like text (`<name>`, `&lt;`): shown as text,
  with entities decoded, never interpreted as markup.
- Hand-back from an agent that has no notification in the session (e.g. a foreground
  agent): not shown as a separate item; it stays hidden, as other hidden messages do today.
- Very long reports (tens of KB): the conversation stays responsive; the collapsed item
  costs no more than a line.
- Older clients (a UI from before this change talking to a newer daemon) keep showing the
  existing one-line marker.
- Terminal (PTY) sessions shown in the chat view use the same transcript reading and get
  the same items for free; the terminal itself is unchanged (Claude Code draws it).

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The conversation MUST show each background-task notification as one item,
  never as a user message and never with its raw markup.
- **FR-002**: The item MUST show: the kind (sub-agent or background command), its label
  (agent description or command description, taken from the summary), and its outcome in
  Spanish words: completed, failed, stopped/killed, or the raw status if unknown.
- **FR-003**: A command's exit code MUST count towards its outcome: a non-zero exit code is
  shown as a failure, with the code.
- **FR-004**: When the notification carries usage, the item MUST show duration, tool uses
  and tokens in short human form; missing values are left out, never shown as zero or empty.
- **FR-005**: Failed and stopped items MUST be visually distinct from completed ones.
- **FR-006**: An item with a report MUST be expandable to show the report formatted as
  Claude's messages are (same Markdown rendering and the same safety rules: remote images
  only on click, no raw HTML), collapsed by default.
- **FR-007**: When the report was delivered as a separate hand-back from the same task,
  the item MUST show the hand-back's report (with the harness preamble and indentation
  removed); the hand-back MUST NOT appear as its own item or bubble.
- **FR-008**: When a referenced hand-back is missing, the item MUST say the report is not
  available instead of showing the placeholder text.
- **FR-009**: Several notifications in one message MUST produce one item each, in order;
  user text in the same message MUST stay a normal user bubble without the markup.
- **FR-010**: The boilerplate note, the task id, the tool-use id and the output-file path
  MUST NOT be shown.
- **FR-011**: The session's "last user message" shown in task lists, inbox and
  notifications MUST ignore notifications and hand-backs (keep and extend today's rule).
- **FR-012**: The change to what the daemon sends MUST be additive: a client that only
  knows today's one-line marker keeps working and shows that line.
- **FR-013**: The existing live "background work" strip (spec 010) MUST keep working as is;
  this feature is about the conversation history, not the live strip.
- **FR-014**: All new UI text MUST be in Spanish through the app's translation helper, like
  the rest of the chat.

### Key Entities

- **Background-task notification**: what the agent's CLI sends when background work
  finishes. Attributes used: task id (to match a hand-back and repeated notifications),
  status, summary (label and, for commands, exit code), optional report, optional usage
  (tokens, tool uses, duration), time. The tool-use id, output file and note are read but
  not shown.
- **Hand-back**: a hidden message from a sub-agent carrying its full report, identified by
  the sender's task id; it belongs to the notification of the same task id that points at
  it.
- **Background item (conversation entry)**: what the chat shows for one notification:
  kind, label, outcome, metrics, report or "report not available".

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: In every fixture and in a real orchestrated session with 10+ background
  agents, 0 notifications or hand-backs are shown as raw markup or as user bubbles.
- **SC-002**: For each finished sub-agent, the user can read its report in at most one
  click from the conversation.
- **SC-003**: A failed or stopped background task can be told apart from a completed one
  without expanding anything, in 100% of the fixture cases.
- **SC-004**: A conversation with 15 collapsed background items takes no more vertical
  space than 15 single lines.
- **SC-005**: A UI from before this change, pointed at the new daemon, still shows one
  line per notification and no errors.

## Assumptions

- The notification format is the one in the fixtures and real transcripts (Claude Code
  2.1.x): tags `task-id`, `tool-use-id`, `output-file`, `status`, `summary`, `note`,
  `result`, `usage` (`subagent_tokens`, `tool_uses`, `duration_ms`), and transcript lines
  marked with `origin.kind = "task-notification"`. Hand-backs are hidden transcript lines
  with `origin.kind = "peer"`, `handback: true` and the sender's task id. Delilah reads them
  defensively: a missing field is not an error.
- The kind is inferred from the summary wording ("Agent "…" finished" vs "Background
  command "…" …") and the task id; when it can't be inferred, the item is a generic
  "background task".
- Only the conversation history is in scope; the live background strip and the terminal
  view are unchanged. No new settings, no database migration.
- Reports are read from the transcript; the output file on disk is not read (it may be
  gone, and it is the raw sub-agent transcript, not the report).
- The item stays in the conversation's time order, where the notification arrived; it is
  not moved next to the tool call that started the task.
