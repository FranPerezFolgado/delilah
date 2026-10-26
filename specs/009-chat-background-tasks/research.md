# Research: Show background work in chat mode

Claude Code 2.1.292, personal Mac, 2026-10-26. Capture: `claude -p --input-format
stream-json --output-format stream-json --verbose --include-partial-messages
--permission-mode bypassPermissions` in a throwaway git folder, clean env, stdin held open
150 s. Prompt: (1) a background Bash `sleep 25`, (2) a background sub-agent that globs and
reads files, (3) a foreground sub-agent that reads `a.txt`; then end the turn and say
ALL-DONE when the background work ends. 177 stdout lines, 3 `result`s in one process.

## R1. What the stream says about tool starts

**Decision**: a tool start is the `assistant` line carrying a `tool_use` block (it has
`name`, `id` and the full `input`), emitted by the parser as `ToolStarted` as soon as the
line arrives, before the tool runs.

**Evidence**: `stream_event` `content_block_start` with `content_block.type == "tool_use"`
gives `name` and `id` but the input only as `input_json_delta` pieces; the complete
`assistant` line for that block follows at `content_block_stop`, before any `tool_result`.

**Alternatives**: assembling `input_json_delta` pieces (more code, a few ms earlier, no
gain); the `PreToolUse` hook (works for PTY too, but arrives over HTTP out of order with the
stream; used only for PTY sessions, see R7).

## R2. Sub-agent messages in the stream

**Decision**: lines with a non-null `parent_tool_use_id` belong to the sub-agent started by
that `Agent` tool call. The parser must route them away from the main message assembly and
emit `SubagentStep { parent, tool, input }` for each `tool_use` block in them. Nested
sub-agents are folded into the outermost known `Agent` call (a map child → root).

**Evidence**: `assistant` and `user` lines with `parent_tool_use_id: "toolu_01MF…"` /
`"toolu_01SA…"`; no `stream_event` carried a parent (sub-agents send whole messages, not
deltas). These lines arrive **interleaved** with the main thread's `stream_event` deltas
(e.g. a sub-agent `assistant` between a main `content_block_start` and its deltas).

**Bug found**: today's `Parser` treats a sub-agent `assistant` line as a new main message:
it flushes the main thread's pending message early and emits `MessageDone` for the
sub-agent's message id. The UI then has a "done" for a message it never showed and may
replace the writing bubble too early. Fixed by R2's routing (the main `pending` and
`current` are untouched by lines with a parent).

## R3. Background and sub-agent task events

**Decision**: parse these `system` subtypes into `StreamEvent::Task(TaskEvent)`:

| subtype | fields used | meaning |
|---|---|---|
| `task_started` | `task_id`, `tool_use_id`, `description`, `task_type` (`local_bash`, `local_agent`), `is_backgrounded`, `subagent_type`, `spawn_depth` | a background command or a sub-agent (foreground too) started |
| `task_progress` | `task_id`, `tool_use_id`, `description` (a present-tense step, e.g. "Reading a.txt"), `last_tool_name`, `usage.tool_uses`, `usage.duration_ms` | a sub-agent's progress |
| `task_updated` | `task_id`, `patch.status` (`completed`…), `patch.end_time` (epoch ms) | state change |
| `task_notification` | `task_id`, `tool_use_id`, `status`, `summary`, `usage` | the task ended; Claude is told |
| `background_tasks_changed` | `tasks[]` (`task_id`, `task_type`, `description`) | the full set of running background tasks (empty when none) |

`background_tasks_changed` is the source of truth for "is background work running"
(a snapshot, so a missed event cannot leave a ghost item). `task_started` with
`is_backgrounded: true` adds the start time and `tool_use_id`; `task_updated` /
`task_notification` give the end state. Unknown `status` values map to `desconocido`.

**Evidence**: captured lines, abridged:

```json
{"type":"system","subtype":"background_tasks_changed","tasks":[{"task_id":"bnksojypv","run_id":"…","task_type":"local_bash","description":"Background sleep for 25 seconds"}]}
{"type":"system","subtype":"task_started","task_id":"bnksojypv","tool_use_id":"toolu_01Rb…","description":"Background sleep for 25 seconds","is_backgrounded":true,"task_type":"local_bash"}
{"type":"system","subtype":"task_started","task_id":"aaa7691b5dbb7c001","tool_use_id":"toolu_01SA…","description":"Read a.txt file content","subagent_type":"general-purpose","is_backgrounded":false,"spawn_depth":1,"task_type":"local_agent","prompt":"…"}
{"type":"system","subtype":"task_progress","task_id":"ae5e1d1a7c4e8a88d","tool_use_id":"toolu_01MF…","description":"Reading b.rs","usage":{"total_tokens":22145,"tool_uses":3,"duration_ms":8309},"last_tool_name":"Read"}
{"type":"system","subtype":"task_updated","task_id":"bnksojypv","patch":{"status":"completed","end_time":1793018439327}}
{"type":"system","subtype":"task_notification","task_id":"bnksojypv","tool_use_id":"toolu_01Rb…","status":"completed","summary":"Background command \"Background sleep for 25 seconds\" completed (exit code 0)"}
{"type":"system","subtype":"background_tasks_changed","tasks":[]}
```

