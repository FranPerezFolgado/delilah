# Implementation Plan: Show background work in chat mode

**Branch**: `009-chat-background-tasks` | **Date**: 2026-10-26 | **Spec**: [spec.md](spec.md)

**Input**: Feature specification from `specs/009-chat-background-tasks/spec.md`

## Summary

Chat mode already reads Claude's stream-json, but the parser keeps only `init`, text
deltas, whole messages, results, rate limits and permission requests. A capture with
Claude 2.1.292 ([research.md](research.md)) shows that the stream also carries everything
needed: tool starts, sub-agent messages tagged with `parent_tool_use_id`, and
`system/task_started|task_progress|task_updated|task_notification|background_tasks_changed`.
Turns that Claude starts by itself begin with a bare `system/init`, and the `Stop` hook
fires even when the turn ends with background work still running.

The plan:

1. **Parser** (`delilah-agents::stream`): route sub-agent lines away from the main message
   assembly (fixes an interleaving bug), and emit `ToolStarted`, `SubagentStep`,
   `Task(TaskEvent)` and `Init { self_started }`.
2. **Progress** (`delilah-daemon`): a per-session in-memory `Progress` (turn clock, step,
   last sign of life, sub-agents, background items) kept in a `watch` channel. It is
   updated by the stream pump, exposed in `GET …/conversation` and relayed over the live
   socket (throttled to 4/s).
3. **State**: hold back a `Stop` while background work runs (no "Te espera", inbox,
   autopilot or saving-mode step), with a 30 s fallback. Treat a self-started turn like a
   typed prompt. Refuse the runtime switch while background work runs.
4. **Transcript**: `<task-notification>` user lines become a `background-done` marker
   instead of a raw user bubble.
5. **UI**: a working indicator (clock, step, "sin novedades"), live progress in the Agent
   tool card, and an "En segundo plano" strip.
6. **PTY**: free part only. `PreToolUse` sets the activity at tool start.

## Technical Context

**Language/Version**: Rust 2024 (workspace `crates/*`), TypeScript 5 + React 18 (`app/`)

**Primary Dependencies**: tokio (`sync::watch`, timers), axum (WebSocket), serde_json; React, Vitest

**Storage**: none new: in-memory per live session (FR-016); SQLite untouched, no migration

**Testing**: `cargo test -p delilah-agents`, `cargo test -p delilah-daemon`, Vitest; parser tests against the captured fixture

**Target Platform**: macOS desktop (daemon + Tauri app); UI checked in a preview

**Project Type**: daemon + CLI + desktop web UI

**Performance Goals**: change visible ≤ 2 s (SC-002); ≤ 4 progress messages/s per session (FR-018)

**Constraints**: additive API/socket changes only; unknown stream lines ignored (FR-017); Spanish UI strings through `t()`

**Scale/Scope**: one stream per chat-mode session; ≤ 50 recent steps per sub-agent; a handful of background items

## Constitution Check

`.specify/memory/constitution.md` is the unfilled template. The project's rules in
`CLAUDE.md` and `docs/development.md` serve as gates:

| Gate | Status |
|---|---|
| Never edit a shipped migration; schema changes as new migrations | ✅ no schema change |
| API ↔ `app/src/api.ts` contract kept in step | ✅ [contracts/api.md](contracts/api.md), additive |
| Try changes only in a preview, never the installed daemon (7420) | ✅ [quickstart.md](quickstart.md) |
| Agents spawned with a clean env; hooks in settings outside the repo | ✅ unchanged; T001 capture uses the spike example (clean env, `--settings`) |
| No work-repo content; fixtures only | ✅ fixtures captured from a throwaway folder |
| Docs updated when behaviour changes (status, architecture, README, decisions) | ✅ Phase 7 tasks |
| No version bump on the feature branch | ✅ |

Post-design re-check: no violations.

## Project Structure

### Documentation (this feature)

