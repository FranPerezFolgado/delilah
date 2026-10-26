# Tasks: Show background work in chat mode

**Input**: Design documents from `specs/009-chat-background-tasks/`
**Prerequisites**: [plan.md](plan.md), [spec.md](spec.md), [research.md](research.md), [data-model.md](data-model.md), [contracts/api.md](contracts/api.md), [quickstart.md](quickstart.md)

**Tests**: test-first, as the project's convention. Every implementation task is preceded by a
task that writes failing tests for it. Checks: `cargo test -p delilah-agents`,
`cargo test -p delilah-daemon`, `cd app && npx tsc --noEmit && npx vitest related --run <files>`.

**Organization**: by user story (spec.md): US1 working indicator (P1), US2 sub-agent card
(P1), US3 background strip and state (P1), US4 self-started turns (P2). US3 depends on US4's
self-started handling for its ending; both are kept separate so US4 can ship alone.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: can run in parallel (different files, no dependency on an unfinished task)
- **[Story]**: US1–US4 from spec.md

---

## Phase 1: Setup (capture and fixtures)

**Purpose**: confirm the stream shapes with Delilah's real flags and freeze them as fixtures.

- [X] T001 Add a `background` scenario to `crates/delilah-agents/examples/claude_stream_spike.rs`: the same flags as `StreamSession` (`--permission-prompt-tool stdio`, partial messages, `--settings` shaped like `claude::settings` with every hook posting to the listener), in a fixture workspace from `scripts/make-fixture-parent.sh`; prompt: a background Bash `sleep 25`, a background sub-agent that globs and reads files, a foreground sub-agent that reads one file, then end the turn and say ALL-DONE when the background work ends; keep stdin open 150 s; also send `{"type":"control_request","request_id":"s1","request":{"subtype":"stop_task","task_id":<first background task_id>}}` in a second run (`background-stop`). Record: whether `Stop` fires for the turn that ends with background work, whether `UserPromptSubmit` fires for self-started turns, whether `stop_task` is accepted and what it answers
- [X] T002 Run T001 on the personal Mac and save the stdout as `crates/delilah-agents/tests/fixtures/stream-background.jsonl` and `stream-background-stop.jsonl` (strip absolute home paths from `output_file` and `cwd` to `/fixture/...`), and the hook posts as `stream-background-hooks.jsonl`; write the answers to R4's open point and R8 into `specs/009-chat-background-tasks/research.md` (section R9 "Results") and a new "Background tasks (2026-10-26)" section of `docs/spikes/2026-10-24-claude-stream.md`

**Checkpoint**: fixtures exist; R4/R8 settled. If `stop_task` is not accepted, T048 becomes a no-op note.

---

## Phase 2: Foundational (parser, progress model, shared summaries)

**Purpose**: the stream events and the in-memory progress every story reads. Blocks all stories.

