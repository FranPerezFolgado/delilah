# Architecture

How Delilah is built today. User-facing behaviour is in the [README](../README.md).
The reasons behind these choices are in [decisions.md](decisions.md), and what is
done or pending is in [status.md](status.md).

## The pieces

```
 Delilah.app (Tauri shell + React UI)   delilah (CLI)        (later: mobile web)
            │  HTTP + WebSocket              │ HTTP + WS
            └──────────────┬─────────────────┘
                           ▼
                delilahd  127.0.0.1:7420  ── SQLite  $DELILAH_HOME/delilah.db
                  │  PTYs                     ── files  $DELILAH_HOME/{workspaces,tasks}/
                  ▼
     claude (one process per session) ── hooks / status line ──► POST /hooks/claude…
```

- **`delilahd`** is the only process that does anything: it launches agents in
  PTYs, keeps state in SQLite, and resumes paused tasks. It keeps running with
  the window closed; launchd owns it in the installed app.
- **API-first.** The UI, the CLI and any future client use only the daemon's
  HTTP and WebSocket API. Nothing else reads the database, and the Tauri shell
  adds only desktop extras (window, menu, LaunchAgent, notifications, Dock
  badge), never Tauri commands for data. Swapping the shell for Electron would not
  touch the UI.
- **Loopback only.** The API has no authentication, so:
  - the daemon binds to a loopback address (`DELILAH_ADDR` is refused otherwise);
  - a browser `Origin` other than the app's is refused by a guard and by CORS;
  - the `Host` must be a loopback name, which stops DNS rebinding;
  - the app origins are `tauri://localhost`, `http://tauri.localhost`, the dev
    server `http://localhost:1420`, and `DELILAH_UI_ORIGIN` for a preview.

## Crates and folders

| Path | What it is |
|---|---|
| `crates/delilah-core` | Task model and state machine, SpecKit spec parsing (`spec.rs`), project kinds. Pure, no I/O. |
| `crates/delilah-agents` | The Claude Code adapter: launch args, the settings file with hooks, the status line script, hook classification (`Signal`), PTY (`pty.rs`), clean environment with the login shell's PATH (`env.rs`), usage-limit detection (`limits.rs`), the read-only submodule suggestion (`suggest.rs`). |
| `crates/delilah-git` | Git through the CLI: worktrees, selective submodule init, branches, checkout mode, archiving (`workspace.rs`), what a branch changed (`changes.rs`). |
| `crates/delilah-integrations` | Jira: Cloud (REST v3, ADF→Markdown), Server/Data Center (REST v2) and built-in sample data. |
| `crates/delilah-daemon` | `delilahd`: API, hook server, scheduler, spec watcher, store. |
| `crates/delilah-cli` | `delilah`: a client of the API (`DELILAH_URL`, default 127.0.0.1:7420). |
| `app/` | React + TypeScript UI (Vite), CodeMirror 6 editor, xterm.js terminals. |
| `app/src-tauri` | The Tauri 2 shell (outside the Cargo workspace): `main.rs` (window, menu), `daemon.rs` (LaunchAgent). |
| `scripts/` | Test fixtures (`make-fixture-parent.sh`), the dev daemon (`dev-daemon.sh`), previews (`preview.sh`), the bundled daemon build (`build-sidecar.sh`). |
| `specs/`, `.specify/` | SpecKit's setup and the specs of features built with it. |
| `docs/` | These docs, spikes (`docs/spikes/`), and local-only session notes (`docs/sessions/`, git-excluded). |

### Daemon modules (`crates/delilah-daemon/src`)