```text
specs/009-chat-background-tasks/
├── spec.md
├── plan.md              # this file
├── research.md          # capture results R1–R9
├── data-model.md        # StreamEvent additions, Progress, deferred turn end
├── quickstart.md
├── contracts/api.md     # conversation `progress`, socket `{progress}`, marker, 409
├── checklists/requirements.md
└── tasks.md             # /speckit-tasks
```

### Source Code (repository root)

```text
crates/delilah-agents/
├── src/stream.rs            # Parser routing by parent_tool_use_id; ToolStarted, SubagentStep,
│                            #   Task(TaskEvent), Init{self_started}; StreamSession awaiting_turn,
│                            #   stop_task(task_id) (if R8 holds)
├── src/transcript.rs        # MarkerKind::BackgroundDone + detail; task-notification lines;
│                            #   shared one-line tool summary (moved from conversation::activity_of)
├── examples/claude_stream_spike.rs   # new `background` scenario (T001)
└── tests/fixtures/stream-background*.jsonl

crates/delilah-daemon/src/
├── progress.rs   (new)      # Progress model + pure reducer `apply(&mut Progress, &StreamEvent, now)`
├── sessions.rs              # Live: progress watch channel, deferred_stop + timer, has_background()
├── stream_events.rs         # pump feeds the reducer; self-started Init → resume effects
├── hooks.rs                 # TurnEnded deferral inside the existing apply_signal (pub(crate));
│                            #   synthetic-prompt handling in note_activity; PreToolUse activity for PTY
├── phases.rs                # ignore the synthetic (agent-origin) prompt
├── conversation.rs          # `progress` in the response; flush_queue on deferral; reset_turn_state
│                            #   for agent-origin prompts; message route: typed when only
│                            #   background runs; stop → stop_task; activity_of → shared fn
├── live.rs                  # relay `{progress}` from the watch channel, ≤ 4/s, first on open
├── runtime_api.rs           # 409 while background runs
└── run.rs                   # exit: progress items → stopped, replay a held Stop before exit handling

app/src/
├── api.ts                   # Progress types; Conversation.progress; marker "background-done"
├── live.ts                  # `progress` message to subscribers
└── components/
    ├── ConversationView.tsx # WorkingIndicator + BackgroundStrip placement, background-done divider
    ├── WorkingIndicator.tsx (new)  # clock, step, "sin novedades desde hace…"
    ├── BackgroundStrip.tsx  (new)  # "En segundo plano" rows
    └── ToolCard.tsx         # Agent card: state, clock, step count, latest step, step list
```

**Structure Decision**: the progress logic is a new daemon module `progress.rs`. It holds a
pure reducer, so it can be unit-tested without processes. `sessions.rs` only owns the
channel. The parser stays the only place that knows stream-json shapes. The two new UI
pieces are their own components, so `ConversationView` (already large) only places them.

## Design

### D1. Parser (R1–R4)

- At the top of `Parser::parse`, read `parent_tool_use_id`. If it is set:
  - for `assistant`: emit `SubagentStep` per `tool_use` block, folding the parent to its
    root through `subagent_root: HashMap<child, root>` (filled when a sub-agent's
    `assistant` line itself has an `Agent` tool_use);
  - for `user`: nothing;
  - return. The main `current`, `pending` and `stopped` are untouched.
- Main-thread `assistant` lines: also emit `ToolStarted` for each `tool_use` block, before
  the existing pending logic. The order of `MessageDone` is unchanged.
- `system`: `init` → `Init { self_started: !awaiting_turn }`; the five task subtypes →
  `Task(..)`; everything else ignored.
- `StreamSession::send_user` sets `awaiting_turn`, `Init` clears it. The flag lives in the
  parser behind the session's existing lock, so writer and reader agree.
- The `broadcast` channel already carries `StreamEvent`. The new variants go through it,
  and `live::relay` ignores them (it relays progress from the watch channel instead).

### D2. Progress reducer (`progress.rs`)

`pub fn apply(p: &mut Progress, ev: &StreamEvent, now: DateTime<Utc>) -> bool` (changed?)
implements the rules in [data-model.md](data-model.md). It also has a
`tick(p, now) -> bool` that drops finished background rows 60 s after they end. The pump
calls `apply` for every event and `live.progress.send_if_modified(..)`. A per-session
interval of 5 s, spawned with the pump and ended with it, calls `tick` only while rows wait to expire. Unit tests feed the captured
fixture line by line and check the progress after each step.