**Alternatives**: inferring background work from `Bash` inputs with `run_in_background`
(misses background agents and the end); polling Claude's task output files (racy, private).

## R4. Turns Claude starts by itself

**Decision**: a `system/init` that arrives when Delilah has not written a user line since
the last `result` is a **self-started turn**. `StreamSession` keeps an `awaiting_turn` flag
set when it writes a user line and cleared on `Init`; `Init` with the flag clear is emitted
as `Init { self_started: true }`. The daemon then applies the same effects as a typed
prompt (session working, `UserAnswered`, inbox cleared) and records history
`agent-resumed`.

**Evidence**: 3 `result`s, but only one user line written; turns 2 and 3 each began with
`system/init` right after a `task_notification`. The transcript has, for each, a `user`
line with `origin: {"kind":"task-notification", …}` and text starting `<task-notification>`.
`stop_hook_summary` appears 3 times: **the `Stop` hook fires at the end of every turn,
including the one that ended with background work pending.**

**Open (checked in T001)**: whether `UserPromptSubmit` fires for a self-started turn (the
capture had no hook settings). The design does not depend on it; if it fires, the effects
are idempotent (state already `working`, `UserAnswered` from `running` is a no-op).

**Alternatives**: waiting for the next hook (the UI stays idle until a tool ends); treating
`task_notification` as the turn start (it does not always start one: the foreground
sub-agent's notification in turn 1 did not).

## R5. The `<task-notification>` line in the transcript

**Decision**: `transcript::line_items` turns a `user` line whose `origin.kind` is
`task-notification` (or whose text starts with `<task-notification>`) into a new
`Entry::Marker { marker: BackgroundDone, detail }`, `detail` being the `<summary>` text (first
line, ≤ 200 chars). Today it is shown as a user bubble with raw XML.

**Alternatives**: hiding it (loses why Claude went on); keeping the bubble (looks like the
user typed it).

## R6. Turn end with background work pending

**Decision**: when the `Stop` hook (`TurnEnded`) of a chat-mode session arrives while its
background set is non-empty, the daemon **defers** it: session stays `working`, no inbox
change, no `AgentAsked`, no `note_turn_done` (review hint, saving mode, autopilot), no phase
signal. The hook payload is kept on the live session. It is applied as is when:

- the set becomes empty and no self-started turn begins within 30 s (FR-012), or
- the process exits (then the normal exit handling runs after it).

A self-started turn discards the deferred payload (its own `Stop` will come).
`StopFailure` (`TurnFailed`) and usage limits are never deferred.

**Ordering**: `background_tasks_changed` arrives at task start, well before the turn's
`result` and `Stop`, so the set is known when `Stop` arrives. A `Stop` that arrives before
any task event of a background item started in that very turn is not possible in the
capture (task start precedes the tool result).

**Alternatives**: a new task state "background" (needs a migration and touches every
state consumer; out of proportion); suppressing in the UI only (board, Bandeja, autopilot
would still be wrong).

## R7. Where progress lives and how it reaches the UI

**Decision**: a `Progress` value per live session (`sessions.rs`), held in a
`tokio::sync::watch` channel: writers replace it, the live socket relays the latest value at
most every 250 ms (`watch` coalesces, so no backlog and no lag resets). `GET
…/conversation` includes the current `progress` so a reload mid-turn is right (FR-004).
Not persisted (FR-016).

For **PTY sessions** (free part of US1), `PreToolUse` sets the existing `activity` at tool
start, as `PostToolUse` does today; nothing else changes for them.

**Alternatives**: new messages on the existing `broadcast` of stream events (each event
would be a socket message: no throttle, and lag resets would also drop live text); polling
the conversation route (slow, heavy).

## R8. "Detener" and background items

**Decision**: "Detener" keeps today's interrupt. Whether the stream accepts a request to
stop one background task (`control_request` `{"subtype":"stop_task","task_id":…}`, as the
Agent SDK's `stopTask`) is checked in T001; if it works, "Detener" sends it for each running
item; if not, items are left to Claude, and the strip marks them `detenido` when the
process exits (FR-011 fallback). Either way the deferred turn end of R6 applies when the set
empties.

## R9. Verification capture (T001)

Re-run the capture through `crates/delilah-agents/examples/claude_stream_spike.rs` with a
new `background` scenario (hook settings shaped like `claude::settings`, stdio permission
flag) to confirm, and save the stdout and `hooks.jsonl` as fixtures under
`crates/delilah-agents/tests/fixtures/stream-background*.jsonl`:

1. the R3 shapes with the real flags Delilah uses;
2. `Stop` fires for a turn ending with background work (R4);
3. whether `UserPromptSubmit` fires for a self-started turn (R4 open point);
4. whether `stop_task` is accepted (R8).

Results go into this file and `docs/spikes/2026-10-24-claude-stream.md` (a new section).

### R9 Results (T001/T002, Claude Code 2.1.292, 2026-10-26)

Two runs of `claude_stream_spike background` and `background-stop` (the flags of
`StreamSession`: `-p --input-format stream-json --output-format stream-json --verbose
--include-partial-messages --permission-prompt-tool stdio`, `--settings` with every hook
posting to a local listener, fixture workspace from `make-fixture-parent.sh`, stdin open
150 s). Fixtures: `crates/delilah-agents/tests/fixtures/stream-background.jsonl` (216
lines, 3 `result`s), `stream-background-stop.jsonl` (203 lines, 2 `result`s),
`stream-background-hooks.jsonl` (26 hook bodies of the first run). Home and scratch paths
are rewritten to `/fixture/...`.

1. **R3 shapes hold with the real flags.** `task_started` (`task_type` `local_bash` /
   `local_agent`, `is_backgrounded`, `tool_use_id`, `run_id`, `subagent_type`,
   `spawn_depth`), `task_progress` (`description`, `last_tool_name`, `usage`),
   `task_updated` (`patch.status`, `patch.end_time`), `task_notification` (`status`,
   `summary`, `output_file`, `usage` for agents only) and `background_tasks_changed` (full
   set; `[]` at the end) all arrive as in R3. Each has an extra `run_id`. A foreground
   sub-agent also gets `task_started` (`is_backgrounded: false`) and `task_notification`
   before the turn ends; it never appears in `background_tasks_changed`.
   **Sub-agent lines are tagged with `parent_tool_use_id`** = the `tool_use_id` of the
   `Agent` call, on `assistant`, `user` (tool results) and `stream_event` lines; main-thread
   lines have `null`. Sub-agent tool permissions arrive as `control_request`
   `can_use_tool` on the same stdio channel.
2. **`Stop` fires for the turn that ends with background work pending: yes.** Run 1 had 3
   `result`s and 3 `Stop`s. New and useful: the `Stop` payload has `background_tasks`
   (`[{id, type: "shell", command, description, status: "running"}]`; `[]` on the last),
   `last_assistant_message`, `prompt_id`, `stop_hook_active`, `session_crons`, `effort`.
   The deferral of R6 can read it directly.
3. **`UserPromptSubmit` fires for a self-started turn: yes.** Its `prompt` is the
   `<task-notification>` XML (`task-id`, `tool-use-id`, `output-file`, `status`,
   `summary`). It arrives right after the `Stop` of the previous turn and about 100 ms
   before the `system/init` of the new turn. Idempotent effects, as R4 planned. Also:
   a notification that arrives **during** a running turn (run 2: the stopped `sleep`)
   fires `UserPromptSubmit` too but starts no new turn and has no `init`, so a
   `UserPromptSubmit` is not by itself a turn start; keep deciding from `init`.
4. **`stop_task` is accepted.** Sent
   `{"type":"control_request","request_id":"s1","request":{"subtype":"stop_task","task_id":"b3796hsz2"}}`
   for the background `sleep` 1 ms after its start; answer
   `{"type":"control_response","response":{"subtype":"success","request_id":"s1","response":{}}}`.
   In the same millisecond: `task_updated` `patch.status: "killed"`, `task_notification`
   with `status: "stopped"` (no `usage`; `summary` is just the description) and
   `background_tasks_changed` with `tasks: []`. Claude kept going: the other background
   agent ran on and the turn's later text said the sleep "was stopped". So R8's first
   branch applies (T047/T048 stay): "Detener" can send `stop_task` per running item. Task
   `status` values seen: `completed`, `stopped`; `killed` in `patch.status`: map both to
   `detenido`.

Other findings. A stop-task notification does not start a turn by itself when the turn is
still running; with the stopped task gone, run 2 had only one self-started turn (the
background agent's), and its `Stop` carried `background_tasks: []`. Self-started turns
begin 0.1-0.3 s after the previous `result`. With `--permission-prompt-tool stdio` and no
allow rules, the `find`/`ls` of a sub-agent asked via `can_use_tool` (the spike allows);
Read did not. The user's PreToolUse hooks (rtk) may rewrite Bash input, visible in
`updatedInput` echoes.