| Module | Role |
|---|---|
| `api.rs` | Router, origin/Host guard, CORS, task CRUD. |
| `run.rs` | `POST /api/v1/runs`: creates or starts a task, builds the workspace, launches the primary session, accepts the folder-trust dialog. |
| `session_api.rs`, `sessions.rs`, `store_sessions.rs` | Extra Claude sessions and shells. Live sessions are kept in memory and their records in SQLite. Also project sessions (`@project`). |
| `terminal.rs` | WebSocket terminal: scrollback first, then live bytes; text frames for `resize`. |
| `hooks.rs` | `/hooks/claude` and `/hooks/claude/statusline`. Each hook is classified into a `Signal`, which sets the session's live state, the inbox attention, the task transition and the phase. |
| `asks.rs` | AskUserQuestion: the PreToolUse hook posts here and waits up to a day. The answer goes back as `updatedInput.answers`. |
| `proposals.rs` | A card headed `Nueva tarea` in a project's conversation is a task proposal: answering `Crear` creates the backlog task (brief as notes, `CreatedFromConversation` event) before Claude is told. Owns the protocol strings. Called by `asks::answer` through `on_answer`. |
| `conversation.rs` | An idea conversation's history (`conversation_entries`): your messages (sent now, or queued while Claude works and flushed when its turn ends), Claude's closing messages, and cards (`Card`, outcome pending / answered / terminal / withdrawn). Endpoints `GET /sessions/{id}/conversation`, `POST /sessions/{id}/messages`, `DELETE /sessions/{id}/messages/{entry}`, `POST /sessions/{id}/stop`. The live activity and "needs the terminal" flag sit on `Live` (`ConvoState`) and emit `conversations` once per change. |
| `inbox.rs` | `GET /api/v1/inbox` (everything that waits on the user) and replies typed into a session. |
| `phases.rs` | The phase: set by the user, moved forward by spec files and agent signals, and review hints. |
| `specs.rs` | A watcher (every 5 s) that reads each task's `specs/<feature>/` and records phase changes. |
| `scheduler.rs` | Runs every 30 s: resumes tasks paused by a usage limit once the window resets. |
| `actions.rs` | Resume, done, archive, delete, and the usage report. |
| `transitions.rs` | The one way to change a task's status, so every change emits events and notifications. |
| `changes.rs`, `files.rs` | Cambios (diffs) and the editor's file API, confined to the workspace and checked by file version. |
| `projects.rs`, `store_projects.rs` | Registered projects (`parent` or `repo`), submodules, and the submodule suggestion. |
| `jira.rs`, `secrets.rs` | The Jira connection. The token lives in the Keychain; tests keep it in memory. |
| `events.rs` | `GET /api/v1/events`: a WebSocket that only announces topics. |
| `preferences.rs`, `store_settings.rs`, `lang.rs` | Preferences (`pref:<key>`), settings, and the language the daemon writes in. |
| `notify.rs`, `logs.rs`, `owners.rs`, `config.rs`, `state.rs`, `store*.rs` | Notifications (osascript), the in-memory log tail, owner resolution, environment config, `AppState`, SQLite access. |

## State on disk

`DELILAH_HOME` defaults to `~/.delilah`. The dev daemon uses `fixtures/dev-home`,
and a preview uses `fixtures/preview-home-N`.

| Path | Contents |
|---|---|
| `delilah.db` | SQLite (sqlx). The migrations are in `crates/delilah-daemon/migrations/NNNN_*.sql`, and a shipped one is never edited. |
| `workspaces/<KEY>/` | Worktree-mode workspaces. Checkout-mode tasks work in the project's own folder. |
| `tasks/<KEY>/` | Per-task files kept out of the repo: the Claude settings file with Delilah's hooks, the status line script, `ticket.md`, `hooks.jsonl` (raw hook log), and session logs. `tasks/@<project>/` holds the same for project sessions. |

**Tables:**
- `tasks`: key, title, project, workflow, phase, status JSON, notes, `in_checkout`.
- `task_events`: the history.
- `workspaces`: `task_key`, parent, path, branch, submodules JSON, kind, `previous`. For checkout mode, `previous` is JSON with the branches to go back to.
- `sessions`: id, owner, kind, label, primary, attention.
- `projects`
- `settings`

## Tasks

`TaskKey::parse` allows letters, digits, `-` and `_`, up to 64 characters. The key
names folders and goes into hook commands. Free tasks are named `LIBRE-n`.

**Status** (`delilah-core::task`) is a state machine driven by `Event`s:

| From | Event | To |
|---|---|---|
| Queued (= backlog) | Start | PreparingWorkspace |
| PreparingWorkspace | WorkspaceReady / StartAborted | Running / Queued |
| Running | AgentAsked | WaitingForUser |
| WaitingForUser | UserAnswered | Running |
| Running, WaitingForUser | LimitReached | PausedByLimit { resume_at } |
| PausedByLimit | ResetDue (after `resume_at`) / ManualResume | Running |
| Running | AgentFinished | Done |
| Running, WaitingForUser, PausedByLimit | MarkDone | Done |
| any non-terminal | Fail | Failed |

**The phase** (`specify → plan → implement → review`) is separate from the status
and only changes while the task is under way (see `phases.rs`):

- **The user** sets any phase: through the phase bar, with `POST /api/v1/tasks/{key}/phase`, or by asking for a review.
- **SpecKit files** (`specs.rs`) only move it forward:
  - no `plan.md` yet: specify;
  - `plan.md`: plan;
  - unchecked items in `tasks.md`: implement;
  - every item checked: review.
- **Agent signals** (`phases::signal_phase`) only move it forward:
  - Edit, Write, MultiEdit or NotebookEdit outside `/specs/` and `/.specify/`: implement;
  - a Bash command with `gh pr create` or `git push`: review.
- **A review hint**: when the primary session's turn ends with commits on the branch and the phase is below review, the inbox shows `suggest-review`.

## Sessions and agents

- **Owners.** A session belongs to an owner: a task key, or `@<project>` for a
  project session working in the project's checkout. `owners::workspace` resolves
  the folder for either.
