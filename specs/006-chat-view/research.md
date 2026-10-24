# Research: Chat view for sessions, and an experimental chat mode

Claude Code 2.1.292 on the personal Mac (2026-10-24). Starting notes:
[docs/spikes/2026-10-23-chat-mode-notes.md](../../docs/spikes/2026-10-23-chat-mode-notes.md).
R1–R6 are design decisions made now. R7 lists what the spike (US5, Phase 1 of the plan)
must confirm before chat mode is built; each item has the design that follows from a
"yes" and the fallback for a "no".

## R1. Where the conversation comes from: Claude's transcript, read on demand

- **Decision**: entries are built from Claude's transcript
  (`~/.claude/projects/<folder>/<session>.jsonl`, the file `claude::has_conversation`
  already locates). They are parsed by a pure, tolerant function in `delilah-agents`
  (`transcript.rs`) and **not copied into SQLite**. The daemon reads the file when the
  conversation is asked for, from a cursor, so the UI fetches only what is new.
  Delilah's own entries (queued messages, cards, markers) stay in `conversation_entries`
  as today and are merged in by time.
  - The path is the one the hooks send as `transcript_path` when the daemon has seen one
    (kept in memory on the live session); otherwise it is derived the way
    `has_conversation` does.
- **Rationale**:
  - The transcript already holds everything (FR-002) and survives restarts and
    `--resume` (FR-006). Sessions from before this feature work with no backfill.
  - Copying it would make Delilah own a second history and its sync problems.
  - Parsing a few MB of JSONL in Rust takes milliseconds, and a cursor avoids
    re-sending it.
  - It amends what 004's research rejected ("the transcript is Claude's private
    format"): the transcript is used for **display only**, and state still comes from
    hooks (#8). Hence the tolerant parser (FR-005) and a recorded decision.
- **Alternatives**:
  - Copy entries into `conversation_entries` as they appear. Rejected: double storage,
    and nothing gained since the file is the source.
  - More hooks (PreToolUse/PostToolUse for every tool). Rejected: no intermediate text.
  - Scraping the terminal. Rejected: unreadable.

## R2. Noticing new lines: poll the file while the session is live

- **Decision**: one daemon-wide loop looks at the transcript size of every live Claude
  session once a second, and also right after each hook for that session. When the file grows, it emits `Topic::Conversations`. The UI refetches from
  its cursor. A partial last line (no `\n` yet) is left for the next read.
- **Rationale**: SC-001 asks for 3 s. Hooks do not fire for intermediate assistant text,
  so polling is needed. One `stat` per second per live session costs nothing. No new
  dependency (`notify` is not in the workspace, and FSEvents coalescing adds latency
  anyway).
- **Alternatives**: the `notify` crate. Rejected: a new dependency for no gain at this
  scale.

## R3. Mapping: one shared function over Claude's content blocks

- **Decision**: `delilah-agents::transcript` maps a **message** (role plus content
  blocks: `text`, `tool_use`, `tool_result`, `thinking`, `image`) to entries:
  - user text → `user`;
  - assistant text → `claude`;
  - `tool_use` paired with its `tool_result` by `tool_use_id` → one `tool` entry;
  - a compaction summary → a `marker` entry;
  - thinking, hook outputs, meta and system-reminder lines, and Delilah's own injected
    text → skipped.

  Transcript lines wrap that message in an envelope (`type`, `uuid`, `timestamp`,
  `message`, `isMeta`, `isSidechain`…). The same mapping is used for stream-json
  `assistant`/`user` events, whose `message` has the same shape, if chat mode ever needs
  it (R7-b).
  - Unknown `type`, unknown block types and unknown fields are skipped.
  - A line that does not parse is skipped and counted in a debug log.
  - Sidechain lines (sub-agents) are skipped; the `Task`/`Agent` tool card shows the
    prompt and the final result.
  - Entry ids are stable strings, `t:<uuid>:<block index>`, so the UI can key and dedupe
    them.
- **Tool summaries** are computed in Rust, so the list stays small:
  - Bash: command and description; exit status from `is_error`.
  - Edit, MultiEdit and Write: path and +/- counts, plus a unified diff built with the
    `similar` crate (new dependency in `delilah-agents`) from `old_string`/`new_string`
    or the written content.
  - Read: path and range.
  - Grep and Glob: pattern, path, match count.
  - TodoWrite: the items.
  - Task/Agent: description and prompt.
  - AskUserQuestion: matched to its card (R5).
  - Anything else: name and raw input.

  Results longer than 4 KB are truncated in the list (`truncated: true`), and the full
  text is fetched per tool (`GET …/conversation/tools/{tool_use_id}`).
- **Rationale**: one mapping means one set of fixture tests, and it gives FR-016 (the
  same entries in both runtimes) by construction. Truncating keeps a 3,000-entry session
  light (SC-003). `similar` is the standard diff crate, and doing the diff in Rust keeps
  the UI free of a diff library: rows reuse ChangesView's line format.
- **Alternatives**: computing diffs in the UI with `jsdiff`. Rejected: one more npm
  dependency, and the UI already renders diff rows from the daemon.

## R4. A view setting per task, and the composer for every Claude session

- **Decision**:
  - New column `tasks.view` (`'chat'` or `'terminal'`, NULL meaning chat). It is set in
    the new-task dialog and from the task screen (`PATCH /tasks/{key}`), and TaskScreen
    uses it as the starting view for each Claude session. The existing in-memory
    `terminals` set becomes the per-session override, so the one-click switch keeps
    working. Project sessions open in chat.
  - The conversation routes (`messages`, `stop`, removing a queued message) stop
    requiring `conversation = 1`, and work for any live Claude session. `flush_queue` at
    turn end runs for every session that has queued messages.
- **Rationale**: the user asked for a per-task choice (clarification). The composer
  already exists. Widening its guard is the cheapest route to US2.
- **Alternatives**: a global preference. Rejected by the user (per task).

## R5. Question cards inline, without duplicates

- **Decision**: the ask hook (PreToolUse for AskUserQuestion) carries `tool_use_id`. It
  is stored in the card's body (JSON; no migration). When the transcript yields an
  AskUserQuestion `tool_use` whose id matches a card, the card is shown in that place
  and the tool entry is dropped. Cards with no match (older ones) stay where their time
  puts them.
  - For idea conversations, stored `user` entries that are not queued, and stored
    `claude` entries, are hidden **once the session has a transcript**, because the
    transcript has the same messages. Without a transcript they are shown, as today
    (FR-011).
- **Rationale**: keeps FR-010 (inline cards in sync with the Bandeja; they are the same
  `asks.rs` objects), and keeps today's idea conversation history readable.

## R6. Runtime abstraction

- **Decision**: `delilah-agents::runtime::Runtime`, an enum
  `{ Pty(Arc<PtySession>), Stream(Arc<StreamSession>) }`, with:
  - `send_prompt(text)`: paste plus Enter, or a user JSON line;
  - `stop()`: ESC, or an interrupt control request;
  - `kill()`, `wait()`, `has_exited()`;
  - `as_pty() -> Option<&Arc<PtySession>>` for the terminal, attach and the trust
    dialog.

  `sessions::Live.pty` becomes `Live.runtime`. The ~70 daemon uses are almost all
  `kill`/`wait`/`has_exited`, plus `type_prompt` (actions.rs), the ESC in
  `conversation::stop`, `terminal::pump` and `run::accept_trust_dialog`.

  The refactor lands **on its own first, with no behaviour change** (PTY only), before
  `StreamSession` exists.
- **Rationale**: an enum, not a trait object. There are exactly two runtimes, and the
  PTY-only operations (resize, attach, scrollback) need a clear "not in chat mode"
  answer, which `as_pty()` gives. Landing the refactor alone keeps the riskiest diff
  reviewable and testable with today's tests.
- **Alternatives**: `dyn AgentRuntime`. Rejected: optional methods everywhere, and no
  third runtime in sight (other agents are out of scope).

## R7. What the spike must answer (US5, before Phase 6)

Run by `crates/delilah-agents/examples/claude_stream_spike.rs` with the shape of
Delilah's settings file (hooks posting to a local listener the example starts),
in a fixture workspace, with a clean env. Findings go to
`docs/spikes/2026-10-2x-claude-stream.md`.

| # | Question | If yes | If no |
|---|---|---|---|
| a | Hooks from `--settings` fire under `-p --input-format stream-json --output-format stream-json --verbose` (`UserPromptSubmit`, `PreToolUse`, `PostToolUse`, `Stop`, `Notification`) | State, phases and inbox are unchanged (FR-017) | Chat mode needs states from stream events: big cost, so US3/US4 are dropped and recorded |
| b | One process takes several turns from stdin, writes the transcript, and its `assistant`/`user` events match transcript messages | Entries come from the transcript in both modes (R1–R3) | Multi-turn no: a process per turn with `--resume`. Transcript no: entries from stream events through the same R3 mapping |
| c | `--resume` works PTY → stream → PTY on the same session id | US4 as specified | US4 dropped and recorded; chat mode stays without an escape hatch, which is a reason to drop US3 too |
| d1 | Folder trust: print mode does not wait on a dialog in a new workspace | Nothing to do | Run the PTY trust acceptance once before the first stream launch in that workspace |
| d2 | Permissions: with `--permission-prompts host` (the default), a tool not allowed arrives as a `control_request` `can_use_tool` on stdout (with `permission_suggestions`), and a `control_response` on stdin allows or denies it | Option C through the control protocol on stdin, no MCP server (R8) | Try `--permission-prompt-tool` with a stdio MCP server (`delilah mcp-permission`, which calls the daemon); if that fails too, the fallback: `--permission-prompts none` plus the task's allowed tools in the settings, with refusals shown in the chat |
| e | Interrupt: a `control_request` `interrupt` on stdin stops the turn, the process stays up, and the next user line starts a turn. Which hooks fire (`Stop`?) | "Detener" = interrupt | "Detener" = kill and relaunch with `--resume` (slower, same result) |
| d3 | AskUserQuestion: the blocking ask hook (PreToolUse) still waits for Delilah's answer in print mode | Question cards unchanged in chat mode (FR-018) | Answer questions through the `can_use_tool` control request for AskUserQuestion (the SDK way), with the same card |
| f | The status line command runs in print mode (rate limits for the usage-limit pause) | Limits unchanged | Read `rate_limit` info from stream events if present; else limit detection relies on `StopFailure` (rule 2 of the PTY spike), recorded as a known gap |
| g | `--include-partial-messages` deltas (`stream_event` `content_block_delta` `text_delta`) arrive with a message id to attach them to | Live text (FR-016) | Live text shows per complete message only, recorded |
| h | `--remote-control` with `-p` | Kept | Hidden in chat mode, said in the UI |

> **Spike result (2026-10-24, claude 2.1.292, [spike](../../docs/spikes/2026-10-24-claude-stream.md))**:
> a, b, c, d1, e, d3, g yes; f no (status line never runs in `-p`; use `rate_limit_event`); h
> accepted but unverified (hidden in chat mode). d2 is yes, but the default is not
> `--permission-prompts host`: the flag is **`--permission-prompt-tool stdio`**, and
> without it Claude denies by itself. The same flag adds AskUserQuestion to the `-p` tools
> (d3). No `Stop` fires after an interrupt (e). Verdict: adjust, decision 67.

## R8. Chat-mode permissions (option C), assuming R7-d2 is "yes" (it is, with `--permission-prompt-tool stdio`)

- **Decision**:
  - `StreamSession` turns each `can_use_tool` request into a typed
    `StreamEvent::PermissionRequest`, and the wire format stays inside `stream.rs`. The
    daemon's `permissions.rs` keeps these as **permission asks** in their own registry
    (there is no blocking hook, unlike `asks.rs`) and answers through
    `reply_permission`. Each one shows as a "¿Permitir?" card in the conversation and as
    a Bandeja item; the Inbox lists both registries.
  - The answers are:
    - allow once: `behavior: allow` with the original input;
    - allow always in this task: the same, plus the request's
      `permission_suggestions` rule (or the tool name when none is given) appended to
      the new column `tasks.allowed_tools` (JSON array). That list goes into the
      `permissions.allow` of every later session settings file for the task, and the
      daemon answers matching requests by itself;
    - deny: `behavior: deny` with the user's reason, or a default message.
  - An autopilot task pauses into the Bandeja on a permission ask (spec edge case).
- **Rationale**: no MCP server, no extra process, and the same Inbox surface as question
  cards. Suggestions give Claude's own rule shape (e.g. `Bash(npm install:*)`),
  so "always" for shell commands covers that command, not all of Bash.
- **Alternatives**:
  - `--permission-prompt-tool` with an MCP server: the fallback if d2 fails.
  - Allow list only: the last fallback.

## R9. Live text in chat mode: a per-session WebSocket, not a topic

- **Decision**: `GET /api/v1/sessions/{id}/live` (WebSocket), modelled on
  `terminal.rs`. It sends `{message_id, text}` deltas, then `{message_id, done: true}`.
  Nothing is stored. The UI shows a "writing" bubble that is replaced when the
  transcript entry for that message arrives.
- **Rationale**: topics (#11) are payload-free "refetch" signals. Deltas need a payload
  and are per session, like the terminal stream.

## R10. Switching runtime between turns, and restarts

- **Decision**:
  - `POST /api/v1/tasks/{key}/runtime {"mode": "chat"|"terminal"}`. It is refused with
    409 while any live session of the task is working or has a pending ask. Otherwise it
    sets `tasks.chat_mode`, kills the live Claude sessions and relaunches them with
    `--resume` in the new runtime.
  - `launch_session` picks the runtime from the task's flag (task sessions only; project
    sessions and idea conversations are always PTY). Restarts, usage-limit resumes and
    saving-mode handoffs all go through `launch_session`, so they follow the flag
    (FR-022).
  - If a chat-mode spawn fails at once (unknown option, or the process exits before
    `system init`), the start fails with a message, and the UI offers "Iniciar en
    terminal" (FR-023).
- **Rationale**: every launch already funnels through `launch_session`, so one branch
  covers every path.
