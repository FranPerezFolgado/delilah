# Implementation Plan: Create a task from an idea, with Claude

**Branch**: `feature/LIBRE-4` | **Date**: 2026-10-21 | **Spec**: [spec.md](spec.md)

**Input**: Feature specification from `specs/004-task-from-idea/spec.md`

## Summary

"Nueva tarea con Claude" opens an **idea conversation**: a project session flagged
`conversation`, read-only through `--permission-mode dontAsk` with an allow list. It is
shown in a new conversation view instead of a terminal. A Delilah plugin, passed with
`--plugin-dir`, gives Claude a `task-from-idea` skill. The skill has Claude ask with
AskUserQuestion and end with a fixed-shape confirmation card (`header: "Nueva tarea"`,
title as the question, brief as the `Crear` option's `preview`).

The **daemon** creates the backlog task when the user answers `Crear`, not Claude. The
answer Claude receives carries the new key. The view's history is a new
`conversation_entries` table fed by the hooks Delilah already receives
(`UserPromptSubmit`, `Stop`, the ask hook). Messages sent while Claude works are held in
a daemon-side queue, and "Detener" sends `ESC`. Project sessions get the same skill and
keep their terminal.

## Technical Context

**Language/Version**: Rust (workspace, edition as in `Cargo.toml`); TypeScript + React (`app/`)

**Primary Dependencies**: axum, sqlx (SQLite), tokio, serde; React, Vite; vitest + Testing Library. Claude Code ≥ 2.1.292 (`--plugin-dir`, `--append-system-prompt`, `--permission-mode dontAsk`)

**Storage**: SQLite. Migration `0011_idea_conversations.sql`: `sessions.conversation`, plus the new `conversation_entries` table. The plugin's files go in `$DELILAH_HOME/plugins/delilah/`, written at start-up.

**Testing**: `cargo test -p delilah-agents`, `cargo test -p delilah-daemon` (`Store::in_memory()`, hook posts as in `hooks.rs` tests, asks as in `asks.rs` tests); `cd app && npx tsc --noEmit && npx vitest run`

**Target Platform**: macOS (Tauri app); the web UI in a preview for trying it

**Project Type**: desktop-app (daemon + web UI)

**Performance Goals**: a task appears within 5 s of "Crear" (SC-001). It is in fact one SQLite transaction plus an SSE `tasks` event. The view refetches on the `conversations` topic.

**Constraints**:
- No hooks, skills or settings in the repo or `~/.claude` (decision 9).
- The agent's env stays clean.
- Task sessions never get the plugin or the proposal handling (FR-014).
- The terminal stays reachable (FR-004).
- Additive API only.

**Scale/Scope**: 1 migration, ~3 daemon modules touched + 1 new (`conversation.rs`), the agents crate's `Launch`, 1 new view + changes to `AskCard`, `TaskScreen`, `Sidebar`/project menu and `NewTaskDialog`; plugin text; docs.

## Constitution Check

`.specify/memory/constitution.md` is still the unfilled template. The project's ground
rules (`CLAUDE.md`) serve as gates instead:

| Gate | Status |
|---|---|
| Nothing from the work repos; fixtures only | ✅ tests use `Store::in_memory()` and fixture repos from `scripts/preview.sh` |
| Agent hooks/settings outside the repo, clean env | ✅ plugin and settings under `$DELILAH_HOME`, passed by flag; `env.rs` untouched; no `DELILAH_URL` given to agents |
| Try changes only in a preview, never 7420 / pkill | ✅ quickstart uses `scripts/preview.sh` |
| No remote operations without confirming the account | ✅ none |
| Migrations never edit a shipped one | ✅ new `0011` |
| Contracts (`api.rs` ↔ `api.ts`) | ⚠️ changed, additively: see [contracts/api.md](contracts/api.md); both sides in the same package |

Post-design re-check: no change. The one new external dependency is behaviour of
Claude Code (`dontAsk`, `--plugin-dir`, the AskUserQuestion answer string). It is
verified by a spike (WP-1) before anything builds on it.

## Project Structure

### Documentation (this feature)

```text
specs/004-task-from-idea/
├── spec.md
├── plan.md              # this file
├── research.md          # R1–R7, incl. checklist resolutions
├── data-model.md
├── contracts/api.md
├── quickstart.md
├── checklists/{requirements,conversation}.md
└── tasks.md             # /speckit-tasks
```

### Source Code

```text
crates/delilah-core/src/task.rs                 # Event::CreatedFromConversation
crates/delilah-agents/src/claude.rs             # Launch: plugin_dir, append_system_prompt, Permission; settings permissions
crates/delilah-agents/plugin/                   # NEW: plugin.json + skills/task-from-idea/SKILL.md (include_str!)
crates/delilah-daemon/migrations/0011_idea_conversations.sql   # NEW
crates/delilah-daemon/src/store_sessions.rs     # conversation flag; entries CRUD
crates/delilah-daemon/src/store.rs              # create_task_from_conversation (task + event, one tx)
crates/delilah-daemon/src/api.rs                # routes; create_task core extracted for reuse
crates/delilah-daemon/src/session_api.rs        # open_in_project {conversation}; launch flags per owner/kind
crates/delilah-daemon/src/conversation.rs       # NEW: GET conversation, messages, delete queued, stop, queue flush
crates/delilah-daemon/src/hooks.rs              # record entries; activity; needs_terminal; flush queue on Stop
crates/delilah-daemon/src/asks.rs               # proposal detection, create on Crear, card entries, release on stop
crates/delilah-daemon/src/sessions.rs           # Live: activity, needs_terminal
crates/delilah-daemon/src/events.rs             # Topic::Conversations
crates/delilah-daemon/src/main.rs|state.rs      # write the plugin dir at start-up
app/src/api.ts                                  # types + client fns
app/src/components/ConversationView.tsx         # NEW (+ .test.tsx)
app/src/components/AskCard.tsx                  # preview, confirmation card (+ test)
app/src/components/TaskScreen.tsx               # conversation sessions → ConversationView; Ver terminal toggle
app/src/components/Sidebar.tsx, ProjectView.tsx # "Nueva tarea con Claude"
app/src/components/NewTaskDialog.tsx            # third origin "Con Claude"
app/src/components/TaskView.tsx|BacklogTaskView.tsx # history line for the new event
app/src/i18n…                                   # new strings
docs/{status,architecture,decisions}.md, README.md
```

**Structure Decision**: no new crate. The plugin's text lives in `delilah-agents`,
because it is Claude-specific, next to `claude.rs`. The daemon writes it out at
start-up. Conversation endpoints get their own module (`conversation.rs`), so
`session_api.rs` (already 17 K) does not grow.

## Design

### 1. Launching (agents crate + `session_api.rs`)

`Launch.read_only: bool` becomes `permission: Permission { Default, Plan, DontAsk }`.
Two fields are added, `plugin_dir: Option<&Path>` and `append_system_prompt: Option<&str>`.
`settings()` takes the permission and, for `DontAsk`, adds the R3 `permissions` block.

| Session | Permission | Plugin | System prompt |
|---|---|---|---|
| Task (primary/extra) | Default | — | — |
| Project session, read-only | Plan | ✅ | — |
| Project session, "Permitir cambios" | Default | ✅ | — |
| Idea conversation | DontAsk | ✅ | ✅ |

Reopen and `--resume` rebuild the same flags from the session record. "Convertir en
tarea" relaunches as a task session, so the plugin is gone (CHK019).

### 2. Proposal → task (`asks.rs`, `store.rs`, `api.rs`)

- `asks::hook` sets `Ask.proposal` (data-model rules). For a conversation session it
  stores a `card` entry.
- `asks::answer`: if `proposal` is set and the answer is `Crear`, it calls
  `tasks::create_from_proposal(app, session, proposal)`. That uses the core extracted
  from `api::create_task`: title check, `next_free_key`, project workflow, notes with
  the mark line, and `Store::create_task_from_conversation`, which writes the task and
  the history event in one transaction. The answer string sent to Claude is then
  rewritten. The card entry is updated (`task` or `error`), and `tasks`, `inbox` and
  `conversations` are emitted.
- The hook still holds Claude until the answer, so Claude always sees the outcome.

### 3. History and live state (`hooks.rs`, `sessions.rs`, `conversation.rs`)

For `conversation` sessions only:
- **`UserPromptSubmit`**: marks the matching `sent` entries `delivered`, or adds a
  terminal entry.
- **`Stop`**: adds a `claude` entry, clears `activity`, then flushes the queue.
- **`PostToolUse`**: sets `activity` from `tool_name` + `tool_input` (file path, pattern,
  or command head).
- **`Notification(permission_prompt)`**: sets `needs_terminal`.

Each change emits `conversations`. The queue flush types the queued texts joined by
`\n\n` with `actions::type_prompt` and marks them `sent`.

`conversation::stop`:
1. writes `\x1b` to the PTY;
2. releases the session's pending asks (`Asks::release_session`, new, next to
   `release_task`) and marks their cards `withdrawn`;
