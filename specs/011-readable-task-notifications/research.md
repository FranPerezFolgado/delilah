# Research: Readable background-task notifications in the chat view

Sources: the real transcript of an orchestrated session (LIBRE-16, 1,400+ lines, 11
notifications, 2 hand-backs), the stream fixtures
`crates/delilah-agents/tests/fixtures/stream-background*.jsonl` (Claude Code 2.1.292), and
the code on `main` after PR #30.

## R1. What a notification looks like in the transcript

**Finding.** A finished background task is written as a `type: "user"` line, `isMeta`
unset, `origin: {kind: "task-notification", producer: "session-task", runId}`, and
`message.content` a **string**:

```text
<task-notification>
<task-id>ad1c30dd1649f3fe6</task-id>
<tool-use-id>toolu_01…</tool-use-id>
<output-file>/private/tmp/…/tasks/ad1c30dd1649f3fe6.output</output-file>
<status>completed</status>
<summary>Agent "WP-07 update status.md" finished</summary>
<note>A task-notification fires each time this agent stops …</note>
<result>## WP-07 — done …</result>                 (agents only)
<usage><subagent_tokens>12317</subagent_tokens><tool_uses>2</tool_uses><duration_ms>89665</duration_ms></usage>
</task-notification>
```

Commands carry no `result`/`usage`, and their summary reads
`Background command "Sleep 25 seconds in background" completed (exit code 0)`. Stopped
tasks have `status` `stopped` (stream fixture `stream-background-stop.jsonl`). `result` text
is entity-escaped in places (`&lt;name&gt;`) and not in others (`<name>`), so the parser
cannot use an XML library: it must find tags by name and take everything up to the
matching closing tag, and decode the five basic entities afterwards.

**Decision.** A small tag scanner in `delilah-agents::transcript` (no new dependency):
`<task-notification>` … `</task-notification>` (or end of text when the closing tag is
missing), and inside it the first `<tag>…</tag>` for each known tag. The `result` is taken
from `<result>` to the **last** `</result>` before `</usage>`/`</task-notification>`, since
a report may itself quote `</result>`.

**Alternatives.** `quick-xml`: rejects the unescaped `<` that real reports contain. Regex
crate: not needed for fixed tag names.

## R2. Hand-backs, and notifications that are never written

**Finding.** When a background agent ends with a hand-back (`SubagentHandback`), the
transcript holds a **hidden** user line: `isMeta: true`,
`origin: {kind: "peer", from: <task id>, senderTaskId, name: <agent type>, body: <report>,
handback: true}`, and `content` "Another Claude session sent a message:\n<agent-message
from=…>…". In both hand-backs of the LIBRE-16 transcript, the notification for that task
id appears **only** as `queue-operation` `enqueue` … `remove` lines: Claude Code removed it
from the queue once the hand-back was delivered, and no `user` notification line exists.
The notification's `result`, when it is written, says "This agent's report was delivered
to you as a message from "<id>" … it is not repeated here."