- [X] T003 [P] Write failing tests in `crates/delilah-agents/src/stream.rs` (tests module) for `Parser`: (a) lines with `parent_tool_use_id` never produce `TextDelta`/`MessageDone` and do not flush or change the main pending message (feed the interleaving from `stream-background.jsonl`: a sub-agent `assistant` between a main `content_block_start` and its deltas; the main message's `MessageDone` is unchanged); (b) a sub-agent `assistant` with a `tool_use` block gives `SubagentStep { parent, tool, input }`; a nested sub-agent's steps are folded to the outermost `Agent` call; (c) a main `assistant` line with a `tool_use` block gives `ToolStarted { tool_use_id, tool, input }` before any later event of that message; (d) `system/task_started|task_progress|task_updated|task_notification|background_tasks_changed` give the `Task(TaskEvent)` variants of data-model.md, unknown statuses → `TaskStatus::Unknown`, unknown system subtypes → nothing; (e) `Init { self_started }` is false after a user line was written and true for an `init` with no user line since the last `result` (the fixture's turns 2 and 3)
- [X] T004 Implement T003 in `crates/delilah-agents/src/stream.rs`: `StreamEvent::{ToolStarted, SubagentStep, Task}`, `TaskEvent`, `TaskKind`, `TaskStatus`, `Init { session_id, self_started }`; parser routing by `parent_tool_use_id` with a `subagent_root` map; `awaiting_turn` set by `StreamSession`'s user-line writer and cleared on `init`; update every match on `StreamEvent::Init` across `crates/` (grep `StreamEvent::Init`) and existing tests/fixtures
- [X] T005 [P] Write failing tests in `crates/delilah-agents/src/transcript.rs` (tests module): a `user` line with `origin.kind == "task-notification"` (and one with only text starting `<task-notification>`) parses to `Entry::Marker { marker: BackgroundDone, detail: Some(<summary first line, ≤200 chars>) }`, not a user entry; a summary-less one gives `detail: None`; and `tool_step_summary(tool, &input)` returns "Leyendo <file>", "Buscando <pattern>", the first 40 chars of a Bash command, else the tool name
- [X] T006 Implement T005 in `crates/delilah-agents/src/transcript.rs`: `MarkerKind::BackgroundDone`, `detail: Option<String>` on `Entry::Marker` (`skip_serializing_if = "Option::is_none"`, serialised `"background-done"`), the task-notification branch in `line_items`/`message_items`, and `pub fn tool_step_summary`; replace `activity_of` in `crates/delilah-daemon/src/conversation.rs` with a call to it (same output; existing `post_tool_use_sets_activity_and_stop_clears_it` stays green)
- [X] T007 Write failing tests in a new `crates/delilah-daemon/src/progress.rs` (tests module) for the pure reducer `apply(&mut Progress, &StreamEvent, now) -> bool` and `tick(&mut Progress, now) -> bool`, replaying `stream-background.jsonl` through the parser: turn set on `Init` (`by` user/claude) and cleared on `TurnResult`/`Exited`; `step` replaced on `ToolStarted` and `SubagentStep` ("Subagente: <description> · <step>", `task_progress.description` preferred); `last_signal_at` bumped on every event; background rows from `Task(Started{background:true})` and the `Running` snapshot (snapshot authoritative; unknown ids added with `started_at: None`; ids leaving it become `completed` unless already final); sub-agents created on `Started{kind: Agent}` or first `SubagentStep`, `steps` from `tool_uses` else counted, `recent` capped at 50; final rows dropped on the next `Init` or by `tick` 60 s after `ended_at`; `Exited` → running rows and sub-agents `stopped`; `has_background()`; `apply` returns false when nothing changed
- [X] T008 Implement T007 in `crates/delilah-daemon/src/progress.rs` (`Progress`, `Turn`, `BackgroundItem`, `Subagent`, `Step`, serde shapes exactly as `contracts/api.md`), register the module in `crates/delilah-daemon/src/lib.rs`
- [X] T009 Write failing tests in `crates/delilah-daemon/src/sessions.rs` (tests module): `Live` holds progress in an `Arc` `watch` channel (`progress()` snapshot, `update_progress(f) -> bool` sends only if modified); `hold_stop_if_background(hook)` holds and returns true only when background runs, atomically with the background set (one mutex); `drop_held_stop()`, `take_held_stop()`, `has_held_stop()`; clones of `Live` share all of it
- [X] T010 Implement T009 in `crates/delilah-daemon/src/sessions.rs`
- [X] T011 Write failing tests in `crates/delilah-daemon/src/stream_events.rs` (tests module): the pump feeds every event to the reducer through `Live::update_progress`; a 5 s tick runs only while final rows wait and stops when the pump ends
- [X] T012 Implement T011 in `crates/delilah-daemon/src/stream_events.rs` (`spawn_pump`)

**Checkpoint**: `cargo test -p delilah-agents && cargo test -p delilah-daemon` green; progress is computed but not yet exposed.

---

## Phase 3: User Story 1 - See that the agent is alive and what it is doing (Priority: P1) 🎯 MVP

**Goal**: a working indicator with the turn clock, the current step from tool start, and "sin novedades desde hace…".

**Independent Test**: quickstart US1 (a 60 s `sleep` in a chat-mode task).

- [X] T013 [P] [US1] Write failing tests in `crates/delilah-daemon/src/conversation.rs` (tests module): `GET /api/v1/sessions/{id}/conversation` returns `progress` (the live snapshot) for a stream session and `null` for a PTY or closed session
- [X] T014 [US1] Implement T013 in `crates/delilah-daemon/src/conversation.rs`
- [X] T015 [P] [US1] Write failing tests in `crates/delilah-daemon/src/live.rs` (tests module): the socket sends `{"progress": …}` right after opening, then on each change at most every 250 ms with the latest value (10 changes in 100 ms → one message with the last state); text/done/reset messages unchanged; no text/done for sub-agent messages
- [X] T016 [US1] Implement T015 in `crates/delilah-daemon/src/live.rs` (`relay` selects on the stream broadcast, the progress `watch` receiver with a 250 ms throttle, and the socket)
- [X] T017 [P] [US1] Write failing tests in `crates/delilah-daemon/src/conversation.rs` (tests module): a `PreToolUse` hook on a PTY session sets `activity` with `tool_step_summary` (today only `PostToolUse` does); `Stop` still clears it
- [X] T018 [US1] Implement T017 in `conversation::on_hook` in `crates/delilah-daemon/src/conversation.rs`
- [X] T019 [P] [US1] Add the types to `app/src/api.ts`: `Progress`, `TurnProgress`, `BackgroundItem`, `SubagentProgress`, `ProgressStep`; `Conversation.progress?: Progress | null`; marker union adds `"background-done"` and `detail?: string`
- [X] T020 [P] [US1] Write failing tests in `app/src/live.test.ts` (create if missing): a `{"progress": …}` socket message reaches `on(id, "progress", fn)` subscribers; existing text/done/reset handling unchanged
- [X] T021 [US1] Implement T020 in `app/src/live.ts`
- [X] T022 [P] [US1] Write failing tests in `app/src/components/WorkingIndicator.test.tsx`: hidden when `turn` is null; shows a clock from `turn.started_at` that advances with fake timers; shows `step`; shows "sin novedades desde hace 30 s" when `now - last_signal_at ≥ 30 s` and not before; English text through `t()`
- [X] T023 [US1] Implement T022 in `app/src/components/WorkingIndicator.tsx` (+ styles in `app/src/components/ConversationView.css`) and its English texts in `app/src/i18n/en/WorkingIndicator.ts`
- [X] T024 [US1] Write failing tests in `app/src/components/ConversationView.test.tsx`: with `progress` from the conversation load the indicator shows; a socket `progress` message replaces it; without `progress` (PTY, older daemon) the old `conv-activity` line still shows; a reload mid-turn shows the server's `started_at`, not a reset clock
- [X] T025 [US1] Implement T024 in `app/src/components/ConversationView.tsx`

**Checkpoint**: US1 works alone in a preview (quickstart US1).

---

## Phase 4: User Story 2 - Follow a sub-agent from its card (Priority: P1)

**Goal**: the Agent tool card shows the sub-agent's state, clock, step count, latest step and step list, live.

**Independent Test**: quickstart US2.

- [X] T026 [P] [US2] Write failing tests in `app/src/components/ToolCard.test.tsx`: an `Agent` card with a `subagent` prop (`state: running`) shows "en curso", a clock, "3 pasos" and the latest step; expanded it lists `recent` and "(+K anteriores)" when `steps > recent.length`; `completed` shows "Hecho · N pasos · M min", `failed` "Fallido"; without the prop it renders as today
- [X] T027 [US2] Implement T026 in `app/src/components/ToolCard.tsx` (+ `ToolCard.css`, English texts in `app/src/i18n/en/` for ToolCard strings, following the per-component file convention)
- [X] T028 [US2] Write failing tests in `app/src/components/ConversationView.test.tsx`: each Agent card gets the `progress.subagents` entry with its `tool_use_id`; two parallel sub-agents each show only their own steps; socket updates reach the right card
- [X] T029 [US2] Implement T028 in `app/src/components/ConversationView.tsx`

**Checkpoint**: US1 + US2 work in a preview.

---

## Phase 5: User Story 4 - A turn Claude starts by itself is work (Priority: P2)

Done before US3 because US3's ending relies on it; independently testable.

**Goal**: a self-started turn moves the session and task to working, with history `agent-resumed`.

**Independent Test**: quickstart US3/US4 (the follow-up turn makes the task working without a user message).

- [X] T030 [P] [US4] Write failing tests in `crates/delilah-daemon/src/stream_events.rs` (tests module): `Init { self_started: true }` on a waiting primary stream session → session `Working`, task back to `running` (`UserAnswered`), the inbox question of the previous turn end cleared, history `agent-resumed` with `reason` "background-done" when the last task event was an end; `self_started: false` does none of that; a real `UserPromptSubmit` hook for the same turn arriving before or after the `Init` leaves the same end state (idempotent)
- [X] T031 [P] [US4] Write failing tests in `crates/delilah-daemon/src/phases.rs` and `crates/delilah-daemon/src/hooks.rs` (tests modules): a synthetic prompt hook with `"delilah_origin": "agent"` does not run phase detection or `drop_hint`, and `note_activity` does not record an empty prompt; in `crates/delilah-daemon/src/conversation.rs`: `on_hook` runs `reset_turn_state` for it
- [X] T032 [US4] Implement T030–T031: in `crates/delilah-daemon/src/stream_events.rs` call `hooks::apply_signal(PromptSubmitted, {"hook_event_name":"UserPromptSubmit","delilah_origin":"agent"})` (make it `pub(crate)`; not logged to `hooks.jsonl`, like `limit_hit_from_stream`) and write history `agent-resumed`; guard `phases::on_hook` in `crates/delilah-daemon/src/phases.rs`, `note_activity` in `crates/delilah-daemon/src/hooks.rs`, and `on_hook` in `crates/delilah-daemon/src/conversation.rs`
- [X] T033 [P] [US4] Write failing tests in `app/src/history.test.ts`: `agent-resumed` renders "Claude continúa: tarea en segundo plano terminada" with reason, "Claude continúa" without; add it to the list of known kinds
- [X] T034 [US4] Implement T033 in `app/src/history.ts` and `app/src/i18n/en/history.ts`
- [X] T035 [P] [US4] Write failing tests in `app/src/components/ConversationView.test.tsx`: a `background-done` marker renders as a divider "Tarea en segundo plano terminada" with its detail; an unknown marker kind renders nothing and does not throw
- [X] T036 [US4] Implement T035 in `app/src/components/ConversationView.tsx` and `app/src/i18n/en/ConversationView.ts`

**Checkpoint**: self-started turns are work; the transcript divider replaces the raw XML bubble.

---

## Phase 6: User Story 3 - See background work still running after the turn ends (Priority: P1)

**Goal**: a turn that ends with background work does not look finished; the strip lists the work; the task ends correctly.

**Independent Test**: quickstart US3 (10 runs for SC-003).

- [X] T037 [US3] Write failing tests in `crates/delilah-daemon/src/hooks.rs` (tests module) for the deferral inside `apply_signal`: (a) `Stop` on a stream session with background running → state stays `Working`, no inbox change, no `AgentAsked`, no `note_turn_done` (review hint, saving-mode boundary, autopilot step), no phase signal, payload held; (b) the same `Stop` with no background → today's behaviour; (c) `PromptSubmitted` or `Working` drops a held payload; (d) a `Stop` arriving after the next turn's `Init` (turn `started_at` later than the hook minus 1 s) is applied, never held; (e) `TurnFailed`, a usage-limit end and a PTY session are never held; (f) a `Running []` snapshot between check and hold cannot lose the Stop (atomic hold)
- [X] T038 [US3] Implement T037 in `crates/delilah-daemon/src/hooks.rs` (`apply_signal` before `set_state`), calling `conversation::flush_queue` at the deferral point
- [X] T039 [US3] Write failing tests in `crates/delilah-daemon/src/conversation.rs` (tests module): messages queued during a turn are typed when that turn's Stop is held (not stranded); with only background work running (state `Working`, `progress.turn` null, no ask) `POST …/messages` types the message now instead of queueing; `turn_interrupted` with background running sets `Waiting`, drops a held payload, and leaves the rows in progress
- [X] T040 [US3] Implement T039 in `crates/delilah-daemon/src/conversation.rs` (`send_message` queue condition, `turn_interrupted`)
- [X] T041 [US3] Write failing tests in `crates/delilah-daemon/src/stream_events.rs` (tests module): when the background set empties with a payload held, a 30 s timer (tokio paused time) replays it through `apply_signal(TurnEnded, held)` → normal end-of-turn effects; any `Init` before 30 s cancels it and drops the payload
- [X] T042 [US3] Implement T041 in `crates/delilah-daemon/src/stream_events.rs` (timer with an `Arc` cancellation token on `Live`)
- [X] T043 [US3] Write failing tests in `crates/delilah-daemon/src/run.rs` (tests module): when a stream process exits with a held payload, it is replayed before the exit handling, and the task ends as after a normal turn end then exit; running rows become `stopped`
- [X] T044 [US3] Implement T043 in the exit watcher in `crates/delilah-daemon/src/run.rs`
- [X] T045 [P] [US3] Write failing tests in `crates/delilah-daemon/src/runtime_api.rs` (tests module): `POST /api/v1/tasks/{key}/runtime` answers 409 with "Hay trabajo en segundo plano: espera a que termine o pulsa Detener" while a session has background running; unchanged otherwise
- [X] T046 [US3] Implement T045 in `crates/delilah-daemon/src/runtime_api.rs` (message in Spanish and English through the daemon's `lang` like the existing refusal)
- [X] T047 [US3] Write failing tests in `crates/delilah-agents/src/stream.rs` and `crates/delilah-daemon/src/conversation.rs` (tests modules), only if T002 confirmed `stop_task`: `StreamSession::stop_task(task_id)` writes the control request; `POST /api/v1/sessions/{id}/stop` sends it for each running background row after the interrupt. If T002 found it unsupported, record that in `research.md` R8 and skip T048
- [X] T048 [US3] Implement T047 in `crates/delilah-agents/src/stream.rs` and `conversation::stop` in `crates/delilah-daemon/src/conversation.rs`
- [X] T049 [P] [US3] Write failing tests in `app/src/components/BackgroundStrip.test.tsx`: hidden with no items; one row per item with description, a ticking clock while running, total time when ended, state labels "en curso", "terminado", "fallido", "detenido", "estado desconocido"; `started_at` null → "desde hace ?"
- [X] T050 [US3] Implement T049 in `app/src/components/BackgroundStrip.tsx` (+ CSS) and `app/src/i18n/en/BackgroundStrip.ts`
- [X] T051 [US3] Write failing tests in `app/src/components/ConversationView.test.tsx`: the strip shows above the composer from `progress.background` (load and socket); "Detener" stays visible while background runs with no turn; the composer sends (not "en cola") when only background work runs
- [X] T052 [US3] Implement T051 in `app/src/components/ConversationView.tsx`

**Checkpoint**: all four stories work; run quickstart US3/US4 ten times for SC-003/SC-004.

---

## Phase 7: Polish & Cross-Cutting Concerns

- [ ] T053 [P] Update `docs/architecture.md` (section "Chat mode"): parser routing and the new stream events, `progress.rs` and the `watch` relay, the held Stop (where, when released, never-held cases), self-started turns and `agent-resumed`, the `background-done` marker, the 409; update the "From hook to effect" table for `TurnEnded` with background running
- [ ] T054 [P] Update `docs/status.md`: chat view/chat mode bullets (working indicator, sub-agent progress, background strip, self-started turns); in "Chat mode, limits and known problems" replace "a sub-agent's inner steps are not shown" with what is shown now (one-line steps, not full nested conversations), and add the 30 s fallback and the undocumented stream shapes
- [ ] T055 [P] Update `README.md` chat mode section with what the user now sees (indicator, sub-agent cards, "En segundo plano")
- [ ] T056 [P] Add a decision to `docs/decisions.md`: background work holds the turn end in memory instead of a new task state (alternatives from research R6), and progress is live-only (R7)
- [ ] T057 [P] Make sure every new Spanish string has its English entry: run `cd app && npx vitest run src/i18n/coverage.test.ts`
- [ ] T058 Run all checks: `cargo test --workspace`, `cargo clippy --workspace -- -D warnings`, `cd app && npx tsc --noEmit && npx vitest run`
- [ ] T059 Run quickstart.md end to end in a preview (`scripts/preview.sh --fresh`, never the installed daemon), US1–US4 and the "Detener" variant; record anything that differs in `specs/009-chat-background-tasks/research.md`

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (T001–T002)**: first; needs the personal Mac and a Claude subscription.
- **Foundational (T003–T012)**: after Setup (tests use the fixtures). Blocks every story.
- **US1 (T013–T025)**: after Foundational.
- **US2 (T026–T029)**: after Foundational and T019 (types); otherwise independent of US1 (T028/T029 touch `ConversationView.tsx` after T025, so sequence them).
- **US4 (T030–T036)**: after Foundational.
- **US3 (T037–T052)**: after Foundational and US4 (its release path relies on self-started `Init` handling); T051–T052 after T025.
- **Polish (T053–T059)**: after the stories it documents.

### Within each story

Failing tests → implementation; daemon before UI where the UI reads new fields (T019 types first).

### Same-file chains (never parallel)

- `conversation.rs`: T006 → T014 → T018 → T032 → T040 → T048
- `stream_events.rs`: T012 → T032 → T042
- `hooks.rs`: T032 → T038
- `ConversationView.tsx`: T025 → T029 → T036 → T052

### Parallel Opportunities

- Foundational: T003 (stream.rs) ‖ T005 (transcript.rs) ‖ T007 (progress.rs) tests; then T004 ‖ T006 ‖ T008.
- US1: T013 ‖ T015 ‖ T017 (daemon) ‖ T019 ‖ T020 ‖ T022 (app).
- US2 UI (T026–T027) ‖ US4 daemon (T030–T032) ‖ US3 runtime_api (T045–T046) ‖ BackgroundStrip (T049–T050).
- Polish docs T053–T057 all parallel.

## Parallel Example: Foundational

```text
Task: "T003 failing parser tests in crates/delilah-agents/src/stream.rs"
Task: "T005 failing transcript tests in crates/delilah-agents/src/transcript.rs"
Task: "T007 failing reducer tests in crates/delilah-daemon/src/progress.rs"
```

## Parallel Example: User Story 1

```text
Task: "T015 failing live socket tests in crates/delilah-daemon/src/live.rs"
Task: "T022 failing WorkingIndicator tests in app/src/components/WorkingIndicator.test.tsx"
Task: "T020 failing live.ts tests in app/src/live.test.ts"
```

## Implementation Strategy

### MVP first

Setup + Foundational + US1: the agent never looks frozen during a turn (the core of the
report), and the parser fix stops sub-agent messages from corrupting the live text. Ship
if needed.

### Incremental delivery

1. MVP (US1).
2. US2: sub-agent cards come alive (most of the silent minutes in orchestrated work).
3. US4: self-started turns count as work.
4. US3: background turn ends no longer look finished; strip; runtime switch guard.
5. Polish: docs, full checks, preview run.

## Notes

- No migration, no persisted data; API and socket changes are additive.
- Don't bump the version (CLAUDE.md).
- Try changes only through `scripts/preview.sh`; never restart or use the installed daemon (7420).