3. sets the state to `Waiting`;
4. flushes the queue.

### 4. UI

- **`ConversationView`**. Props: `session`, `onOpenTerminal`. It fetches
  `GET …/conversation` and refetches on `conversations`. It renders:
  - the empty-state hint (CHK001);
  - user bubbles, with an "en cola" badge and ✕ on queued ones;
  - Claude replies through `Markdown`;
  - card entries: the live `AskCard` when `pending`, a summary of the answer
    otherwise, and a link to the task when one was created;
  - a status line: activity, "Esperando tu respuesta", "Pausado por el límite",
    "Claude espera en la terminal" with a button, or "Detenido" with "Reabrir";
  - the composer: textarea, Enviar (⌘↵), and Detener while working.
- **`AskCard`**:
  - renders `options[].preview` as Markdown;
  - when `proposal` is set, uses the confirmation layout: title heading, brief open
    (max-height 60 %, scroll), buttons Crear / Cambiar algo / Descartar;
  - after Crear, shows the created key or the error.
- **`TaskScreen`**: a project session with `conversation` shows `ConversationView` by
  default, with a "Ver terminal" / "Ver conversación" toggle. The `TerminalPane` stays
  mounted only while the terminal is shown.
- **Entry points**:
  - the project "+" menu and `ProjectView` gain "Nueva tarea con Claude";
  - `NewTaskDialog` gains the origin "Con Claude". With it the form is just project +
    an optional "Tu idea" textarea. Submit opens the conversation (with the text as
    `prompt`) and navigates to it.
