# Tasks: Chat view for sessions, and an experimental chat mode

**Input**: Design documents from `specs/006-chat-view/`
**Prerequisites**: plan.md, spec.md, research.md, data-model.md, contracts/api.md, quickstart.md

**Tests**: test-first. Write each test task before its implementation and see it fail.

**Checks**: `cargo test -p <crate>`; `cd app && npx tsc --noEmit && npx vitest related --run <files>`; at the end `cargo test --workspace`, `cargo test --manifest-path app/src-tauri/Cargo.toml` and `npx vitest run`.

**Fixtures**: transcripts are hand-written JSONL under `crates/delilah-agents/tests/fixtures/transcripts/`, never copied from a real session of a work repo. Stream tests use a fake `claude` shell script that prints canned stream-json lines and echoes what it reads on stdin.

**Gate**: Phase 2 (the spike) decides Phases 6–9. If it says **drop**, delete Phases 6–9 from this file, record the decision, and go to Phase 10. If it says **adjust**, rewrite the affected tasks of Phases 6–9 to the fallbacks in research.md R7 before starting them.

**Verdict (T004, 2026-10-24)**: **adjust** ([decision 49](../../docs/decisions.md), [spike](../../docs/spikes/2026-10-24-claude-stream.md)). Chat mode runs with `--permission-prompt-tool stdio`; limits come from `rate_limit_event`; interrupt ends the turn from `result`; Remote Control is hidden in chat mode; **T059 is dropped**. Changed tasks are marked *(spike)* below.

## Format: `[ID] [P?] [Story] Description`

## Phase 1: Setup

- [X] T001 Add `similar` (latest 2.x) to `crates/delilah-agents/Cargo.toml` and check it builds (`cargo build -p delilah-agents`).

---

## Phase 2: User Story 5 — Spike: is chat mode viable? (Priority: P1, first) 🔬

**Goal**: answer research.md R7 a–h with Claude Code's installed version, before any chat-mode code.

**Independent Test**: `docs/spikes/2026-10-2x-claude-stream.md` answers a–h with yes/no or the observed behaviour, and the Claude version; `docs/decisions.md` has the verdict.

- [X] T002 [US5] Write `crates/delilah-agents/examples/claude_stream_spike.rs`. It should:
  - create a temp workspace from `scripts/make-fixture-parent.sh` (or a plain `git init` dir);
  - start a tiny HTTP listener on a free port that appends every hook body to a JSONL file;
  - write a settings file shaped like `claude::settings` (all `HOOK_EVENTS` posting to the listener, and the status line command writing its stdin to a file);
  - spawn `claude -p --input-format stream-json --output-format stream-json --verbose --include-partial-messages --session-id <uuid> --settings <file>` with `crate::env::agent_env()` (clean env);
  - write user lines `{"type":"user","message":{"role":"user","content":"…"}}` on stdin;
  - log every stdout line with a timestamp to `<out>/stream.jsonl`.

  Scenarios, each a CLI subcommand of the example:
  - `turns`: three prompts in one process;
  - `resume`: a PTY session via `claude::command`, then a stream turn with `--resume`, then the PTY again;
  - `permission`: ask for `npm --version` without an allow rule, answer the `control_request` with allow and then with deny;
  - `interrupt`: a long task, `{"type":"control_request","request_id":"x","request":{"subtype":"interrupt"}}` after 3 s, then a new prompt;
  - `trust`: a brand-new folder;
  - `ask`: a prompt that makes Claude call AskUserQuestion, with Delilah's blocking ask hook answering it (does the PreToolUse ask hook work in print mode?);
  - `remote`: `--remote-control <name>`.
