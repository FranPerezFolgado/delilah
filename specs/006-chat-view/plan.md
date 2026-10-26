# Implementation Plan: Chat view for sessions, and an experimental chat mode

**Branch**: `feature/LIBRE-13` | **Date**: 2026-10-24 | **Spec**: [spec.md](spec.md)
**Input**: [spec.md](spec.md), [research.md](research.md), notes in
[docs/spikes/2026-10-23-chat-mode-notes.md](../../docs/spikes/2026-10-23-chat-mode-notes.md).

## Summary

**Chat view.** Every Claude session gets a chat view built from Claude's own transcript.
- A pure, tolerant parser in `delilah-agents` maps transcript messages to entries: user
  text, Claude's text, tool calls paired with their results (diffs computed in Rust),
  and markers.
- The daemon reads the file on demand from a cursor, merges Delilah's stored entries
  (queued messages, cards), and announces growth on the existing `conversations` topic.
  A per-session poll runs once a second.
- `ConversationView` gains tool cards, and becomes the starting view for task sessions
  according to a new per-task `view` setting.
- The composer (send, queue, Detener) is widened from idea conversations to every
  Claude session.

**Chat mode.**
- A spike checks `claude -p` with stream-json first.
- A refactor introduces `Runtime { Pty, Stream }` with no behaviour change.
- An opt-in `chat_mode` flag per task then runs its sessions on a `StreamSession`.
  States still come from hooks. Entries still come from the transcript. Live text goes
  over a per-session WebSocket.
- Permission prompts become "¿Permitir?" cards answered on stdin through Claude's host
  control protocol, with a fallback chain if the spike says otherwise.
- The task can switch runtime between turns.

## Technical Context