### D3. Turn end with background work (R6)

`hooks.rs` already funnels every hook, and the stream's stand-ins for hooks
(`limit_hit_from_stream`), through the private `apply_signal(app, task, session, signal,
&hook)`. The deferral goes **inside it**, before `set_state`, so it is the single choke
point. The replay timer and `run.rs` call the same function (made `pub(crate)`); there is no
second entry point and no extraction.

```text
// in apply_signal, once `live` is known
match signal {
    TurnEnded if live is stream => {
        if live.hold_stop_if_background(hook) {      // one mutex: check + hold atomically
            conversation::flush_queue(app, live);     // queued messages are typed now
            return;                                   // state stays Working; nothing else runs
        }
    }
    PromptSubmitted | Working => live.drop_held_stop(),   // a new turn: the held Stop is stale
    _ => {}
}
```

- `Live::hold_stop_if_background` checks `has_background()` and stores the payload under the
  **same** mutex that the reducer's background set lives in. A `background_tasks_changed []`
  can therefore not slip between the check and the hold. `progress` (watch sender),
  `held_stop` and the timer's cancellation token are `Arc`-wrapped fields of `Live`, which is
  `Clone`.
- **Queued messages**: on deferral, `flush_queue` types them right away, as an idle session
  would (US3 #4). Their turn's `UserPromptSubmit` then drops the held Stop.
- **Release**: when the reducer empties the background set while a payload is held, the
  pump starts a 30 s timer. If it fires, it calls `apply_signal(TurnEnded, held)`. Any
  `Init` (self-started or not), `PromptSubmitted` or `Working` cancels the timer and drops
  the payload. A `Stop` hook that arrives **after** the next turn's `Init` (hooks are async
  HTTP and race the stream) is detected because `Progress.turn` is set with a `started_at`
  later than the hook's arrival minus a 1 s skew. It is then applied as today, never held.
  Tests cover both orders.
- **Exit**: the exit watcher in `run.rs` replays a held payload through `apply_signal`
  before its normal exit handling.
- **Never held**: `TurnFailed`, usage-limit ends and PTY sessions.
- **Interrupt and error results** (`stream_events::turn_result` → `conversation::turn_interrupted`,
  which bypasses Stop): unchanged. "Detener" or an error leaves the session `Waiting` even
  with background items running, because the user is now in charge. The strip keeps showing
  the items, and a later self-started turn makes the session `working` again (D4). Any held
  payload is dropped there too.

### D4. Self-started turn (R4)

In `stream_events::handle`, `Init { self_started: true }` on a live, owned session:

- calls `apply_signal(PromptSubmitted, synthetic hook {"hook_event_name":"UserPromptSubmit","delilah_origin":"agent"})`,
  following the `limit_hit_from_stream` precedent (not logged to `hooks.jsonl`). The
  state, the inbox clear and `UserAnswered` follow the existing path;
- writes history `agent-resumed` (primary session only);
- sets `Progress.turn.by = claude` with the reason taken from the last `Task(Ended)` seen
  ("tarea en segundo plano terminada").

Three consumers must treat `delilah_origin == "agent"` as "not a user prompt":

- `phases::on_hook`: no phase detection and no `drop_hint`;
- `conversation::on_hook`: still runs `reset_turn_state` (today it skips it for a prompt
  with no text);
- `hooks::note_activity`: does not record an empty prompt through `touch_session`.

A real `UserPromptSubmit` for the same turn, if Claude sends one (T001), finds the state
already `working` and `UserAnswered` a no-op. A test covers both orders.

### D5. API and socket

- `conversation::get` adds `"progress": live.progress()` (null if not a stream session).
- `live::relay` gets the session's `watch::Receiver<Progress>`. It sends the current value
  on open, then on each `changed()` at most every 250 ms (sleep-then-borrow, so it always
  sends the latest value).