- **Kinds.** The kinds are the primary Claude (its turns drive the task's status),
  extra Claudes, and login shells. Each runs in a PTY owned by the daemon; its
  scrollback is kept and logged to `tasks/<KEY>/`.
- **The `claude` command line:**
  - `--session-id <uuid>`, or `--resume <uuid>` when reopening, only if the conversation exists;
  - `--settings tasks/<KEY>/…json`;
  - `--add-dir` when needed;
  - `--remote-control <name>` when that preference is on;
  - `--permission-mode plan` for read-only project sessions.
- **Clean environment.** Claude runs with a clean environment plus the login
  shell's PATH (see `env.rs`). A Claude spawned with another Claude session's
  variables does not save its transcript.
- **Hooks.** They live only in that settings file, never in the repo or
  `~/.claude`:
  - `SessionStart`, `UserPromptSubmit`, `PostToolUse`, `Notification`, `Stop`, `StopFailure` and `SessionEnd` each `curl` their JSON to `/hooks/claude?task=…&session=…`;
  - a `PreToolUse` hook on AskUserQuestion waits on `/hooks/claude/ask`.
- **Status line.** The user's own status line from `~/.claude/settings.json`
  still draws: Delilah's script forwards Claude's JSON to `/hooks/claude/statusline`
  (which reads the usage windows) and then runs the user's command.
- **Folder trust.** A new workspace shows Claude's folder-trust dialog once.
  `run.rs` waits for it to settle, checks that "Yes" is selected and sends Enter.

**From hook to effect** (`hooks.rs`):

| Signal (hook) | Effect |
|---|---|
| `PromptSubmitted` (UserPromptSubmit), `Working` (PostToolUse) | session working; primary → `UserAnswered`; drops the review hint; phase signals |
| `NeedsUser` (Notification) | inbox attention (permission or idle); primary → `AgentAsked` |
| `TurnEnded` (Stop) | inbox question with the last message; primary → `AgentAsked`; maybe review hint |
| `TurnFailed` (StopFailure) | inbox error, or a usage limit |
| a turn ending while a usage window is at 100 % | `LimitReached` (any session: the subscription is shared) |

**Usage limits** (`limits.rs`, [spike](spikes/2026-10-19-claude-pty.md)):
1. The latest `rate_limits` of each session comes from the status line.
2. A turn that ends with `five_hour` or `seven_day` at 100 % pauses the task until the latest `resets_at` among the full windows.
3. The scheduler resumes it and sends the "carry on" prompt in the current language.

## Workspaces

- **Worktree mode** (in `workspaces/<KEY>/`):
  - **Parent projects:** a worktree of the parent, with only the chosen submodules initialised, all on the same branch `feature/<KEY>`.
  - **Repo projects:** a plain worktree.
  - Several tasks can run in parallel.
- **Checkout mode** (the default for new tasks; `use_checkout` / `leave_checkout`):
  - The task works on a branch in the project's own folder, or in the chosen submodules of a parent.
  - The checkout must be clean to start, and only one active task may hold it.
  - A finished holder is released when a new task takes it.
  - The branches to go back to are stored in `workspaces.previous`.
- **Archive** (`actions::archive`):
  - **Checkout mode:** goes back to the previous branches. It never refuses: uncommitted files stay, and a folder that cannot switch is logged and left on the task's branch.
  - **Worktree mode:** refuses while there are uncommitted changes or commits on no remote, unless `?force=true`. It then runs `git worktree remove --force`.
- **Delete** (`DELETE /api/v1/tasks/{key}`, `delilah rm`): stops the task's agents, releases the workspace (the branch stays) and frees the key. The same force rule applies.

## Live updates

The UI does not poll:
- `GET /api/v1/events` sends topics only: `tasks`, `sessions`, `inbox`, `specs`, `usage`, `projects`, or `resync`.
- The UI refetches that one endpoint (`app/src/live.ts`).
- On reconnect it refetches everything, and a full refresh runs every 30 s as a safety net.

## The UI (`app/src`)

- **Core files:**
  - `App.tsx`: layout, routing between views, shortcuts.
  - `api.ts`: every endpoint and type, mirroring the daemon's serde structs. **Keep the two in sync.**
  - `live.ts`: the events socket.
  - `phases.ts`: board columns.
  - `alerts.ts`: Dock badge and notifications while the app is open.
  - `platform.ts`: the only Tauri imports, all dynamic, so the UI runs in a plain browser.
- **Components:**
  - `Sidebar`, `InboxView` (Bandeja), `BoardView`, `TaskScreen` (tabs: task, sessions, Cambios, files) and `TaskView`;
  - `SpecPanel`, `ChangesView`, `FileTree`, `TerminalPane` (xterm.js), `JiraView`, `ProjectsView`, `ProjectView`;
  - `NewTaskDialog`, `BacklogTaskView`, `SettingsView`, `LogsView`, `QuickOpen` (⌘P), `AskCard`, `Markdown`.
- **Editor:** `editor/` holds CodeMirror 6 and per-task tab state, kept in localStorage.
- **i18n:**
  - Texts are Spanish in the code, wrapped in `t()` / `tn()`, and English lives in `i18n/en.ts`, keyed by the Spanish text.
  - **Every new text needs an `en.ts` entry**; dev builds warn in the console.
  - The daemon picks its own texts with `lang().pick(es, en)`.
