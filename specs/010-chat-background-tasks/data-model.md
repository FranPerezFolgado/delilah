# Data model: Show background work in chat mode

All of it is in memory, per live session. No table, no migration (FR-016).

## `StreamEvent` additions (`delilah-agents::stream`)

| Variant | Fields | From |
|---|---|---|
| `Init` (changed) | `session_id`, `self_started: bool` | `system/init`; `self_started` when no user line was written since the last `result` (R4) |
| `ToolStarted` | `tool_use_id`, `tool`, `input: Value` | main-thread `assistant` line with a `tool_use` block (R1) |
| `SubagentStep` | `parent: String` (the outermost `Agent` call's `tool_use_id`), `tool`, `input: Value` | `assistant` line with `parent_tool_use_id` and a `tool_use` block (R2) |
| `Task(TaskEvent)` | see below | `system/task_*`, `system/background_tasks_changed` (R3) |

`TaskEvent`:

- `Started { task_id, tool_use_id: Option, kind: TaskKind, description, background: bool }`
- `Progress { task_id, tool_use_id: Option, description: Option, last_tool: Option, tool_uses: Option<u32> }`
- `Ended { task_id, status: TaskStatus, summary: Option }` (from `task_updated` with a final status, or `task_notification`)
- `Running { task_ids: Vec<String>, described: Vec<(task_id, kind, description)> }` (the `background_tasks_changed` snapshot)

`TaskKind`: `Command` (`local_bash`), `Agent` (`local_agent`), `Other(String)`.
`TaskStatus`: `Running`, `Completed`, `Failed`, `Stopped`, `Unknown` (any other string).

Lines with a `parent_tool_use_id` never touch the main message assembly (`current`,
`pending`, `stopped`) and never produce `TextDelta` / `MessageDone`.

## `Progress` (`delilah-daemon::sessions`, serialised to the API)

```text
Progress {
  turn: Option<Turn>,                 // None between turns
  background: Vec<BackgroundItem>,    // strip rows, in start order
  subagents: Vec<Subagent>,           // running and recently finished, by tool_use_id
}
Turn {
  started_at: DateTime,               // Init time (or first sign of the turn)
  by: "user" | "claude",              // claude = self-started (R4)
  reason: Option<String>,             // "tarea en segundo plano terminada" when self-started after a task end
  step: Option<String>,               // current step, one line (FR-002)
  last_signal_at: DateTime,           // any stream line of this session (FR-003)
}
BackgroundItem {
  task_id, tool_use_id: Option, kind: "command" | "agent" | "other",
  description, state: "running" | "completed" | "failed" | "stopped" | "unknown",
  started_at: Option<DateTime>,       // None when first seen after a restart
  ended_at: Option<DateTime>,
}
Subagent {
  tool_use_id, description, state: "running" | "completed" | "failed" | "stopped",
  started_at, ended_at: Option, steps: u32,
  recent: Vec<Step>,                  // last 50 (FR-006); steps - recent.len() = hidden
}
Step { tool, summary, at }
```

### Rules

- **Step text** (`step`, `Step.summary`) reuses the existing one-line summaries: the
  `activity_of` wording ("Leyendo x", "Buscando p", the command's first 40 chars) moved to
  one shared function; for a sub-agent step the step is `Subagente: <description> · <its
  latest step>`; `task_progress.description` wins over our own summary when present.
- **Turn**: set on `Init`; `step` replaced on `ToolStarted` and `SubagentStep`;
  `last_signal_at` bumped on every parsed line; cleared on `TurnResult` and `Exited`.
- **Background**: `Running` snapshot is authoritative for which ids are running: ids not in
  it become `completed` unless an `Ended` already set another final state; ids in it that are
  unknown are added (with `started_at` None until a `Started` arrives). Final rows are removed
  when the next turn starts (`Init`) or 60 s after `ended_at` (FR-010).
- **Subagents**: created on `Task(Started{kind: Agent})` (or the first `SubagentStep` for an
  unknown parent), `steps` from `Progress.tool_uses` when present else counted, finished on
  `Ended`. Finished ones are dropped when the turn after their end ends (the transcript's card
  has the result by then).
- **Exit**: every `running` background item and sub-agent becomes `stopped`; `turn` cleared.
- **has_background()**: any item with state `running`. Used by the turn-end deferral (R6),
  the runtime switch refusal (FR-013) and the message route (a message is typed now, not
  queued, when only background work runs: FR, US3 #4).

## Deferred turn end (`Live`)

`held_stop: Option<Value>`, the `Stop` hook payload held back (R6), kept under the **same**
`Arc<Mutex<…>>` as the background set so `hold_stop_if_background` is atomic, plus a
cancellable 30 s timer (`Arc` token) started when the set empties with a payload held.
Any `Init`, `PromptSubmitted` or `Working`, an interrupt or error result, and the replay
itself clear the payload and cancel the timer. The progress `watch::Sender` is an
`Arc` field of `Live` too. `deferred_turn_end` is internal (`Live::has_held_stop()`, for
tests), not part of the API.

## Transcript entry addition (`delilah-agents::transcript`)

`MarkerKind::BackgroundDone` with an optional `detail: String` on `Entry::Marker` (the
`<summary>` of the notification). Serialised as `"marker": "background-done", "detail": "…"`.

## Task history

New event kind `agent-resumed` (task history, like `permission-asked`), written when a
self-started turn begins on the primary session; its text: "Claude continúa: tarea en
segundo plano terminada" (or "Claude continúa" with no known reason). No schema change:
history kinds are free text.