- [X] T003 [US5] Run the spike scenarios on the personal Mac (clean env, never from inside a Delilah agent's env) and write `docs/spikes/2026-10-2x-claude-stream.md`, in the style of `docs/spikes/2026-10-19-claude-pty.md`. Cover:
  - the Claude version;
  - a table for R7 a–h with the result and evidence: hook names seen; whether the transcript is written in print mode and whether stream `assistant`/`user` messages match transcript lines; `control_request` shapes (`can_use_tool` fields, `permission_suggestions`); interrupt behaviour and which hooks fire; AskUserQuestion through the ask hook in print mode; status line runs or not; delta event shapes with their message id;
  - the gotchas.
- [X] T004 [US5] Add the verdict to `docs/decisions.md` (Agents section): go / adjust (which fallbacks of R7 apply) / drop for US3–US4, and why. Update Phases 6–9 of this file to match (see **Gate**).

**Checkpoint**: verdict recorded; Phases 6–9 match it.

---

## Phase 3: Foundational — conversation split and the transcript parser (blocks US1, US2, US3)

**Purpose**: separate idea-only logic from the composer, and the tolerant parser every story uses. No behaviour change.

- [X] T005 Move idea-conversation-only logic out of `crates/delilah-daemon/src/conversation.rs` into a new `crates/delilah-daemon/src/idea.rs`: the Nueva tarea / proposal card handling, the read-only flag and their hook handling. Rename `Live::is_conversation`/`set_conversation` in `crates/delilah-daemon/src/sessions.rs` to `is_idea`/`set_idea`, and update every caller. Pure move: every existing test passes unchanged (moved with its code).
- [X] T006 [P] Write the fixtures in `crates/delilah-agents/tests/fixtures/transcripts/`:
  - `basic.jsonl`: user prompt, assistant text, Bash `tool_use` + `tool_result`, Edit with old/new strings, Read, Grep, TodoWrite, final text;
  - `unknown.jsonl`: unknown line `type`, unknown content block type, extra fields everywhere;
  - `partial.jsonl`: the last line without `\n` and cut mid-JSON;
  - `noise.jsonl`: thinking blocks, `isMeta` lines, system-reminder and hook-output user lines, a compaction summary;
  - `sidechain.jsonl`: a Task tool with `isSidechain` lines;
  - `large.jsonl`: a 20 KB tool result, and an image block;
  - `ask.jsonl`: an AskUserQuestion `tool_use` + result;
  - `pending.jsonl`: a `tool_use` with no result yet.
- [X] T007 Write failing tests in a new `crates/delilah-agents/src/transcript.rs` (tests module) over the T006 fixtures:
  - `parse(bytes, from_offset) -> Parsed { entries, next_offset, skipped }` returns, for `basic`, the ordered kinds `user, claude, tool(Bash), tool(Edit), tool(Read), tool(Grep), tool(TodoWrite), claude`, with ids `t:<uuid>:<n>` and RFC 3339 `at`;
  - the Bash tool has `summary` (command or description), `result` and `is_error`;
  - the Edit tool has `path`, and `diff` rows `{kind: add|del|ctx|hunk, old, new, text}` with the right +/- counts;
  - `unknown` parses with the known entries only and `skipped > 0`, no panic;
  - `partial` stops before the incomplete line (`next_offset` = start of that line), and a second call with the completed bytes returns it;
  - `noise` gives no thinking/meta/reminder entries, and one `marker: compacted`;
  - `sidechain` gives one `tool(Task)` with prompt and final result and no inner entries;
  - `large` truncates the result to 4 KB with `truncated: true`, and the image is a placeholder `tool`/generic entry;
  - `pending` gives a tool with `result: null`;
  - `ask` gives `tool(AskUserQuestion)` with `tool_use_id`;
  - `parse_message(&Value)` (the shared R3 mapping over a `message` object) gives the same entries for one line's `message`.
- [X] T008 Implement T007 in `crates/delilah-agents/src/transcript.rs`, and export it from `crates/delilah-agents/src/lib.rs`:
  - `Entry` types (serde, API shape per data-model.md);
  - `parse`, `parse_message`;
  - per-tool summaries;
  - diffs with `similar`;
  - `tool_detail(bytes, tool_use_id)` returning the untruncated input and result;
  - a `transcript_path(workspace, session_id)` helper that shares the folder rule with `claude::has_conversation` (refactor that function to use it).

**Checkpoint**: `cargo test -p delilah-agents && cargo test -p delilah-daemon` green; no behaviour change.

---

## Phase 4: User Story 1 — Read any session as a conversation (Priority: P1) 🎯 MVP

**Goal**: every Claude session shows its conversation from the transcript with tool cards, live, with a per-task starting view.

**Independent Test**: quickstart.md "Chat view" steps 1–4 and 6.

### Backend

- [X] T009 [US1] Write failing tests in `crates/delilah-daemon/src/hooks.rs` (tests module): a hook carrying `transcript_path` stores it on the live session (`Live::transcript()`), and a later hook with another path replaces it.
- [X] T010 [US1] Implement T009: `transcript: Mutex<Option<PathBuf>>` on `Live` in `crates/delilah-daemon/src/sessions.rs`, set in `claude_hook` in `crates/delilah-daemon/src/hooks.rs`.
- [X] T011 [US1] Write failing tests in `crates/delilah-daemon/src/conversation.rs` (tests module, a temp HOME with a transcript copied from the fixtures), for `GET /api/v1/sessions/{id}/conversation`:
  - on a task session (not an idea one), the response holds `entries` from the transcript plus stored entries merged by `at`, with `cursor`, `transcript: "present"`, `complete: true` and `runtime: "pty"`;
  - `?cursor=<c>` returns only later transcript entries, plus all stored entries;
  - more than 500 new entries gives `complete: false`;
  - a missing file gives `transcript: "pending"` for a live session that has not had a turn, and `"missing"` otherwise, with the stored entries still returned;
  - a shell session gets 409 `a shell session has no conversation`;
  - ids are strings.
- [X] T012 [US1] Implement T011 in `crates/delilah-daemon/src/conversation.rs`:
  - resolve the path (`Live::transcript()`, else `transcript::transcript_path`);
  - read from the cursor offset and `parse`;
  - merge with stored entries, whose ids are now strings;
  - drop the `conversation = 1` guard for reading.
- [X] T013 [US1] Write failing tests and implement `GET /api/v1/sessions/{id}/conversation/tools/{tool_use_id}` (full input and result, 404 when unknown) in `crates/delilah-daemon/src/conversation.rs`, with the route in `crates/delilah-daemon/src/api.rs`.
- [X] T014 [US1] Write failing tests in a new `crates/delilah-daemon/src/transcript_watch.rs` (tests module, short interval injected):
  - appending a complete line to a live session's transcript emits `Topic::Conversations` within one interval;
  - appending only a partial line emits nothing until it is completed;
  - no live session means no events;
  - `poke(session)` (called from hooks) checks at once.
- [X] T015 [US1] Implement T014: one daemon-wide loop (1 s) over `app.sessions`, remembering the last size per session and forgetting sessions that are gone. Start it with the daemon (`crates/delilah-daemon/src/main.rs` or wherever background loops start, next to the scheduler), and call `poke` from `claude_hook` in `crates/delilah-daemon/src/hooks.rs`.
- [X] T016 [P] [US1] Write failing tests and implement migration `crates/delilah-daemon/migrations/0016_task_view.sql` (`ALTER TABLE tasks ADD COLUMN view TEXT NOT NULL DEFAULT 'chat';`):
  - `Task.view: TaskView { Chat, Terminal }` (serde lowercase, default `Chat`) in `crates/delilah-core/src/task.rs`;
  - read and write in every task query in `crates/delilah-daemon/src/store.rs`, plus `set_view(key, view)`.
- [X] T017 [US1] Write failing tests in `crates/delilah-daemon/src/api.rs` (tests module): `POST /api/v1/tasks` accepts `view`; `PATCH /api/v1/tasks/{key}` with `{"view":"terminal"}` persists it and emits `tasks`; an invalid value gives 400. Then implement (`CreateTask.view`, the `update_task` field).

### UI

- [X] T018 [P] [US1] Extract the diff row rendering from `app/src/components/ChangesView.tsx` into `app/src/components/DiffRows.tsx`, taking rows `{kind, old, new, text}`. ChangesView uses it. Its tests still pass.
- [X] T019 [US1] Update `app/src/api.ts`:
  - the `Entry` union: string ids, `kind: "tool"` and `kind: "marker"` per data-model.md;
  - the `Conversation` response fields (`cursor`, `complete`, `transcript`, `runtime`);
  - `api.conversation(id, cursor?)`;
  - `api.toolDetail(id, toolUseId)`;
  - `Task.view`, and `api.updateTask` accepting `view`.

  Fix the type errors this causes (`npx tsc --noEmit`).
- [X] T020 [US1] Write failing tests in a new `app/src/components/ToolCard.test.tsx`:
  - each card starts collapsed with its summary line and expands on click;
  - Bash shows the command, the output and a failed state when `is_error`;
  - Edit and Write show the path, the diff via DiffRows, and an "Abrir en el editor" button calling the given `onOpenFile(path)`;
  - Read shows the path and range;
  - Grep and Glob show the pattern and count;
  - TodoWrite shows the items;
  - Task shows the prompt and result;
  - an unknown tool shows a generic card (name, JSON input, result);
  - `truncated` shows "Ver todo", which calls `api.toolDetail` and shows the full text;
  - a pending tool (`result: null`) shows a running state.
- [X] T021 [US1] Implement T020 in `app/src/components/ToolCard.tsx`, with styles next to the existing `conv-*` styles and es/en strings in the i18n files.
- [X] T022 [US1] Write failing tests in `app/src/components/ConversationView.test.tsx`:
  - a non-idea task session renders user, claude, tool and marker entries in order;
  - on the `conversations` topic it calls `api.conversation(id, cursor)` and appends new entries, replacing stored ones;
  - `complete: false` triggers another fetch;
  - `transcript: "pending"` shows the waiting notice, and `"missing"` shows "No hay conversación que mostrar" with an "Abrir terminal" button;
  - `needs_terminal` shows the "Claude espera en la terminal" notice with a button;
  - the view sticks to the bottom only when the user was at the bottom;
  - every existing idea-conversation test still passes.
- [X] T023 [US1] Implement T022 in `app/src/components/ConversationView.tsx`: an `onOpenFile(path)` prop wired from `TaskScreen.tsx` to the existing editor tab opening (FR-012), cursor state, merge, `ToolCard` for tool entries, markers, the notices, and `content-visibility: auto` on entries for long sessions.
- [X] T024 [US1] Write failing tests in `app/src/components/TaskScreen.test.tsx`:
  - a Claude task session opens in ConversationView when `task.view` is `chat`, and in TerminalPane when it is `terminal`;
  - the "Chat / Terminal" switch toggles a session either way and stays toggled while the screen is open;
  - a shell session always shows the terminal;
  - project sessions open in chat;
  - the task's view control calls `api.updateTask(key, {view})`.
- [X] T025 [US1] Implement T024 in `app/src/components/TaskScreen.tsx`: the starting view from `task.view`, the existing `terminals` set generalised into a per-session override for every Claude session, the switch in the session tab bar, and the view control in the task's settings area.
- [X] T026 [P] [US1] Write failing tests and implement the "Vista" select ("Chat" / "Terminal", default Chat) in `app/src/components/NewTaskDialog.tsx` and its test file, sent as `view` on create.

**Checkpoint**: US1 independently testable with quickstart "Chat view" 1–4 and 6. Contract review of `api.rs` ↔ `api.ts` for this phase.

---

## Phase 5: User Story 2 — Talk to the agent from the conversation (Priority: P1)

**Goal**: composer, queue, Detener and inline question cards for every Claude session, and the idea conversation enriched without duplicates.

**Independent Test**: quickstart.md "Chat view" steps 5 and 7.

- [X] T027 [US2] Write failing tests in `crates/delilah-daemon/src/conversation.rs` (tests module, a fake PTY like the existing ones):
  - `POST /sessions/{id}/messages` on an idle task session types the text (as `actions::type_prompt` does) and records a `sent` user entry;
  - on a working session it stores a `queued` entry, typed at the next `Stop` by `flush_queue`;
  - `DELETE …/messages/{entry}` with a string id removes a queued one;
  - `POST …/stop` sends ESC on a task session and keeps the queue (as today for ideas);
  - a closed session gives 409.
- [X] T028 [US2] Implement T027, and make `needs_terminal` (FR-008) set for every Claude session, not only ideas: a `Notification` hook for a permission prompt or a dialog sets it, and the next `UserPromptSubmit`/`PostToolUse` clears it. Add a test for a task session in the T027 tests. Widen the guards in `crates/delilah-daemon/src/conversation.rs` to any live Claude session (shell sessions get 409); in `crates/delilah-daemon/src/hooks.rs`, call `flush_queue` on turn end for every session with queued entries, not only ideas.
- [X] T029 [US2] Write failing tests in `crates/delilah-daemon/src/asks.rs` (tests module): the ask hook's `tool_use_id` (from the PreToolUse payload) is stored in the card's body. Then implement it in `crates/delilah-daemon/src/asks.rs`, and in `crates/delilah-daemon/src/conversation.rs` where cards are recorded.
- [X] T030 [US2] Write failing tests in `crates/delilah-daemon/src/conversation.rs` (tests module):
  - a transcript `tool(AskUserQuestion)` whose `tool_use_id` matches a stored card is replaced by that card at the tool's position;
  - an unmatched card stays at its own `at`;
  - for an idea session with a transcript, stored non-queued `user` entries and stored `claude` entries are left out;
  - for an idea session without a transcript, they are kept, as today.

  Then implement the merge rules (research R5).
- [X] T031 [US2] Write failing tests in `app/src/components/ConversationView.test.tsx`: the composer shows for a live task session (not only ideas) and sends with `api.sendMessage`; a queued entry shows "en cola" with a remove button using the string id; "Detener" calls `api.stopMessage`; a pending card renders `AskCard` inline and answering it calls the existing reply API. Then implement in `app/src/components/ConversationView.tsx`, removing the idea-only conditions.
- [X] T032 [US2] Check that the Bandeja (`app/src/components/InboxView.tsx`) and the inline card refresh each other on the `inbox` and `conversations` topics. Add a test in `app/src/components/ConversationView.test.tsx`: an `inbox` event re-fetches, and a card answered elsewhere shows as answered.

**Checkpoint**: US1 + US2 complete, a shippable increment. Contract review of the composer routes (`DELETE …/messages/{entry}` id type).

---

## Phase 6: Foundational — runtime abstraction (only on the spike's "go"; no behaviour change)

**Purpose**: `Live.pty` → `Live.runtime`, so chat mode can add a second runtime.

- [X] T033 Write failing tests in a new `crates/delilah-agents/src/runtime.rs` (tests module, a fake PTY command):
  - `Runtime::Pty` `send_prompt(text)` writes the bracketed paste and then `\r`, with the same bytes as `actions::prompt_bytes`;
  - `stop()` writes ESC;
  - `kill`, `wait` and `has_exited` delegate;
  - `as_pty()` is `Some`.
- [X] T034 Implement T033 in `crates/delilah-agents/src/runtime.rs`:
  - `enum Runtime { Pty(Arc<PtySession>) }`; the `Stream` arm comes in T040;
  - move `sanitize_prompt` and `prompt_bytes` from `crates/delilah-daemon/src/actions.rs` into it, with their tests;
  - add `runtime::spawn(&Launch, RuntimeKind::Pty) -> Runtime`, wrapping `claude::command` and `PtySession::spawn`.
- [X] T035 Replace `Live.pty` with `Live.runtime` in `crates/delilah-daemon/src/sessions.rs` (`remove_if_same` compares runtimes). Move every caller: `actions.rs` (`type_prompt` → `runtime.send_prompt`, plus kill/has_exited), `archive.rs`, `projects.rs`, `proposals.rs`, `session_api.rs`, `scheduler.rs`, `handoff.rs`, `inbox.rs`, `conversation.rs` (`stop` → `runtime.stop()`), `terminal.rs` (`as_pty()`), and `run.rs`, where `launch_session` calls `runtime::spawn` and the trust dialog runs only when `as_pty()` is `Some`. Update the test helpers that build `Live` with a fake PTY.
- [X] T036 Run `cargo test --workspace` and `cargo clippy --workspace --all-targets`: everything is green with no test changed beyond construction helpers.

**Checkpoint**: no behaviour change; the PTY path is untouched for users.

---

## Phase 7: User Story 3 — Run a task in chat mode (Priority: P2)

**Goal**: an opt-in per-task stream runtime with live text, the same states and entries, and terminal/attach refused.

**Independent Test**: quickstart.md "Chat mode" steps 1, 2, 4 and 6.

- [X] T037 [US3] Write a fake `claude` script for tests in `crates/delilah-agents/tests/fixtures/fake-claude-stream.sh`. It prints a `system` init line, then for each stdin user line prints `stream_event` text deltas, an `assistant` message and a `result`. An `interrupt` control request produces a `result` with the interrupt shape found in the spike. A line containing `NEEDS_PERMISSION` produces a `control_request` `can_use_tool` and waits for the `control_response` before continuing. Shapes follow the spike findings (T003): the `interrupt` response is `control_response` `{subtype: success, request_id, response: {still_queued: []}}`, then a `user` message `[Request interrupted by user]` and a `result` `error_during_execution` (no `Stop` hook); a `can_use_tool` request has `request.{subtype, tool_name, tool_use_id, input, permission_suggestions}` and the answer is `control_response` `{response: {subtype: success, request_id, response: {behavior: allow, updatedInput}|{behavior: deny, message}}}`; the script also prints one `rate_limit_event` per turn; deltas are `stream_event` after a `message_start` carrying `message.id`. *(spike)*
- [X] T038 [US3] Write failing tests in a new `crates/delilah-agents/src/stream.rs` (tests module, using T037):
  - `StreamSession::spawn(cmd)` yields `StreamEvent::Init`;
  - `send_user(text)` writes one JSON user line;
  - deltas arrive as `TextDelta { message, text }`, where `message` is the id of the preceding `message_start` (deltas carry no id of their own), and then `MessageDone { message }` *(spike)*;
  - a `rate_limit_event` becomes `RateLimit { five_hour, seven_day }` (percent and reset epoch, converted from the fractional `utilization`) *(spike)*;
  - `TurnResult` arrives at the end of the turn;
  - `interrupt()` writes the interrupt control request and the process stays alive for the next `send_user`;
  - a `can_use_tool` request becomes `PermissionRequest { request_id, tool, input, suggestion }`;
  - `reply_permission(id, Decision::Allow{input}/Deny{message})` writes the `control_response` the spike recorded;
  - unknown event types are skipped;
  - `kill`, `wait` and `has_exited` work;
  - stderr goes to the log.
- [X] T039 [US3] Implement T038 in `crates/delilah-agents/src/stream.rs`: tokio child process, stdin writer behind a mutex, stdout line reader into a `broadcast` of typed events, with the wire shapes kept private to this module.
- [X] T040 [US3] Write failing tests and implement the stream runtime:
  - `Runtime::Stream(Arc<StreamSession>)` in `crates/delilah-agents/src/runtime.rs`: `send_prompt` → `send_user`, `stop` → `interrupt`, `as_pty` → `None`;
  - `RuntimeKind::Stream` in `runtime::spawn`;
  - `claude::stream_command(&Launch)` in `crates/delilah-agents/src/claude.rs`: the same env and `--settings`, with `-p --input-format stream-json --output-format stream-json --verbose --include-partial-messages` and `--session-id`/`--resume`, with no positional prompt (the first prompt goes on stdin), and with **`--permission-prompt-tool stdio`** (the flag that makes `can_use_tool` requests arrive and adds AskUserQuestion to the session's tools; not `--permission-prompts host`) *(spike)*.

  Add an args test like the existing `argv` tests.
- [X] T041 [P] [US3] Write failing tests and implement migration `crates/delilah-daemon/migrations/0017_chat_mode.sql`:
  - `chat_mode INTEGER NOT NULL DEFAULT 0` and `allowed_tools TEXT`;
  - `Task.chat_mode: bool` and `Task.allowed_tools: Vec<String>` (`#[serde(default)]`) in `crates/delilah-core/src/task.rs`;
  - store reads and writes, `set_chat_mode`, `add_allowed_tool` (dedupe) in `crates/delilah-daemon/src/store.rs`.
- [X] T042 [US3] Write failing tests in `crates/delilah-daemon/src/run.rs` (tests module, with the program path pointed at the fake script):
  - starting a task with `chat_mode` launches a `Runtime::Stream` with the prompt sent on stdin;
  - project sessions and idea conversations always launch a PTY;
  - a stream process that exits before `Init` makes the start fail with `chat mode could not start: <stderr tail>`, and the task is left startable;
  - saving-mode handoffs and the usage-limit resume of a chat-mode task relaunch in stream with `--resume`.
- [X] T043 [US3] Implement T042 in `crates/delilah-daemon/src/run.rs`: `launch_session` passes the kind from the task's flag to `runtime::spawn`, and the stream path waits for `Init` (with a timeout) before reporting started. `RateLimit` events feed the same `limits` state as the status line hook does for PTY sessions, since the status line never runs in print mode *(spike)*. No other `if chat_mode` branches.
- [X] T044 [US3] Write failing tests in `crates/delilah-daemon/src/api.rs` and `crates/delilah-daemon/src/session_api.rs` (tests modules):
  - `POST /api/v1/tasks` accepts `chat_mode`;
  - task JSON has `chat_mode` and `allowed_tools`;
  - session JSON has `runtime: "pty"|"stream"` (live runtime, else the task's flag; project sessions are always `pty`).

  Then implement.
- [ ] T045 [US3] Write failing tests in `crates/delilah-daemon/src/terminal.rs` (tests module): `GET /sessions/{id}/terminal` and `GET /tasks/{key}/terminal` on a stream session return 409 with the chat-mode message from contracts/api.md. Then implement via `as_pty()`.
- [ ] T046 [US3] Write failing tests in a new `crates/delilah-daemon/src/live.rs` (tests module):
  - `GET /api/v1/sessions/{id}/live` on a stream session relays `{message, text}` deltas and `{message, done: true}` from the session's events;
  - nothing is replayed to a late client;
  - a PTY session closes with code 4409 and the reason `not in chat mode`.

  Then implement, and add the route in `crates/delilah-daemon/src/api.rs`.
- [ ] T047 [US3] Write failing tests in `crates/delilah-daemon/src/conversation.rs` (tests module): `POST …/stop` on a stream session calls `interrupt`, keeps the session live, records a `marker: stopped`, and the next message starts a turn. No `Stop` hook fires after an interrupt, so the daemon ends the turn from the `TurnResult` (`error_during_execution`) and sets the session idle, as it does for ESC in the PTY *(spike)*. Then implement (the runtime `stop()` already branches, so add the marker for both runtimes).
- [X] T048 [P] [US3] Write failing tests and implement `--chat` on `delilah new` and `delilah run` (create body `chat_mode: true`) in `crates/delilah-cli/src/main.rs`. `delilah attach` on a 409 prints the API message and exits 1.
- [X] T049 [US3] Update `app/src/api.ts`: `Task.chat_mode`, `Task.allowed_tools`, `Session.runtime`, create with `chat_mode`, and `liveSocket(id)`. Write failing tests in `app/src/components/ConversationView.test.tsx`: on a stream session, deltas from a mocked socket render a "writing" bubble that grows, and it is replaced when the transcript entry for that message arrives. Then implement it in `app/src/components/ConversationView.tsx`.
- [X] T050 [US3] Write failing tests and implement in `app/src/components/TaskScreen.tsx` and its test: for a `runtime: "stream"` session, the Terminal switch is disabled with the tooltip "Esta sesión va en modo chat: no hay terminal". The task header shows an "experimental: modo chat" mark. Remote Control is hidden for chat-mode tasks (the spike could not confirm `--remote-control` works with `-p`, R7-h) *(spike)*.
- [X] T051 [P] [US3] Write failing tests and implement in `app/src/components/NewTaskDialog.tsx` and its test: a "Modo chat (experimental)" switch, off by default, with a one-line warning, sent as `chat_mode`. Do the same in the start dialog if it is a separate component (check where `autopilot` is offered and mirror it). If a start fails with `chat mode could not start`, offer "Iniciar en terminal", which retries with `chat_mode` off.

**Checkpoint**: US3 without permission cards works with the fake script and in a preview. Contract review (task and session fields, live socket, 409s).

---

## Phase 8: User Story 3 — Permissions in chat mode (Priority: P2, FR-020)

**Goal**: "¿Permitir?" cards in the conversation and the Bandeja (allow once, always in this task, deny with a reason), through the control protocol with `--permission-prompt-tool stdio` (spike: d2 is yes, so R8 stands as written).

**Independent Test**: quickstart.md "Chat mode" step 3.

- [ ] T052 [US3] Write failing tests in a new `crates/delilah-daemon/src/permissions.rs` (tests module, with the fake script's `NEEDS_PERMISSION`):
  - a `PermissionRequest` creates a `PermissionAsk`, emits `inbox` and `conversations`, and sets the session to waiting (`NeedsUser`-like attention);
  - `answer(ask, AllowOnce)` replies allow with the original input;
  - `AllowAlways` also appends the suggestion (or the tool name) to `tasks.allowed_tools`;
  - `Deny{reason}` replies deny with the reason, or "El usuario lo ha denegado" when there is none;
  - a later request matching an allowed rule is answered at once with no ask;
  - stop, kill and runtime switch release pending asks as denied ("sesión detenida").
- [ ] T053 [US3] Implement T052 in `crates/delilah-daemon/src/permissions.rs`: its own registry, a subscriber per stream session started from `launch_session`'s stream path, and rule matching (`Tool` and `Bash(<prefix>:*)` as Claude writes them).
- [ ] T054 [US3] Write failing tests and implement `allowed_tools` in the session settings: `claude::settings(..)` in `crates/delilah-agents/src/claude.rs` takes the task's extra allow rules and puts them in `permissions.allow` for task sessions. The `Permission::DontAsk` list is unchanged.
- [ ] T055 [US3] Write failing tests and implement `POST /api/v1/sessions/{id}/permissions/{ask}` (`allow_once|allow_always|deny`, `reason`; 204; 404 when gone) in `crates/delilah-daemon/src/permissions.rs`, with the route in `crates/delilah-daemon/src/api.rs`.
- [ ] T056 [US3] Write failing tests in `crates/delilah-daemon/src/inbox.rs` and `crates/delilah-daemon/src/conversation.rs` (tests modules): the inbox lists permission asks as kind `permission` (tool, summary, suggestion); the conversation includes a `card` with `ask_kind: "permission"` at the request's time, and it disappears when answered. Then implement.
- [ ] T057 [US3] Write failing tests in `crates/delilah-daemon/src/autopilot.rs` (tests module): an autopilot task in chat mode pauses into the Bandeja on a permission ask, with the reason "necesita permiso: <tool>". Then implement.
- [ ] T058 [US3] Update `app/src/api.ts` (permission card entry, inbox kind, `api.answerPermission`). Write failing tests and implement `app/src/components/PermissionCard.tsx` with its test: the tool and an input summary; "Permitir una vez", "Permitir siempre en esta tarea" (with the rule shown) and "Denegar" (an optional reason field). Render it from `ConversationView.tsx` and `InboxView.tsx`, and add tests in both.
- [ ] T059 [US3] ~~DROPPED (spike d2 = yes with `--permission-prompt-tool stdio`; no fallback needed).~~ ~~Only if the spike's d2 was "no": replace T052–T055's control-protocol path with the fallback the spike chose, and update this task list first:
  - `--permission-prompt-tool` with a stdio MCP server `delilah mcp-permission` in `crates/delilah-cli` that calls the daemon; or
  - `--permission-prompts none` with the task's allow list, refusals shown as tool errors in the chat, and "Pasar a terminal" offered.~~

**Checkpoint**: US3 complete. Security review of the permission flow (rules written into settings, and the reply endpoint).

---

## Phase 9: User Story 4 — Switch a chat-mode task to the terminal and back (Priority: P2)

**Goal**: an escape hatch between turns, on the same conversation.

**Independent Test**: quickstart.md "Chat mode" step 5.

- [ ] T060 [US4] Write failing tests in a new `crates/delilah-daemon/src/runtime_api.rs` (tests module, with the fake PTY and the fake stream):
  - `POST /api/v1/tasks/{key}/runtime {"mode":"terminal"}` on an idle chat-mode task sets `chat_mode = 0`, kills the stream sessions and relaunches them as PTY with `--resume`, then returns 204;
  - `{"mode":"chat"}` does the reverse;
  - while a session is working, or has a pending question or permission ask, it returns 409 `wait for the turn to end or press Detener`;
  - the same mode is a no-op 204;
  - `tasks` and `sessions` are emitted.
- [ ] T061 [US4] Implement T060 in `crates/delilah-daemon/src/runtime_api.rs`, with the route in `crates/delilah-daemon/src/api.rs`, reusing `launch_session` with `resume = true`.
- [ ] T062 [P] [US4] Write failing tests and implement `delilah runtime <task> chat|terminal` in `crates/delilah-cli/src/main.rs`, printing the 409 message on refusal.
- [ ] T063 [US4] Write failing tests and implement in `app/src/components/TaskScreen.tsx` and its test: "Pasar a terminal" on a chat-mode task and "Pasar a modo chat" on a PTY task, both calling `api.setRuntime`. A 409 shows its message. After switching to the terminal, the Terminal switch is enabled. Add `api.setRuntime` in `app/src/api.ts`.
- [ ] T064 [US4] Write failing tests in `crates/delilah-daemon/src/run.rs` (tests module): a stream process that dies mid-turn marks the session as exited, like a PTY that exits, and the task can be resumed in either mode. A daemon restart (the existing restart-resume path) relaunches a chat-mode task's live session in stream. Then implement any gaps.

**Checkpoint**: US3 + US4 complete. Contract review (`/runtime`, CLI).

---

## Phase 10: Polish & cross-cutting

- [ ] T065 [P] Add to `docs/decisions.md`:
  - Claude's transcript as a display source: amends #8 (state still only from hooks) and 004's research rejection; tolerant parser.
  - Chat mode as a second, opt-in runtime: amends #36; an enum, not a trait, and a third runtime would mean a trait.
  - Chat-mode permissions via the host control protocol (option C), with the fallback taken if any.
  - The per-task view setting.

  Skip the chat-mode entries if the spike dropped it.
- [ ] T066 [P] Update `docs/status.md`:
  - the chat view and chat mode as built;
  - remove "streaming of Claude's text and a chat view for other sessions" from pending;
  - known gaps: no UI to review or revoke "Permitir siempre" rules; rate limits in chat mode, if the spike found a gap.
- [ ] T067 [P] Update `docs/architecture.md`: the transcript as a display source, the new daemon modules (`transcript_watch`, `idea`, `live`, `permissions`, `runtime_api`), `delilah-agents::{transcript, runtime, stream}`, and the Runtime enum.
- [ ] T068 [P] Update `README.md`: the chat view (cards, Chat/Terminal switch, the per-task view), the composer in every session, chat mode (experimental, permission cards, switching), `delilah new|run --chat` and `delilah runtime`.
- [ ] T069 Check SC-003: in a preview, open a session whose transcript has about 3,000 entries (generate one by concatenating `basic.jsonl` with fresh uuids) and confirm it opens and scrolls without visible lag. Run the quickstart in a preview (`scripts/preview.sh --fresh`) end to end with a real Claude, and fix what fails. Never use 7420.
- [ ] T070 Run `cargo test --workspace`, `cargo clippy --workspace --all-targets`, `cargo test --manifest-path app/src-tauri/Cargo.toml`, and `cd app && npx tsc --noEmit && npx vitest run`.
- [ ] T071 Bump the version, **minor** (new user-facing features, backwards compatible), from the version on `main`, in the four places in `docs/development.md#releasing` plus the lockfiles, as its own commit `chore: bump version to X.Y.Z`.

---

## Dependencies & execution order

- **Phase 1** → everything.
- **Phase 2 (US5 spike)** is independent of Phases 3–5 and can run in parallel with them, but it **gates Phases 6–9**.
- **Phase 3** (foundational: split and parser) → Phases 4, 5 and 7.
- **Phase 4 (US1)** → **Phase 5 (US2)**, which shares ConversationView and the conversation routes.
- **Phase 6** (runtime refactor) → Phases 7–9, and starts only on the spike's "go".
- **Phase 7** → **Phase 8** → **Phase 9**.
- **Phase 10** last; T065–T068 can run in parallel.

### Parallel opportunities

- T002–T004 (spike) alongside T005–T008.
- T006 (fixtures) alongside T005.
- T016 (migration 0016) and T018 (DiffRows) alongside T011–T015.
- T026 (NewTaskDialog view) alongside T024–T025.
- T041 (migration 0017) and T048 (CLI) alongside T038–T040.
- T051 alongside T049–T050.
- T062 alongside T060–T061.
- T065–T068.

## Implementation strategy

1. **MVP**: Phases 1, 3 and 4 (US1). The chat view for every session, read-only plus the terminal switch.
2. **Increment**: Phase 5 (US2). The conversation replaces the terminal for normal work. Phases 1–5 can ship alone (Phase 10 with the chat-mode parts skipped) if the spike says drop.
3. **Experimental**: Phases 6–9 (US3, US4) on the spike's "go", then Phase 10.