`origin.body` starts with a fixed preamble ("[Subagent hand-back] The text below is the
final report … The report follows:\n") and every report line is indented by two spaces.

**Decision.**
- A hand-back line (`isMeta` and `origin.kind == "peer"` and `origin.handback == true`) is
  read **before** the `isMeta` filter and becomes a background item of its own:
  kind `agent`, status `completed`, label = `origin.name` (the agent type), task id =
  `origin.from`, report = `body` without the preamble (cut at the first
  "The report follows:\n" when present) and with one level of two-space indentation
  removed. Other `isMeta` lines stay hidden.
- A notification whose `result` is the "delivered to you as a message" placeholder gets
  `report: null` and `report_in_handback: true`.
- Pairing the two (FR-007) happens in **one place, the UI**, over all loaded entries
  (R4), so it works across pages.

**Alternatives.** Reading `queue-operation` lines to recover the dropped notification
(its label and usage): rejected, `remove` means Claude Code never delivered it, and
queue lines also carry user prompts typed while busy; mixing them in would duplicate
entries. Pairing in the daemon: the conversation is paginated by entries
(`parse_page`, `PAGE`), so a notification and its hand-back can be on different pages; the
orphan-results mechanism shows what cross-page pairing costs on the server. The client
loads pages forward from the start and keeps asking while `complete` is false
(`ConversationView.tsx` `load()`), so it ends up holding every entry; while pages are
still arriving, a pair may be split for a moment. The fold is therefore **derived on every
render from the raw entries** (never stored), so a hand-back arriving on a later page just
completes the pair on the next render, with nothing to undo. "Informe no disponible" is
shown only once the conversation is `complete`; before that an unpaired placeholder says
"Cargando informe…".

## R3. Where raw notification text can still leak

**Finding.**
1. `transcript::line_items` turns a user line into a marker only when the joined text
   *starts with* `<task-notification>` or `origin.kind` says so; otherwise
   `message_items` → `Entry::User` with the raw text. A message with several notifications,
   or user text plus a notification (content arrays, or text queued together while the
   agent was busy), leaks.
2. `hooks::agent_prompt` keeps `UserPromptSubmit` prompts out of the task's
   last-message/inbox line only when the prompt *starts with* `<task-notification>`; a
   hand-back prompt ("Another Claude session sent a message:") is not recognised.
3. The live socket (`/sessions/{id}/live`) sends Claude's text and progress only; user
   lines come from the transcript. Nothing to change.
4. The live `BackgroundStrip` reads `Progress.background` (spec 010), not the transcript.
   Out of scope (FR-013).

**Decision.** Replace the "starts with" test with a split of each user text block into
segments: plain text runs (kept as `Entry::User` when not empty and not injected) and
notification blocks (each one item). Item ids: the existing `t:<uuid>:<block>` for the
first item of a block, `t:<uuid>:<block>:<k>` for the following ones, so ids stay unique
and stable and keep the `t:` prefix the UI's merge relies on. The UI's `arrived()` check (live
"writing" bubbles) only looks at `kind === "claude"` entries, so marker ids with extra
segments cannot be mistaken for a message being written. `agent_prompt` uses the same
detection (`contains("<task-notification>")`, or starts with the hand-back sentence).

## R4. Contract shape

**Finding.** `Entry::Marker { marker: "background-done", detail }` is in `main` but not
released; `app/src/api.ts:428` types it; `ConversationView.tsx:318-327` renders it; the
UI shows nothing for unknown markers and ignores unknown fields.

**Decision.** Keep `marker: "background-done"` and `detail` and add one optional object
`background` (see [contracts/api.md](contracts/api.md)).

Trade-off, stated honestly: the marker with `detail` is only on unreleased `main`, so
"old clients" that know it are few (previews, dev builds). The deciding reason is a
different one: `ConversationView` treats any entry whose `kind` it does not know as a
card (its last branch renders `AskCard`), so a new `kind: "background"` would break any
UI from before this change, while an extra field on a known marker is ignored. The cost
is a `Marker` variant that carries a large optional payload its siblings
(`compacted`, `stopped`) never use.

The fold (R2) is a pure function in the UI, `foldBackground(entries)`: for each item with
`report_in_handback`, the nearest hand-back item (`source: "handback"`) with the same
`task_id` is merged into it (its report fills the item; the hand-back item is dropped);
an unmatched hand-back stays an item of its own; an unmatched placeholder shows "Informe
no disponible".

## R5. Report size

**Finding.** Entries cut texts at `MAX_TEXT` (4 KB); tool results have a "full detail"
endpoint. Real reports run 1–6 KB; a review report can reach 15–20 KB. The conversation
page holds at most `PAGE` entries, and a sub-agent report is rare compared with tool calls.

**Decision.** Cut reports at a separate `MAX_REPORT` of 16 KB, with
`report_truncated: true` and a "(informe recortado)" note in the UI. That covers the
reports seen so far (1–6 KB typical) and bounds a page with tens of agents to well under
a megabyte. No new endpoint.

**Alternatives.** 64 KB inline: megabytes per page in long orchestrated sessions,
re-sent on every poll. A detail endpoint reusing the tool-detail mechanism: the right
next step if truncated reports turn out to be common; not built now (YAGNI).

## R6. Label, kind, outcome

**Decision.** From the summary's first line:
- `Agent "<label>" …` → kind `agent`, label in quotes;
- `Background command "<label>" …` → kind `command`, label in quotes; `(exit code N)` →
  `exit_code: N`;
- anything else → kind `task`, label = the whole first line (≤ 200 chars, as today).

Outcome shown by the UI: `failed`, or `completed` with a non-zero `exit_code` → "falló";
`stopped` / `killed` → "detenida"; `completed` → "terminada"; anything else → the raw word.
The daemon passes `status` as read (lowercase), never inventing one; a missing status is
`null`.

## R7. Metrics formatting

**Decision.** A shared `app/src/format.ts` holds `duration(secs)` (moved from
`BackgroundStrip`, "12 s" / "1 min 30 s") and `formatTokens(n)` (moved from
`app/src/playground/art/kinds.ts`, which re-exports it; "950" / "12.3K" / "1.2M"). The item
shows `duration(duration_ms / 1000)`, "N herramientas" (singular for 1) and
"12.3K tokens". Missing values are left out.