- **Sidebar**: conversations are listed with the project's sessions, with an idea icon
  and the label rule from CHK008.

### 5. Docs

- `docs/status.md`: built and pending.
- `docs/architecture.md`: the conversation session, the entries table, the plugin and the
  launch table.
- `docs/decisions.md`: #26 "Tasks from ideas: the daemon creates them on the
  confirmation card", and #27 "Idea conversations: a view over the PTY fed by hooks, not
  stream-json (left for later)".
- `README.md`: "Nueva tarea con Claude".

## Checklist resolution

All 30 items of `checklists/conversation.md` are resolved in research.md: R1 (CHK016,
017), R3 (CHK011), R5 (CHK004–006), R6 (CHK007, 010, 015) and the R7 table (the rest).
No item is left open. Deliberate scope limits:
- no deleting conversations;
- no streaming;
- no conversation view for task or project sessions.

## Testing

- **agents**:
  - the command line per permission (plan / dontAsk / none), `--plugin-dir` and
    `--append-system-prompt` present only when set;
  - the settings' `permissions` block for `DontAsk`;
  - the plugin files are valid JSON / front-matter.
- **daemon**:
  - `a_crear_answer_on_a_project_proposal_creates_a_backlog_task_with_the_brief`;
  - `a_task_sessions_card_with_the_same_shape_creates_nothing`;
  - `a_second_answer_cannot_create_a_second_task`;
  - `an_empty_title_answers_with_the_reason_and_creates_nothing`;
  - `messages_sent_while_working_are_queued_and_typed_together_after_stop`;
  - `stop_withdraws_pending_cards_and_flushes_the_queue`;
  - `a_prompt_typed_in_the_terminal_is_recorded_once`;
  - `entries_survive_a_new_store_on_the_same_file` (history persistence);
  - `opening_a_conversation_session_needs_claude_and_a_project`;
  - migration test on an existing DB (`conversation` defaults to 0).
- **app**:
  - `ConversationView` renders entries, queued badge + remove, Detener only while
    working, terminal toggle;
  - `AskCard` renders the preview and the confirmation layout and shows the key;
  - `NewTaskDialog` "Con Claude" opens a conversation.
- **By hand**: quickstart.md, including the WP-1 spike.

## Complexity Tracking

| Choice | Why needed | Simpler alternative rejected because |
|---|---|---|
| Daemon-side message queue | Order and state must be deterministic and testable | The TUI's own queue is undocumented (R5) |
| `Permission` enum replacing `read_only` | Three modes now (Default/Plan/DontAsk) | Two booleans would allow nonsense combinations |
