# Tasks: Create a task from an idea, with Claude

**Input**: Design documents from `specs/004-task-from-idea/`
**Prerequisites**: plan.md, spec.md, research.md, data-model.md, contracts/api.md, quickstart.md

**Tests**: test-first (cargo test, vitest). Write each test task before its implementation and watch it fail.

**Checks**:
- Rust: `cargo test -p delilah-core`, `cargo test -p delilah-agents`, `cargo test -p delilah-daemon`.
- App: `cd app && npx tsc --noEmit && npx vitest related --run <files>`.
- At the end: `cargo test --workspace` and `cd app && npx vitest run`.

**Never** try changes against the daemon on 7420. Only use `scripts/preview.sh`.

## Format: `[ID] [P?] [Story] Description`

---

## Phase 1: Setup (spike + plugin text)

**Purpose**: verify the Claude Code behaviour the design relies on (research R3, R5, R1) before building on it, and write the plugin's text.

- [X] T001 Spike, by hand in a preview, with no code committed. Follow quickstart.md "Spike":
  - Start `scripts/preview.sh`.
  - Write a throw-away settings file under `fixtures/preview-home-N/tasks/@<project>/` that copies an existing project session's settings (same hooks) and adds R3's `permissions.allow` / `permissions.deny`.
  - Run `claude --settings <it> --permission-mode dontAsk --plugin-dir <tmp plugin dir with a minimal skill> --append-system-prompt "…"` in a fixture repo.
  - Record in `specs/004-task-from-idea/research.md`, in a new "Spike results (2026-10-…)" section under R3/R5:
    - (a) Edit/Write and `npm install` are refused without any prompt;
    - (b) `git log` runs;
    - (c) AskUserQuestion reaches the PreToolUse hook (see `hooks.jsonl`) with `options[].preview`;
    - (d) a hook reply whose `updatedInput.answers` value is `"Crear — creada LIBRE-9 en el backlog"` reaches Claude verbatim;
    - (e) writing `\x1b` to the PTY stops a turn, and whether `Stop` fires after it;
    - (f) the plugin's skill is listed and invocable as `/delilah:task-from-idea`.
  - If (a), (c) or (d) fails, stop and report: the design needs the research.md fallbacks before Phase 2.
- [X] T002 [P] Create the plugin source:
  - `crates/delilah-agents/plugin/.claude-plugin/plugin.json`: `{"name":"delilah","description":"Delilah: turn an idea into a task","version":"0.0.0"}`. The version is replaced at write time.
  - `crates/delilah-agents/plugin/skills/task-from-idea/SKILL.md`: front-matter `name: task-from-idea` and a `description` that triggers when the user wants a task made from an idea or from the conversation. The body is the six-point protocol in contracts/api.md "The plugin", written as instructions to Claude in English. It tells Claude to answer the user in their language and to include the four brief sections in Spanish headings (`## Idea`, `## Resumen`, `## Zonas afectadas`, `## Preguntas abiertas`).

**Checkpoint**: spike results are written down and nothing in them blocks the design.

---

## Phase 2: Foundational (blocks every story)

**Purpose**: launch flags, schema, event, topic and API types shared by all stories.

### Agents crate: how Claude is launched

- [X] T003 Write failing tests in `crates/delilah-agents/src/claude.rs` (`mod tests`, next to the existing command tests around lines 380–520):
  - `Permission::Plan` → `--permission-mode plan`;
  - `Permission::DontAsk` → `--permission-mode dontAsk`;
  - `Permission::Default` → no `--permission-mode`;
  - `plugin_dir: Some(p)` → `--plugin-dir p`, and absent when `None`;
  - `append_system_prompt: Some(s)` → `--append-system-prompt s`, also when `resume: true`;
  - `settings(…, Permission::DontAsk)` contains `permissions.allow` with exactly R3's list and `permissions.deny` `["Edit","Write","NotebookEdit"]`; other permissions add no `permissions` key.