- `runtime_api`: the refusal check adds `live.has_background()`, with its own message.
- `conversation::send_message`: "queued" means `state == Working && progress.turn.is_some()`,
  or a pending ask. With only background work running (state `Working` but no turn), the
  message is written now, and Claude takes it (US3 #4).
- `conversation::stop`: after the interrupt, `stop_task` for each running item, if T001
  confirms the control request. Otherwise nothing more.

### D6. UI

- `live.ts`: a new `progress` message kind for subscribers (`on(id, "progress", fn)`), as
  the text and done handlers do today.
- `ConversationView`: keeps `progress` from the conversation load, and replaces it with
  socket messages. It places `<WorkingIndicator progress={…} working={…} />`, which takes
  the spot of today's `conv-activity` line (that line stays the fallback when `progress` is
  null, i.e. PTY sessions and older daemons). It places `<BackgroundStrip items={…} />` above
  the composer. A `background-done` marker renders as a divider: "Tarea en segundo plano
  terminada" + detail.
- `WorkingIndicator`: a 1 s ticking clock from `turn.started_at`; the `step`; "sin
  novedades desde hace N" when `now - last_signal_at ≥ 30 s`. Hidden when `turn` is null.
- `BackgroundStrip`: one row per item (icon by kind, description, clock or total time,
  state label: "en curso", "terminado", "fallido", "detenido", "estado desconocido"). Rows
  with `started_at` null show "desde hace ?".
- `ToolCard` (Agent): a new optional `subagent` prop, matched by `tool_use_id`. Header:
  state chip, clock, "N pasos", latest step. The expanded body lists `recent` with "(+K
  anteriores)". Without the prop it renders as today.
- All strings through `t()`, with English entries added where the app keeps translations.

### D7. PTY (free part of US1)

`conversation::on_hook` sets `activity` on `PreToolUse` too (today only on `PostToolUse`),
using the shared summary function. Nothing else changes for PTY sessions.

## Risks

- **Undocumented stream shapes** may change between Claude versions. Mitigations: fixture
  tests, unknown lines ignored, and the deferral falls back after 30 s. The worst case is
  today's behaviour.
- **A held Stop that is never replayed** would leave a task "working" forever. Mitigations:
  the timer, the replay on exit, and the snapshot-based `has_background()`
  (`background_tasks_changed` with `[]` always empties it).
- **The deferral sits in `apply_signal`**, the hottest path. The existing hook tests must
  stay green, and the new branch only applies to stream sessions with background work.
- **Large files**: `conversation.rs` (~2.5k lines) and `run.rs` (~4k) get minimal edits;
  the logic goes into `progress.rs`.

## Plan review

`architecture-reviewer` on the plan (2026-10-26):

- **HIGH, fixed**: a held Stop was dropped only by a self-started `Init`, so a user turn
  could be followed by a stale replay. Any new turn signal now drops it, and the
  Stop-after-Init race is handled (D3).
- **HIGH, fixed**: queued messages would wait out the background run. They are flushed at
  the deferral point, and `turn_interrupted` with background running is decided (D3).
- **HIGH, fixed**: `apply_signal` already exists. The deferral goes inside it, with no
  extraction task (D3).
- **MEDIUM, fixed**: synthetic prompt handling in `phases`, `conversation::on_hook` and
  `note_activity` (D4); atomic check-and-hold, `Arc` fields, tick tied to the pump (D3, D2);
  `deferred_turn_end` removed from the contract.
- **MEDIUM, kept**: full `recent` steps (≤ 50) in each progress message. At about 80 bytes a
  step and ≤ 4 messages/s this stays under 20 KB/s while a sub-agent runs, and it keeps the
  client stateless. Revisit if it shows up in profiles.
- **MEDIUM, noted**: `Init` changes shape, so check every exhaustive match and fixture.
  `ConversationView` must ignore unknown marker kinds. Follow the app's existing `t()`
  convention for strings.

## Complexity Tracking

No constitution violations. One new daemon module (`progress.rs`) and two new UI
components, each for a reason given in the Structure Decision.