**Language/Version**: Rust (edition 2021 workspace), TypeScript + React (Vite) in `app/`
**Primary Dependencies**: tokio, axum, sqlx (SQLite), portable-pty, serde_json; new `similar` (diffs) in `delilah-agents`. App: no new npm dependency
**Storage**: SQLite: migrations `0016_task_view.sql`, `0017_chat_mode.sql`; transcripts are read from `~/.claude/projects/…`, never copied
**Testing**: `cargo test -p delilah-agents` (parser fixtures, StreamSession against a fake `claude` script), `cargo test -p delilah-daemon` (API, watcher, runtime switch with fake PTY or stream programs), `cargo test -p delilah-cli`, `cd app && npx tsc --noEmit && npx vitest run`
**Target Platform**: macOS (daemon via launchd, Tauri app)
**Project Type**: daemon + CLI + desktop web UI
**Performance Goals**: an entry visible ≤ 3 s after Claude writes it (PTY); live text ≤ 1 s (chat mode); 3,000-entry sessions open without lag
**Constraints**:
- The transcript format is not a contract (tolerant parser).
- State only from hooks (#8).
- No terminal in chat mode.
- Clean env and settings outside the repo (#10).
**Scale/Scope**: a handful of live sessions; transcripts up to tens of MB

## Constitution Check

`.specify/memory/constitution.md` is still the unfilled template; the ground rules in
`CLAUDE.md` are the gates:

| Gate | Status |
|---|---|
| Nothing from the work repos; fixtures only | ✅ parser tests use hand-written JSONL fixtures; stream tests use a fake `claude` script; the spike uses a fixture workspace |
| Agent hooks/settings outside the repo, clean env | ✅ the stream runtime uses the same `claude::command` env and `--settings` file; `allowed_tools` goes into that file, never the worktree |
| Try changes only in a preview; never 7420, never `pkill` | ✅ [quickstart.md](quickstart.md) uses `scripts/preview.sh`; the spike runs its own listener on a free port |
| Remote operations only as the confirmed account | ✅ none added |
| Migrations: never edit a shipped one | ✅ new `0016`, `0017` |
| Contracts (`api.rs` ↔ `api.ts`) | ⚠️ changed: [contracts/api.md](contracts/api.md). Additive except the entry `id` type, which only the app reads. Contract review at the end of Phases 2, 6 and 7 |
| #8 (state only from hooks) | ✅ kept: the transcript is display only. Decision entry amends #8 and 004's research |
| #36 (chat view over the PTY; stream-json later) | ⚠️ amended: a second, opt-in runtime. Decision entry |

Post-design re-check: no change.

## Project Structure

### Documentation (this feature)

```text
specs/006-chat-view/
├── spec.md, plan.md, research.md, data-model.md, quickstart.md
├── contracts/api.md
├── checklists/requirements.md, checklists/chat-view.md
└── tasks.md            (speckit-tasks)
```

### Source Code

```text
crates/delilah-agents/
├── src/transcript.rs        NEW  tolerant JSONL/message → Entry mapping, tool summaries, diffs (similar)
├── src/runtime.rs           NEW  enum Runtime { Pty, Stream }: send_prompt, stop, kill, wait, has_exited, as_pty;
│                                 runtime::spawn(&Launch, RuntimeKind) -> Runtime, the only place that picks one
├── src/stream.rs            NEW  StreamSession: spawn claude -p stream-json, stdin writer, stdout reader → typed
│                                 StreamEvent { TextDelta, MessageDone, PermissionRequest{..}, TurnResult, .. };
│                                 interrupt(), reply_permission(id, Decision): the wire protocol stays in here
├── src/claude.rs                 stream_command(launch); settings(..) merges allowed_tools; PreToolUse ask keeps tool_use_id
├── src/pty.rs                    unchanged
├── examples/claude_stream_spike.rs NEW  the spike
└── tests/fixtures/transcripts/*.jsonl NEW

crates/delilah-daemon/
├── migrations/0016_task_view.sql, 0017_chat_mode.sql   NEW
├── src/transcript_watch.rs  NEW  ONE daemon-wide loop: every 1 s (and when a hook pokes it) stats the transcript
│                                 of each live Claude session; emits Topic::Conversations on growth
├── src/conversation.rs           split (Phase 2, before new code): the composer + entries API stay here
│                                 (entries = transcript (cursor) ⊕ stored; guards widened; card ↔ tool_use dedupe;
│                                 stop → runtime.stop(); tool detail route)
├── src/idea.rs              NEW  moved out of conversation.rs: idea-conversation-only logic (Nueva tarea card,
│                                 proposals, the read-only flag); `Live::is_conversation` renamed `is_idea`
├── src/live.rs              NEW  GET /sessions/{id}/live WebSocket (deltas)
├── src/permissions.rs       NEW  its own registry of PermissionAsk (no blocking hook, unlike asks.rs), fed by
│                                 StreamEvent::PermissionRequest, answered via reply_permission; allow_always →
│                                 tasks.allowed_tools; shares only the Inbox surface with asks.rs
├── src/sessions.rs               Live.pty → Live.runtime; transcript path
├── src/run.rs                    launch_session calls runtime::spawn(kind from task.chat_mode) — one call, no
│                                 chat-mode branches; trust dialog only when as_pty() is Some
├── src/runtime_api.rs       NEW  POST /tasks/{key}/runtime (switch between turns)
├── src/terminal.rs               409 for Stream sessions
├── src/hooks.rs                  remember transcript_path; poke the watcher
├── src/asks.rs, inbox.rs         tool_use_id on question cards; inbox lists permission asks too
├── src/actions.rs, handoff.rs, archive.rs, projects.rs, proposals.rs, session_api.rs, scheduler.rs
│                                 mechanical: .pty → .runtime (Phase 5)
├── src/store.rs, store_sessions.rs  task view / chat_mode / allowed_tools
└── src/api.rs                    routes + create/patch fields

crates/delilah-core/src/…         Task gains view, chat_mode, allowed_tools
crates/delilah-cli/src/main.rs    --chat on new/run; `runtime` subcommand; attach prints the 409

app/src/
├── api.ts                        Entry union (+tool, marker, string ids), conversation(cursor), toolDetail,
│                                 permission, setRuntime, Task.view/chat_mode, Session.runtime, live socket
├── components/ConversationView.tsx   cursor-based loading (the API's 500-entry pages are the only paging),
│                                     tool cards (CSS content-visibility for long lists), markers, live bubble,
│                                     permission cards
├── components/ToolCard.tsx       NEW  collapsible card + per-tool bodies (Bash, Edit/Write diff, Read, Grep/Glob,
│                                     TodoWrite, Task/Agent, generic)
├── components/DiffRows.tsx       NEW  extracted from ChangesView (shared rendering)
├── components/PermissionCard.tsx NEW  allow once / always / deny + reason
├── components/TaskScreen.tsx     starting view from task.view; Chat/Terminal switch for all Claude sessions;
│                                 terminal disabled in chat mode; "Pasar a terminal / a modo chat"
├── components/NewTaskDialog.tsx  view select + "Modo chat (experimental)" switch
├── components/InboxView.tsx      permission items
└── i18n strings (es/en)

docs/: spikes/2026-10-2x-claude-stream.md (NEW), decisions.md, status.md, architecture.md, README.md
```

**Structure Decision**: parsing (with the `Entry` types the API serialises as-is) and both runtimes live in `delilah-agents` (agent
knowledge, testable without the daemon). The daemon owns the watching, merging, API and
permission flow. The UI extends the existing ConversationView instead of adding a second
chat component.

## Phasing

One feature, one branch (the task), as asked. Phases 1–4 (US1/US2) are shippable on
their own: **if the spike says drop**, Phases 5–8 are removed from tasks.md, and the
branch is finished after Phase 4 with the docs and the minor bump. **The Phase 5
refactor starts only on the spike's "go"**, never speculatively.

Each phase ends green (`cargo test --workspace`, the Tauri shell tests if touched,
`npx tsc --noEmit && npx vitest run`) and is committed. Phases 1 and 2–4 are
independent. Phases 5–8 need the spike's verdict.

1. **Spike (US5)**:
   - `examples/claude_stream_spike.rs` answers R7 a–h;
   - findings in `docs/spikes/`;
   - a decision entry with the go / adjust / drop verdict for US3/US4.
   - **Gate**: if (a) or (c) fails, Phases 6–8 are cut down per R7 and the tasks are
     revised before continuing.
2. **Transcript → entries (US1, backend)**:
   - first, split `conversation.rs` (composer and entries) from `idea.rs` (idea-only),
     and rename `is_conversation` → `is_idea`, with no behaviour change;
   - `transcript.rs` with fixtures (known types, unknown types and fields, partial line,
     sidechain, compaction, large results, Edit diff);
   - the conversation API for all Claude sessions with a cursor;
   - the tool detail route;
   - `transcript_watch.rs`;
   - hooks remember `transcript_path`.
3. **Chat view UI (US1)**:
   - `ToolCard`, `DiffRows` extraction, cursor loading, "ver anteriores";
   - migration 0016 and `task.view`;
   - TaskScreen's starting view and switch for every Claude session;
   - the view select in NewTaskDialog and on the task;
   - notices for `needs_terminal`, missing or pending transcript.
4. **Composer everywhere (US2)**:
   - guards widened, `flush_queue` for every session;
   - `tool_use_id` on question cards and the card ↔ tool dedupe;
   - idea conversation: stored user/claude entries hidden once a transcript exists.
5. **Runtime refactor (no behaviour change)**:
   - `runtime.rs` with `Runtime::Pty` only in use;
   - `Live.runtime`; every call site moved; `as_pty()` for terminal, attach and trust;
   - existing tests unchanged and green.
6. **Chat mode core (US3)**:
   - `StreamSession` (+ fake-claude tests);
   - `Runtime::Stream`; `claude::stream_command`;
   - migration 0017 and `task.chat_mode`;
   - `launch_session` branching and the spawn-failure message;
   - the live WebSocket and the live bubble;
   - `stop` → interrupt;
   - terminal and attach 409;
   - CLI `--chat`; the NewTaskDialog switch; `Session.runtime` in the UI.
7. **Chat-mode permissions (US3, FR-020)**:
   - `permissions.rs`; Ask kind Permission; `PermissionCard`; the Bandeja item;
   - `allowed_tools` into settings and auto-answers;
   - autopilot pauses on a permission ask;
   - the fallback chain per the spike.
8. **Switch runtime (US4) and resilience**:
   - `POST /runtime` + CLI `delilah runtime`;
   - TaskScreen actions;
   - restart and usage-limit resume in chat mode;
   - the process dying mid-turn.
9. **Docs and release**:
   - decisions (transcript as a display source amending #8 and 004 R-transcript; a
     second runtime amending #36; permissions via the host control protocol, option C);
   - a note that a third runtime would mean a trait instead of the enum;
   - status (known gap: no UI yet to review or revoke "Permitir siempre" rules),
     architecture (new daemon modules: `transcript_watch`, `idea`, `live`, `permissions`,
     `runtime_api`), README;
   - version bump **minor** (new user-facing features, backwards compatible).

## Risks

- **Transcript format drift**: tolerant parser; fixtures with unknown fields; the
  terminal stays one click away.
- **The stream-json input and control protocol are thinly documented**: the spike first,
  the fallback chain in R7, the flag is opt-in, and switching to the terminal is the
  escape hatch.
- **Runtime refactor touches about 70 call sites**: done alone in Phase 5 with no
  behaviour change and the existing tests as the safety net.
- **Rate limits in print mode (R7-f)**: may degrade limit detection in chat mode;
  recorded as a known problem if so.

## Complexity Tracking

| Addition | Why needed | Simpler alternative rejected because |
|---|---|---|
| Second runtime (`StreamSession`) | US3 replaces the PTY for opted-in tasks | Driving the TUI cannot give live text or approval cards |
| Permission asks over the control protocol | FR-020 (option C chosen by the user) | Allow list only: less than PTY tasks get today; kept as the last fallback |
| `similar` crate | Edit cards need diffs (FR-003) | Hand-written diff: more code to test; a UI diff library adds an npm dependency |
