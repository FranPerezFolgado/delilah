# Data model: Playground redesign

All state is in memory (no migration). Daemon types in Rust, mirrored in `app/src/api.ts`.

## Daemon

### `ActivityEvent` (delilah-agents `claude.rs`, changed)

`ToolEnded` gains `failed: bool` (true for `PostToolUseFailure`). Everything else as in
spec 004. Serialised for the history as:

| `type` | fields |
|---|---|
| `tool_started` | `agent: AgentRef \| null`, `action: Action` (id = tool use id, kind, tool, target ≤ 40 chars, background, since) |
| `tool_ended` | `agent: AgentRef \| null`, `id` (tool use id), `failed` |
| `helper_started` | `agent: AgentRef` (id, role) |
| `helper_ended` | `id` |
| `turn_over` | — |

### `Entry` (new, `activity_log.rs`)

| field | type | notes |
|---|---|---|
| `seq` | u64 | daemon-wide, strictly increasing; the `after` cursor |
| `at` | timestamp | when the hook arrived |
| `event` | `ActivityEvent` | served as `EntryView` (daemon DTO, snake_case `type` + fields above) |

### `SessionLog` (new)

| field | type | notes |
|---|---|---|
| `session` | string | session id |
| `owner` | string | task key or `@project` (from `Live.task`) |
| `entries` | `VecDeque<Entry>` | ≤ 200; pushing the 201st drops the oldest and sets `dropped` |
| `dropped` | bool | older entries were dropped |
| `last_at` | timestamp | time of the last entry; eviction order |

### `ActivityLog` (new, in `AppState`)

- `sessions: HashMap<String, SessionLog>`, at most 64.
- `record(session, owner, at, event, live_ids: &HashSet<String>)`: appends; creating a
  65th log evicts the non-live log with the oldest `last_at` (live ids are a snapshot
  taken before the lock); with 64 live logs it grows past 64.
- `for_owner(owner, after) -> Vec<SessionLogView>`: logs of that owner, entries with
  `seq > after`, oldest first.
- `started_at`: daemon start time (so the UI can say "nothing since <time>").

### `TokenUsage` (new, `tokens.rs`)

| field | type | notes |
|---|---|---|
| `total` | u64 | Σ input + cache_creation + output (FR-025/026 tiers use this) |
| `input`, `output`, `cache_write`, `cache_read` | u64 | breakdown; `cache_read` not in `total` |

`TokenMeter` (per live session, inside `TranscriptProgress { seen, meter }` in the watcher state): `offset` (bytes consumed, always
at a line end), `usage: TokenUsage`, `last_message_id: Option<String>`. Dropped with the
session (as `transcript_seen` today).

Parsing rule (`delilah-agents::transcript::usage_delta(bytes) -> (TokenUsage, consumed,
last_message_id)`): only complete lines; only `type == "assistant"` with
`message.usage`; a line whose `message.id` equals the previous counted id is skipped;
missing numbers count as 0; bad JSON lines are skipped.

## App

- `ActivityResponse = { sessions: Record<id, Activity>, tokens: Record<id, TokenUsage> }`.
- `HistoryResponse = { started_at, sessions: { session, live, dropped, entries: Entry[] }[] }`.
- `FlowNode = { id, type: "agent" | "helper" | "tool", label, kind?, state: "running" |
  "done" | "failed" | "unknown", start, end?, parent }` and `FlowEdge = { from, to }`,
  derived by `flowModel.ts`; `TimelineRow = { seq, at, node, text }`.
- `FloorModel = { owner, title, phase, project, leads: DeskSeat[], helpers: DeskSeat[],
  leaving: Leaver[], waiting: number }`, `DeskSeat = { slot, session | helperId, state,
  action, tokens?, look }` — slots stable per owner.

### Desk state (office) — from spec 004, `zzz` now only Paused

| session state / activity | pose | over the desk | monitor |
|---|---|---|---|
| Starting | walking in → seated | — | idle glass |
| Working, action of kind K | typing | caption | glow K |
| Working, no action | seated | — | idle glass |
| Waiting | seated, hand up | `?` | idle glass |
| Paused | seated_sleeping | `zzz` | off |
| helper ended | standing → walking → lounge (≈5 s) → gone | — | idle glass |