- [X] T004 In `crates/delilah-agents/src/claude.rs`:
  - add `pub enum Permission { Default, Plan, DontAsk }`;
  - replace `Launch.read_only: bool` with `permission: Permission`;
  - add `plugin_dir: Option<&'a Path>` and `append_system_prompt: Option<&'a str>`;
  - emit the flags in `command()` (replacing the `if launch.read_only` block at ~line 110);
  - give `settings()` / `write_settings()` a `permission` parameter that adds the R3 block for `DontAsk`.

  Update every `Launch { … }` literal in the crate's tests. T003 then passes.
- [X] T005 [P] Add `pub fn write_plugin(dir: &Path, version: &str) -> std::io::Result<()>` to `crates/delilah-agents/src/claude.rs`:
  - it writes the two files from T002 via `include_str!("../plugin/…")` under `dir`;
  - `plugin.json`'s version becomes `version`;
  - it overwrites existing files (idempotent).

  Test: `write_plugin` into a temp dir gives a `plugin.json` that parses as JSON with name `delilah`, and a `SKILL.md` whose front-matter has `name: task-from-idea`.

### Core: the history event

- [X] T006 [P] In `crates/delilah-core/src/task.rs`:
  - Add a failing test: `Event::CreatedFromConversation { session, label }` applies to a `Queued` task, leaving status and phase unchanged, and is rejected on a `Running` task.
  - Then add the variant, serialized as `{"type":"created_from_conversation","session":…,"label":…}` (match the enum's existing serde tagging), and accept it in `Task::apply` only for `Queued`.

### Daemon: schema and store

- [ ] T007 Create `crates/delilah-daemon/migrations/0011_idea_conversations.sql` as in data-model.md:
  - `ALTER TABLE sessions ADD COLUMN conversation INTEGER NOT NULL DEFAULT 0;`;
  - `conversation_entries` with its index.
- [ ] T008 Write failing tests in `crates/delilah-daemon/src/store_sessions.rs` (`Store::in_memory()`):
  - a session created with `conversation: true` reads back `true`, and existing creation paths read `false`;
  - `add_entry` / `entries(session)` return entries in insertion order with their JSON bodies;
  - `update_entry(id, body)` replaces the body;
  - `delete_queued_entry` deletes a `user`/`queued` entry and refuses (returns false) any other;
  - deleting the session's project cascades to its entries.
- [ ] T009 In `crates/delilah-daemon/src/store_sessions.rs`:
  - add `conversation: bool` to `SessionRecord` (insert/select it, wherever the other columns are);
  - add `pub struct Entry { id: i64, at: DateTime<Utc>, kind: EntryKind, body: Value }` with `EntryKind { User, Claude, Card }`;
  - add the store methods `add_entry`, `entries`, `update_entry`, `delete_queued_entry` and `queued_entries(session)`.

  Fix every `SessionRecord { … }` literal (`run.rs:311`, `run.rs:521`, `hooks.rs:286`, `session_api.rs:113`, `session_api.rs:263`, tests) with `conversation: false`. T008 then passes.
- [ ] T010 Write failing tests in `crates/delilah-daemon/src/store.rs`:
  - `create_task_from_conversation(&task, session, label, at)` inserts the task and one `task_events` row with `CreatedFromConversation`, in one transaction;
  - `events(key)` returns it;
  - a duplicate key fails and leaves no event.

  Then implement it, next to `create_task` (~line 74) and reusing its insert.

### Daemon: plumbing

- [X] T011 [P] Add `Topic::Conversations` (path `/conversations`, doc comment as the others) to `crates/delilah-daemon/src/events.rs`, and to any topic list or serialisation there.
- [X] T012 Write the plugin at start-up:
  - in `crates/delilah-daemon/src/state.rs` (or `main.rs`, wherever `Config` and the dirs are prepared), call `delilah_agents::claude::write_plugin(&config.home.join("plugins/delilah"), env!("CARGO_PKG_VERSION"))`;
  - log and continue on error;
  - add `pub fn plugin_dir(&self) -> PathBuf` to `crates/delilah-daemon/src/config.rs`.

  Test: `plugin_dir()` is `<home>/plugins/delilah`.
- [ ] T013 Write failing tests in `crates/delilah-daemon/src/session_api.rs` (pattern: the existing tests around line 467):
  - `POST /api/v1/projects/{p}/sessions {"kind":"claude","conversation":true}` → 201, with `conversation: true`, `label: "Idea"` and `read_only: false`;
  - with `"kind":"shell"` → 400;
  - `POST /api/v1/tasks/{k}/sessions {"kind":"claude","conversation":true}` → 400 (only projects);
  - without `conversation`, it behaves as today.
- [ ] T014 In `crates/delilah-daemon/src/session_api.rs`:
  - add `conversation: Option<bool>` to `OpenSession`, with the validation from T013, and store it;
  - set the label to "Idea" and `read_only` to false for a conversation.

  In `crates/delilah-daemon/src/run.rs::launch_session` (~line 356), choose the flags per the plan's launch table:
  - `permission`: `DontAsk` for a conversation, `Plan` for a read-only Claude, `Default` otherwise;
  - `plugin_dir`: `Some(app.config.plugin_dir())` when the owner is a project (`project_of(&session.task).is_some()`);
  - `append_system_prompt`: `Some(IDEA_PROMPT)` for a conversation. Define it as a `const` in `run.rs`, one paragraph: "This is a Delilah idea conversation, shown in a chat view, not a terminal. Follow the task-from-idea skill from the first message. Ask with AskUserQuestion. Never use ExitPlanMode."

  Pass the permission to `write_settings`. T013 then passes, along with the existing session and run tests.
- [X] T015 [P] In `app/src/api.ts`:
  - `Session.conversation: boolean`;
  - `AskQuestion.options[].preview?: string`;
  - `InboxItem.ask.proposal?: { title: string; brief: string } | null`;
  - the `ConversationEntry` union (`user` / `claude` / `card`, fields as contracts/api.md);
  - `Conversation { entries; activity: string | null; needs_terminal: boolean }`;
  - `openProjectSession` accepts `conversation?: boolean`;
  - new client functions `conversation(id)`, `sendMessage(id, text)`, `removeMessage(id, entry)`, `stopSession(id)`;
  - `answerAsk` returns `{ task: string | null; error?: string }`;
  - the `conversations` topic, wherever topics are listed.

  Fix the compile errors in components with the new required field (`npx tsc --noEmit`).

**Checkpoint**: `cargo test -p delilah-core -p delilah-agents -p delilah-daemon` and `npx tsc --noEmit` pass. A conversation session can be opened through the API and launches with `dontAsk`, the plugin and the idea prompt.

---

## Phase 3: User Story 1: Tell Claude an idea and get a task in the backlog (P1) 🎯 MVP

**Goal**: "Nueva tarea con Claude" opens a conversation view with no terminal. Claude asks with cards and proposes a task, and "Crear" puts a `LIBRE-n` task with the brief in the backlog.

**Independent Test**: quickstart.md "By hand" 1–5, 9 and 10 in a preview.

### Proposal → task (daemon)

- [ ] T016 [US1] Write failing tests in `crates/delilah-daemon/src/asks.rs` (pattern: the test at ~line 340 that posts to the hook and answers):
  - `a_crear_answer_on_a_project_proposal_creates_a_backlog_task_with_the_brief`:
    - post the hook for owner `@web` with one question (`header: "Nueva tarea"`, `question: "Reintentos con backoff"`, options `Crear` (preview `"## Idea\n…"`), `Cambiar algo`, `Descartar`);
    - answer `Crear`;
    - expect: a `LIBRE-n` task in project `web`, `Queued`, with that title; notes starting with `> Brief escrito por Claude en la conversación «Idea»,` followed by the brief; a `CreatedFromConversation` event; the hook's `updatedInput.answers` value `"Crear — creada LIBRE-n en el backlog"`; the answer response `{task:"LIBRE-n"}`.
  - `a_task_sessions_card_with_the_same_shape_creates_nothing` (owner `PROJ-1`).
  - `a_cambiar_answer_creates_nothing`.
  - `a_second_answer_cannot_create_a_second_task` (second answer → 404, one task).
  - `an_empty_title_answers_with_the_reason_and_creates_nothing` (answer contains `no se pudo crear`, response `{task:null,error}`).
  - `a_card_without_preview_or_with_two_questions_is_not_a_proposal`.
- [ ] T017 [US1] Extract the body of `create_task` in `crates/delilah-daemon/src/api.rs` (lines 256–276) into `pub(crate) async fn build_task(app: &AppState, new: NewTask) -> Result<Task, ApiError>`, without storing it, so `create_task` calls it and then `store.create_task`. Existing api tests must still pass.
- [ ] T018 [US1] In `crates/delilah-daemon/src/asks.rs`:
  - add `proposal: Option<Proposal>` (`#[derive(Serialize)] pub struct Proposal { title: String, brief: String }`) to `Ask`, filled in `hook` by `fn proposal_of(owner: &str, questions: &Value) -> Option<Proposal>` with the data-model rules;
  - change `Outcome::Answered` handling so that `answer()`, when the ask has a proposal and its question's answer is `"Crear"`, does the following:
    - builds a `NewTask { free: true, key: "", title, project: Some(project), notes: Some(mark + brief), in_checkout: None, workflow: None }` with `build_task`;
    - stores it with `store.create_task_from_conversation(&task, session, label, now)` (label from the session record);
    - rewrites the answer value to `Crear — creada {key} en el backlog`, or `Crear — no se pudo crear: {error}`;
    - emits `Tasks`;
    - returns `{task}` / `{task:null,error}`.

  Other answers return `{task:null}`. T016 then passes.
- [ ] T019 [US1] Expose `proposal` and `options[].preview` through the inbox's ask (`crates/delilah-daemon/src/inbox.rs`, wherever `Ask` is mapped into the item), with a test that an inbox item for a proposal ask carries `proposal`.

### History, queue and stop (daemon)

- [ ] T020 [US1] Write failing tests in a new `crates/delilah-daemon/src/conversation.rs` (`mod tests`; reuse the hook-posting helpers from `hooks.rs` tests, and a fake live session as those tests do):
  - `GET /api/v1/sessions/{id}/conversation`: 404 for an unknown session, 409 for a non-conversation session, and the entries in order for a conversation;
  - `messages_sent_while_idle_are_typed_and_recorded_sent`;
  - `messages_sent_while_working_are_queued_and_typed_together_after_stop`: two queued entries; a `Stop` hook types `"a\n\nb"` once and marks both `sent`;
  - `a_queued_message_can_be_removed_but_a_sent_one_cannot` (204 / 409);
  - `stop_writes_escape_withdraws_pending_cards_and_flushes_the_queue`;
  - `a_prompt_typed_in_the_terminal_is_recorded_once`: `UserPromptSubmit` with text matching a `sent` entry → that entry becomes `delivered`, no new entry; non-matching text → a new `from: "terminal"` entry;
  - `stop_records_claudes_last_message`;
  - `post_tool_use_sets_activity_and_stop_clears_it`;
  - `a_permission_prompt_notification_sets_needs_terminal`;
  - `entries_are_recorded_only_for_conversation_sessions`;
  - `entries_survive_reopening_the_store_on_the_same_file` (temp-file store).
- [ ] T021 [US1] Add `activity: Mutex<Option<String>>` and `needs_terminal: AtomicBool` to `Live`, with getters and setters, in `crates/delilah-daemon/src/sessions.rs`.
- [ ] T022 [US1] Implement `crates/delilah-daemon/src/conversation.rs` and register it in `lib.rs`:
  - the handlers `get`, `send`, `remove`, `stop` (contracts/api.md);
  - `pub async fn on_hook(app, session, signal, hook)`, called from `hooks.rs` for conversation sessions: records user and Claude entries, sets activity and `needs_terminal`, and flushes the queue on `TurnEnded`;
  - `pub async fn flush_queue(app, live)`: joins with `\n\n`, uses `crate::actions::type_prompt`, marks the entries `sent`;
  - `fn activity_of(hook) -> Option<String>`: `Read` → "Leyendo {file_path}"; `Grep`/`Glob` → "Buscando {pattern}"; `Bash` → the command's first 40 characters; else the tool name.

  Add `Asks::release_session(&self, session: &str) -> Vec<String>` (the released ids) next to `release_task` in `asks.rs`. Add the routes in `api.rs` next to `/api/v1/sessions/{id}/reply` (line 91):
  - `GET /api/v1/sessions/{id}/conversation`;
  - `POST /api/v1/sessions/{id}/messages`;
  - `DELETE /api/v1/sessions/{id}/messages/{entry}`;
  - `POST /api/v1/sessions/{id}/stop`.

  Emit `Topic::Conversations` on every change. T020 then passes.
- [ ] T023 [US1] Record card entries in `crates/delilah-daemon/src/asks.rs` for conversation sessions:
  - `hook` adds a `card` entry (`pending`, questions, proposal);
  - `answer` updates it (`answered`, answer, task or error);
  - `release` and `release_session` update it to `terminal` or `withdrawn`.

  Add a test that each transition is reflected in `GET …/conversation`.
- [ ] T024 [US1] Call `conversation::on_hook` from `crates/delilah-daemon/src/hooks.rs::claude` when the session record has `conversation`, after the live state is set (~line 68). Keep all non-conversation behaviour identical; the existing hooks tests must pass.

### UI

- [ ] T025 [P] [US1] Write failing tests in `app/src/components/AskCard.test.tsx` (new; pattern: `DeleteTaskDialog.test.tsx`):
  - an option with `preview` shows its Markdown, and the first option's preview is shown before any pick;
  - an ask with `proposal` renders the title as a heading, the brief open, and the buttons "Crear", "Cambiar algo" and "Descartar";
  - clicking "Crear" calls `api.answerAsk` with `{ [question]: "Crear" }` and then shows "Creada LIBRE-7" as a link (mock `answerAsk` → `{task:"LIBRE-7"}`);
  - an `error` response shows it.
- [ ] T026 [US1] Implement T025 in `app/src/components/AskCard.tsx`:
  - render previews with the existing `Markdown` component, in a block with `max-height: 60vh; overflow: auto` (class in `app/src/styles.css`);
  - use the confirmation layout when `ask.proposal` is set;
  - add an `onOpenTask?: (key: string) => void` prop for the link.
- [ ] T027 [P] [US1] Write failing tests in `app/src/components/ConversationView.test.tsx` (mock `api`):
  - with no entries it shows the hint "Cuéntale tu idea…";
  - user, Claude (Markdown) and card entries render in order, and a pending card renders an `AskCard`;
  - a `queued` entry shows "en cola" and a remove button that calls `api.removeMessage`;
  - Enviar (and ⌘↵) calls `api.sendMessage` and clears the field; an empty field cannot send;
  - "Detener" shows only while `session.state === "working"`, and calls `api.stopSession`;
  - `activity` shows while working;
  - `needs_terminal` shows "Claude espera en la terminal" with a button that calls `onOpenTerminal`;
  - a session that is not live shows "Reabrir";
  - an answered card with `task` shows the key as a link.
- [ ] T028 [US1] Create `app/src/components/ConversationView.tsx`:
  - props `{ session: Session; onOpenTerminal: () => void; onOpenTask: (key) => void; onReopen: () => void }`;
  - it fetches `api.conversation(session.id)` and refetches on the `conversations` and `inbox` topics, using the same subscription hook other views use for topics;
  - it renders as in T027 and scrolls to the bottom on new entries.

  Add the styles to `app/src/styles.css`, reusing the existing tokens. T027 then passes.
- [ ] T029 [US1] In `app/src/components/TaskScreen.tsx`, for a project session with `conversation`:
  - render `ConversationView` instead of `TerminalPane` (around line 609);
  - add a header toggle "Ver terminal" / "Ver conversación", per session, in component state, defaulting to conversation;
  - hide the read-only/"Permitir cambios"/"Convertir en tarea" bar (lines 555–585) for conversations.

  Extend `app/src/components/TaskScreen.test.tsx`: a conversation session renders the view, not the terminal, and the toggle switches.
- [ ] T030 [P] [US1] Add the entry points:
  - in `app/src/components/ProjectView.tsx`, a "Nueva tarea con Claude" button next to "Preguntar a Claude" (line 31);
  - in `app/src/components/Sidebar.tsx`, the same action in the project's "+" (line 172 area; turn it into a small menu, or add a second icon button "Nueva tarea con Claude"), calling `api.openProjectSession(name, { kind: "claude", conversation: true })` and selecting the new session;
  - in the sidebar, list conversation sessions with an idea icon (`Icon.tsx`: add one if there is none).

  Extend `app/src/components/Sidebar.test.tsx` for the new action.
- [ ] T031 [US1] In `app/src/components/NewTaskDialog.tsx`:
  - add the third origin `"claude"` ("Con Claude") to the segmented control (lines 161–166). With it, show only the project picker and an optional textarea "Tu idea", and hide the key, title, notes and where fields;
  - on submit, call `api.openProjectSession(project.name, { kind: "claude", conversation: true, prompt: idea || undefined })`, close, and navigate to the session (the same callback the sidebar uses).

  Add a test file, or extend an existing one, to cover that "Con Claude" opens a conversation and that the Jira and Libre paths are unchanged.

**Checkpoint**: US1 works end to end in a preview (quickstart 1–5, 9, 10); all Rust and app tests pass.

---

## Phase 4: User Story 2: Start the task from its brief (P1)

**Goal**: the brief is the task's notes, editable in the backlog, and the agent starts from it.

**Independent Test**: quickstart.md "By hand" 6.

- [ ] T032 [US2] Write a regression test in `crates/delilah-daemon/src/run.rs` (`mod tests`):
  - create a backlog task with `build_task` + `create_task_from_conversation`;
  - `PATCH` its notes;
  - start it with `POST /api/v1/runs`;
  - assert that the written `notes.md` holds the edited brief and that the prompt points to it (the existing `point_to_notes` behaviour).

  If it fails, fix `run.rs` so that a started backlog task always passes its current notes.
- [ ] T033 [P] [US2] Show the history event:
  - wherever task events are listed in `app/src/components/TaskView.tsx` / `BacklogTaskView.tsx` (find the event-to-text mapping), render `created_from_conversation` as "Creada desde la conversación «{label}»";
  - add `"Creada desde la conversación «{label}»"` to `app/src/i18n/en.ts`;
  - add a test for the mapping.

**Checkpoint**: a task created from an idea can be edited and started, and its agent reads the brief.

---

## Phase 5: User Story 3: Create a task from a project session (P2)

**Goal**: a terminal project session can propose and create a task the same way, and keeps its terminal.

**Independent Test**: quickstart.md "By hand" 7 and 8.

- [ ] T034 [US3] Add daemon tests in `crates/delilah-daemon/src/asks.rs`:
  - `a_project_sessions_proposal_creates_a_task_and_records_no_entries` (a non-conversation `@web` session);
  - `a_project_session_after_allow_edits_can_still_create`;
  - `a_session_converted_into_a_task_cannot_create` (after "Convertir en tarea" the owner is the task, so its card is ordinary).

  Fix `asks.rs` if any fails.
- [ ] T035 [US3] Add a test in `crates/delilah-daemon/src/run.rs` or `session_api.rs` for the launch flags:
  - a read-only project session gets `--permission-mode plan` and `--plugin-dir`, and no `--append-system-prompt`;
  - a task's primary and extra sessions get no `--plugin-dir`.

  Assert on the `Launch` built, by extracting `fn launch_flags(session: &SessionRecord, config: &Config) -> (Permission, Option<PathBuf>, Option<&'static str>)` from `launch_session` if needed.
- [ ] T036 [P] [US3] Make sure the confirmation card renders where project sessions show asks: in `TaskScreen.tsx`, next to the terminal (line 614), and in `InboxView.tsx`. Pass `onOpenTask` so the created key links to the task, and extend `TaskScreen.test.tsx` with a proposal ask.

**Checkpoint**: US3 works in a preview, and task agents cannot create tasks (quickstart 8).

---

## Phase 6: User Story 4: Several ideas in one conversation (P3)

**Independent Test**: confirm two proposals in one conversation, and two backlog tasks with different keys and briefs appear.

- [ ] T037 [US4] Add a daemon test in `crates/delilah-daemon/src/asks.rs`, `two_proposals_in_one_conversation_create_two_tasks`: two proposal asks from the same conversation session, both answered `Crear`, give `LIBRE-n` and `LIBRE-n+1` with their own notes and two card entries each showing its key.

---

## Phase 7: Polish & cross-cutting

- [ ] T038 [P] Add every new UI string to `app/src/i18n/en.ts`, Spanish source → English. Examples: "Nueva tarea con Claude", "Con Claude", "Tu idea", "Cuéntale tu idea…", "en cola", "Detener", "Enviar", "Ver terminal", "Ver conversación", "Claude espera en la terminal", "Creada {key}", "Cambiar algo", "Descartar", "Crear".
- [ ] T039 [P] Docs:
  - `docs/status.md`: built, plus the pending items (streaming and a chat view for other sessions, deleting conversations);
  - `docs/architecture.md`: the conversation session, `conversation_entries`, `conversation.rs`, the plugin dir, the launch table, the `conversations` topic, the `Ask.proposal` flow;
  - `docs/decisions.md`: #26 "Tasks from ideas: the daemon creates them on the confirmation card" (R1, with the rejected CLI/curl/MCP), and #27 "Idea conversations: a chat view over the PTY fed by hooks; `dontAsk` instead of plan mode; a session-only plugin" (R2–R5, with `stream-json` left for later). Include the spike's findings;
  - `README.md`: "Nueva tarea con Claude" in the New tasks and Projects paragraphs.
- [ ] T040 Run `cargo test --workspace`, `cargo test --manifest-path app/src-tauri/Cargo.toml`, `cd app && npx tsc --noEmit && npx vitest run`, and fix any failure.
- [ ] T041 Go through quickstart.md "By hand" 1–11 in a preview (`scripts/preview.sh`, never 7420), and note the results in `specs/004-task-from-idea/progress.md` if the orchestrator keeps one.

---

## Dependencies & execution order

- **Phase 1**: T001 (spike) gates Phase 2. T002 can be written in parallel with the spike.
- **Phase 2**: blocks every story. Internal order:
  - T003→T004→T005;
  - T006 [P];
  - T007→T008→T009→T010;
  - T011 [P];
  - T012 after T005;
  - T013→T014 after T004 and T009;
  - T015 [P] at any point.
- **US1 (Phase 3)**:
  - daemon T016→T017→T018→T019, then T020→T021→T022→T023→T024;
  - UI T025→T026 and T027→T028 can run in parallel with the daemon work once T015 is done;
  - T029–T031 after T026/T028.
- **US2 (Phase 4)**: after T018. T033 [P] after T006.
- **US3 (Phase 5)**: after T014 and T018. T036 after T026.
- **US4 (Phase 6)**: after T023.
- **Polish**: last. T038 and T039 [P].

### Parallel examples

- Phase 2: T005, T006, T011 and T015 touch different files and can run together after T004.
- US1: the daemon chain (`asks.rs`, `conversation.rs`, `hooks.rs`) and the UI chain (`AskCard.tsx`, `ConversationView.tsx`) share no files.
- US3: T036 (`TaskScreen.tsx`) runs next to T034/T035 (Rust).

## Implementation strategy

1. **MVP = Phases 1–3 (US1)**. It is usable alone: ideas become backlog tasks with a brief, from the conversation view. Stop and check quickstart 1–5 in a preview.
2. Add **US2** (mostly a regression test and the history label): the brief carries into the run.
3. Add **US3** (tests plus the card in terminal project sessions).
4. **US4** is one test.
5. Polish: strings, docs and a full run of the quickstart.
