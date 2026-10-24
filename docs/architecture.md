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
| `crates/delilah-core` | Task model and state machine, SpecKit spec parsing (`spec.rs`), project kinds and per-project settings (`project.rs`: `ProjectSettings`, validation, the `{ticket}`/`{slug}` branch template), branch-name rules (`branch.rs`: `slug`, `title_slug`, `compose`, `custom`, `fallback`). Pure, no I/O. |
| `crates/delilah-agents` | The Claude Code adapter: launch args, the settings file with hooks, the status line script, hook classification (`Signal`), PTY (`pty.rs`), clean environment with the login shell's PATH (`env.rs`), usage-limit detection (`limits.rs`), the read-only submodule suggestion (`suggest.rs`), the branch-name proposal (`branch_name.rs`) and the headless one-shot both share (`oneshot.rs`). |
| `crates/delilah-git` | Git through the CLI: worktrees, selective submodule init, branches, checkout mode, archiving, and branch names that are free (`branch_taken`, `free_branch`) (`workspace.rs`), the fresh base of a task branch (`base.rs`), what a branch changed (`changes.rs`), shared entries copied or linked into a worktree (`share.rs`). Every git call runs with `core.hooksPath=/dev/null` and `core.fsmonitor=false`, and never prompt (`GIT_TERMINAL_PROMPT=0`, `GIT_ASKPASS=true`, `SSH_ASKPASS_REQUIRE=never`, `credential.interactive=never`, no stdin; `command.rs::command`), since agents write those repos and none of their hooks may run in the daemon; `git()` runs to the end, `git_with_timeout` gives up at a `Deadline`. |
| `crates/delilah-integrations` | Jira: Cloud (REST v3, ADF→Markdown), Server/Data Center (REST v2) and built-in sample data. |
| `crates/delilah-daemon` | `delilahd`: API, hook server, scheduler, spec watcher, store. |
| `crates/delilah-cli` | `delilah`: a client of the API (`DELILAH_URL`, default 127.0.0.1:7420). |
| `crates/delilah-e2e` | The end-to-end test harness (library, `e2e` feature): `Env` (an isolated daemon), fixtures, fake Jira, the `fake-claude` binary (runs a JSON scenario and the settings' hooks) and `e2e-env` (starts an Env for the UI tests and for debugging). |
| `app/` | React + TypeScript UI (Vite), CodeMirror 6 editor, xterm.js terminals. `app/e2e/` has the Playwright specs. |
| `app/src-tauri` | The Tauri 2 shell (outside the Cargo workspace): `main.rs` (window, menu), `daemon.rs` (LaunchAgent). |
| `.github/workflows/release.yml`, `Makefile` | The release: the workflow that builds, signs and publishes a `v*.*.*` tag, and `make release` / `make release-dry-run`, which run `scripts/release/*.mjs` and `scripts/release.sh`. |
| `scripts/` | Test fixtures (`make-fixture-parent.sh`), the dev daemon (`dev-daemon.sh`), previews (`preview.sh`), the bundled daemon build (`build-sidecar.sh`), the end-to-end tests (`e2e.sh`). |
| `specs/`, `.specify/` | SpecKit's setup and the specs of features built with it. |
| `docs/` | These docs, spikes (`docs/spikes/`), and local-only session notes (`docs/sessions/`, git-excluded). |

### Daemon modules (`crates/delilah-daemon/src`)

| Module | Role |
|---|---|
| `api.rs` | Router, origin/Host guard, CORS, task CRUD. |
| `branch_name.rs` | `POST /api/v1/projects/{name}/branch-name` and `propose`: Claude (10 s for the endpoint, 5 s when `start_run` asks), then the title slug, then the key; `unique` adds the `-N` suffix. It never creates anything. |
| `run.rs` | `POST /api/v1/runs`: creates or starts a task, names its branch (`start_run`: the request's `branch` normalised, or `propose`, then `unique`), builds the workspace, launches the primary session, accepts the folder-trust dialog. |
| `session_api.rs`, `sessions.rs`, `store_sessions.rs` | Extra Claude sessions and shells. Live sessions are kept in memory and their records in SQLite. Also project sessions (`@project`). |
| `terminal.rs` | WebSocket terminal: scrollback first, then live bytes; text frames for `resize`. |
| `hooks.rs` | `/hooks/claude` and `/hooks/claude/statusline`. Each hook is classified into a `Signal`, which sets the session's live state, the inbox attention, the task transition and the phase. |
| `asks.rs` | AskUserQuestion: the PreToolUse hook posts here and waits up to a day. The answer goes back as `updatedInput.answers`. |
| `proposals.rs` | A card headed `Nueva tarea` in a project's conversation is a task proposal: answering `Crear` creates the backlog task (brief as notes, `CreatedFromConversation` event) before Claude is told. Owns the protocol strings. Called by `asks::answer` through `on_answer`. |
| `conversation.rs` | An idea conversation's history (`conversation_entries`, one row per message or card): your messages (sent now, or queued while Claude works and flushed when its turn ends), Claude's closing messages, and cards (`Card`, outcome pending / answered / terminal / withdrawn). Endpoints `GET /sessions/{id}/conversation`, `POST /sessions/{id}/messages`, `DELETE /sessions/{id}/messages/{entry}`, `POST /sessions/{id}/stop`. The live activity and "needs the terminal" flag sit on `Live` (`ConvoState`) and emit the `conversations` topic once per change. |
| `autopilot/` | The policy over saving mode ([#38](decisions.md)): `mod.rs` (`shape`, the state helpers, `update` as a compare-and-set, `Finishing`), `notes.rs` (what each session is told, the outcome path), `outcome.rs` (reads the review's file), `pause.rs` (pause, stall and nudge, resume, advance), `ask.rs` (recommended answers), `finish.rs` (preview, draft PRs, recovery), `api.rs` (`POST /tasks/{key}/autopilot`). |
| `preview.rs`, `preview_api.rs` | A task's preview process, owned by the daemon, and its routes (`GET`/`POST`/`DELETE /tasks/{key}/preview`). |
| `inbox.rs` | `GET /api/v1/inbox` (everything that waits on the user) and replies typed into a session. |
| `phases.rs` | The phase: set by the user, moved forward by spec files and agent signals, and review hints. |
| `specs.rs` | A watcher (every 5 s) that reads each task's `specs/<feature>/` and records phase changes. |
| `scheduler.rs` | Runs every 30 s: resumes tasks paused by a usage limit once the window resets, and runs the branching `recover`. |
| `branching.rs` | The branch and title of a free task, from its first analysis (see Workspaces). A per-task lock (`BranchLocks` in `AppState`). Also `POST /hooks/claude/guard` and `POST /api/v1/tasks/{key}/branch`. |
| `actions.rs` | Resume, done, archive, delete, and the usage report. |
| `archive.rs` | How a task is archived, shared by `actions::archive` and `pr_loop::merge`: stop and await the sessions, release the workspace, delete the verified local branches, finish an `archive-partial`. The policy (what must hold first) stays with the caller. |
| `transitions.rs` | The one way to change a task's status, so every change emits events and notifications. |
| `changes.rs`, `files.rs` | Cambios (diffs) and the editor's file API, confined to the workspace and checked by file version. |
| `projects.rs`, `store_projects.rs` | Registered projects (`parent` or `repo`), their settings (validation, `FieldError` with a stable `code`, localised by the daemon), submodules (`GET …/submodules` hides `never` ones unless `?all=true`), and the submodule suggestion. |
| `setup.rs` | A project's setup command: runs `$SHELL -lc` in the worktree as a `Setup` session (10 min), cancel/retry/skip, recovery of interrupted setups at start. |
| `gh/` | The user's `gh` behind the `Gh` trait (`mod.rs`: `CliGh`, access problems, repo/user/PR calls), the GraphQL query and its parser (`graphql.rs`) and a canned `FakeGh` for tests (`fake.rs`). `DELILAH_GH_BIN` (absolute path, checked in `Config` at start) swaps the binary. |

`DELILAH_CLAUDE_BIN` (`Config.claude_bin`, default `claude`) swaps the binary of the headless Claude calls: the branch name and the submodule suggestion. Tests and previews point it at a fake script.
| `pr_types.rs`, `prs.rs` | PR data types (snapshot, checks, reviews, comments) and the pure logic: `ci_outcome`, `pick`, `summary`, `diff`, back-off. |
| `pr_loop/` | The I/O of the PR loop: `mod.rs` (the 30 s loop `spawn`, `nudge`, `check_task`, detection), `access.rs` (access problems, notified once per project), `notices.rs` (turns a snapshot diff into notices and notifications), `merge.rs` (`on_merged`, see Workspaces). |
| `pr_api/`, `store_prs.rs` | The PR routes and the SQLite access for PRs and their notices. `mod.rs` (list, summaries, refresh, link, unlink, dismiss), `dto.rs` (the JSON types), `open.rs` (proposal and open; `open` links the new PR straight to its repo with `pr_loop::link_known`; a parent's proposal defaults to its first free submodule that has commits), `forward.rs` (sending feedback to the agent). |
| `jira.rs`, `secrets.rs` | The Jira connection. The token lives in the Keychain; tests keep it in memory. |
| `upgrade.rs` | Before `Store::open`: if `version` differs from the running version and the DB exists, `VACUUM INTO backups/delilah-<prev>.db` (`unknown` with no marker; never overwrites; keeps 3 by mtime; failures only warn), then writes the marker after the open. |
| `events.rs` | `GET /api/v1/events`: a WebSocket that only announces topics. |
| `preferences.rs`, `store_settings.rs`, `lang.rs` | Preferences (`pref:<key>`), settings, and the language the daemon writes in. |
| `notify.rs`, `logs.rs`, `owners.rs`, `config.rs`, `state.rs`, `store*.rs` | Notifications (osascript), the in-memory log tail, owner resolution, environment config, `AppState`, SQLite access. |

## State on disk

`DELILAH_HOME` defaults to `~/.delilah`. The dev daemon uses `fixtures/dev-home`,
and a preview uses `fixtures/preview-home-N`.

Four environment variables exist only for the end-to-end tests (read in `Config::from_env`; unset, the daemon behaves as usual; any that is on is logged as a warning at start; details in `specs/005-e2e-tests/contracts/daemon-env.md`):
- `DELILAH_AGENT_BIN`: absolute path of an executable launched instead of `claude` for every session (an invalid value stops the daemon at start);
- `DELILAH_SECRETS=memory`: secrets in memory instead of the Keychain;
- `DELILAH_JIRA_ALLOW_HTTP_LOOPBACK=1`: accepts a Jira site over `http://` on 127.0.0.1 or localhost;
- `DELILAH_PR_POLL_SECS`: the PR loop's tick in seconds (default 30).

| Path | Contents |
|---|---|
| `delilah.db` | SQLite (sqlx). The migrations are in `crates/delilah-daemon/migrations/NNNN_*.sql`, and a shipped one is never edited. |
| `workspaces/<KEY>/` | Worktree-mode workspaces. Checkout-mode tasks work in the project's own folder. |
| `plugins/delilah/` | The session-only plugin (`skills/task-from-idea/SKILL.md`), rewritten at every daemon start and passed with `--plugin-dir`. Never installed in the user's Claude. |
| `version` | The version of the daemon that last opened the database. Written after the open succeeds; the next start compares it with its own. |
| `backups/` | `delilah-<version>.db`: a copy of the database made before migrating after a version change (3 newest kept). |
| `tasks/<KEY>/` | Per-task files kept out of the repo: the Claude settings file with Delilah's hooks, the status line script, `ticket.md`, `hooks.jsonl` (raw hook log), and session logs. `tasks/@<project>/` holds the same for project sessions. |

**Tables:**
- `tasks`: key, title, project, workflow, phase, status JSON, notes, `in_checkout`, `title_source` (`initial` / `spec` / `manual`; only `initial` is renamed automatically), `previous_title` (the title the automatic rename replaced) and `branch_error` (why the branch could not be made).
- `task_events`: the history.
- `workspaces`: `task_key`, parent, path, branch, submodules JSON, kind, `previous`, `spec_dirs_at_start` (JSON `label:name` list of the spec folders present at start, taken after the shared entries and extended when the setup ends), `started_branchless` (1 when the task started without a branch: only those get the branch steps; any free task with a baseline is still retitled). `branch` is `''` while a free task has no branch yet. For checkout mode, `previous` is JSON with the branches to go back to (the parent's `.` included).
- `sessions`: id, owner, kind (`claude`, `shell`, `setup`), label, primary, attention, `conversation` (1 for an idea conversation).
- `conversation_entries`: session, time, kind (`user` / `claude` / `card`) and a JSON body.
- `pull_requests`: one PR per (task, repo), with host, `owner/name`, number, URL, how it was linked (`detected`, `opened`, `pasted`), the last snapshot as JSON, the watermarks already notified (failed head, review and comment ids), `checked_at` and the last problem (`error`, `error_kind`).
- `pr_unlinked`: URLs the user unlinked, never detected again for that task.
- `pr_notices`: what the Bandeja shows for a task's PRs (`ci-failed`, `changes-requested`, `comments`, `archive-blocked`, `archive-partial`) and for its start (`base-stale`, `store_prs::KIND_BASE_STALE`), unique per (task, repo, kind, cause) and resolved rather than deleted. Unlinking or replacing a PR resolves that repo's notices except `base-stale`. All three cascade when the task is deleted.
- `projects`: name, path, kind, `settings` (JSON, migration 0016; `deny_unknown_fields`; an unreadable value falls back to the defaults with a warning) and `preview_command`.
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
| PreparingWorkspace | SetupFailed | SetupFailed { reason } |
| SetupFailed | RetrySetup / SkipSetup | PreparingWorkspace |
| any non-terminal | Fail | Failed |

`SetupFailed` is not terminal (the task can be retried, skipped or left alone) but can be archived.

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
  extra Claudes, login shells, and `SessionKind::Setup`: the read-only session of a project's setup command (no input, its log is the full output). Each runs in a PTY owned by the daemon; its
  scrollback is kept and logged to `tasks/<KEY>/`.
- **The `claude` command line:**
  - `--session-id <uuid>`, or `--resume <uuid>` when reopening, only if the conversation exists;
  - `--settings tasks/<KEY>/…json`;
  - `--add-dir` when needed;
  - `--remote-control <name>` when that preference is on;
  - the mode and plugin by kind (`run.rs`, `launch_flags`), below.

  | Session | Mode | Plugin dir | Other |
  |---|---|---|---|
  | Idea conversation | `dontAsk` ([#34](decisions.md)) | yes | idea system prompt, `--setting-sources user`, `--strict-mcp-config` |
  | Read-only project session | `plan` | yes | |
  | Other project session | default | yes | |
  | Task session (primary or extra) | default | no | |
- **Clean environment.** Claude runs with a clean environment plus the login
  shell's PATH (see `env.rs`). A Claude spawned with another Claude session's
  variables does not save its transcript.
- **Hooks.** They live only in that settings file, never in the repo or
  `~/.claude`:
  - `SessionStart`, `UserPromptSubmit`, `PostToolUse`, `Notification`, `Stop`, `StopFailure` and `SessionEnd` each `curl` their JSON to `/hooks/claude?task=…&session=…`;
  - a `PreToolUse` hook on AskUserQuestion waits on `/hooks/claude/ask` (on autopilot it may be answered inside the hook, see below).
- **Status line.** The user's own status line from `~/.claude/settings.json`
  still draws: Delilah's script forwards Claude's JSON to `/hooks/claude/statusline`
  (which reads the usage windows) and then runs the user's command.
- **Folder trust.** A new workspace shows Claude's folder-trust dialog once.
  `run.rs` waits for it to settle, checks that "Yes" is selected and sends Enter.

**The idea conversation** ([#35](decisions.md), [#36](decisions.md)): a project session with `conversation = 1`. It is still a Claude in a PTY, but the UI shows `conversation_entries` instead of the terminal (which stays reachable).
- Hooks feed the history: `UserPromptSubmit` and `Stop` record your messages and Claude's closing message; the AskUserQuestion ask becomes a `card`.
- A card has an outcome: pending, answered, terminal (answered in the terminal) or withdrawn.
- A card whose question is no longer waiting is withdrawn: when its hook connection drops, and for every pending card at daemon start-up (asks live only in memory).
- **`Ask.proposal` flow:** a card headed `Nueva tarea` is a proposal. When you answer `Crear`, `asks::answer` calls `proposals.rs` first: it creates the backlog task (brief from the card's preview as notes, event `created-from-conversation`), and only then returns the answer to Claude, with the new key.
- Writing while Claude works, or while a card waits, queues the message; it is sent when the turn ends. A flush claims the queued entries in one `UPDATE … RETURNING`, so two flushes never type the same message and a claimed one can no longer be removed. "Detener" writes ESC: Claude fires no `Stop` after ESC, so the daemon ends the turn itself and flushes the queue.

**From hook to effect** (`hooks.rs`):

| Signal (hook) | Effect |
|---|---|
| `PromptSubmitted` (UserPromptSubmit), `Working` (PostToolUse) | session working; primary → `UserAnswered`; drops the review hint; phase signals |
| `NeedsUser` (Notification) | inbox attention (permission or idle); primary → `AgentAsked`; a permission prompt of the primary session pauses a running autopilot (`autopilot::on_attention`) |
| `TurnEnded` (Stop) | inbox question with the last message; primary → `AgentAsked`; maybe review hint; in saving mode, a phase boundary offers or starts the next session, and a `saving` handoff goes on to `starting`; on autopilot, see below |
| AskUserQuestion (PreToolUse, `/hooks/claude/ask`) | the card or inbox question and `AgentAsked`; on a running autopilot, from its primary step session, every question with exactly one "(Recommended)" option is answered at once (history `autopilot-answered`, no inbox item), else autopilot pauses with the question's text and the ask goes on as usual |
| `TurnFailed` (StopFailure) | inbox error, or a usage limit |
| a turn ending while a usage window is at 100 % | `LimitReached` (any session: the subscription is shared) |

**Usage limits** (`limits.rs`, [spike](spikes/2026-10-19-claude-pty.md)):
1. The latest `rate_limits` of each session comes from the status line.
2. A turn that ends with `five_hour` or `seven_day` at 100 % pauses the task until the latest `resets_at` among the full windows.
3. The scheduler resumes it and sends the "carry on" prompt in the current language.

### Saving mode (`handoff.rs`)

A task with `saving_mode` runs each phase in a fresh Claude session, so the context
re-read on every turn stays small. The chain of sessions is `Session.step`
(`plan`, `implement`, `review`; at most one open session per task and step, a
partial unique index). The primary switch is one transaction: the new session
becomes `primary`, the old one stays in the task as a finished, reopenable tab
(an extra when reopened: it never takes the main role back). The history records
`step-started`.

- **Boundaries** (`boundary`, checked when the primary's turn ends and when the phase is moved by hand):
  - plan, `tasks.md` written and nothing checked: **offer** the implementation (the user confirms from the Bandeja or the task screen, or declines with "Seguir aquí", which clears the handoff and turns saving mode off for the whole task);
  - every item checked: the review session **starts on its own**; when both hold, the furthest step wins.
  - No spec folder, or no `tasks.md`: never a boundary.
- **Stages** (`tasks.handoff`, `{to, stage, error}`, written before each side effect): `offered` → `saving` (types `/save-session` into the outgoing primary, if the `save-notes` preference is on and it is live) → `starting` (stops the old session, waits up to 3 s, launches the new one) → cleared. `failed` (with the error; Bandeja item and a "Reintentar" button) is another; `skipped` is set when the plan session edits code anyway (`on_code_edit`): the implementation is then never offered, the task carries on in that session, and the review handoff still applies. A failed save turn (`TurnFailed`) fails the handoff.
- **Recovery** (`recover`, on each scheduler tick and at start): `saving` and `starting` are finished, `offered` is re-offered, and a step session that was recorded but never launched is relaunched. A `HandoffCoordinator` in `AppState` holds a mutex over the check-and-set and a set of tasks being started, so two confirmations or a tick never start the same step twice. A usage limit mid-handoff pauses the task as usual and the tick resumes it.
- **API:** `POST /api/v1/tasks/{key}/handoff` (`confirm`, `decline`, `retry`), and `saving_mode` on task create, backlog create and `PATCH` (also while running). The preferences `saving-mode` (default for new tasks) and `save-notes` are in Settings.
- **UI:** `chain.ts` and `SessionChain.tsx` draw the chain; `HandoffActions.tsx` is shared by the task screen and the Bandeja item (kind `handoff`: offered, saving, starting, failed). The automatic review is announced like a finished turn: quiet when the app is listening.

### Autopilot (`autopilot/`)

A task with `tasks.autopilot` (nullable JSON: `state` running/paused/done, `stage`, `reason`, `since`, `nudged`, `review_session`) runs plan → implement → review → draft PR on its own ([#38](decisions.md)). It implies saving mode (turning saving mode off clears it) and rides on its handoffs: only a task on the `feature` (spec) workflow can have it, enforced on every entry point (409 `not-spec-workflow`). Every change of the state is `Store::update_autopilot`, a compare-and-set, so a pause or "off" is never overwritten by a finish in flight ([#45](decisions.md)). The task's status and phase are untouched by pause or resume; the history gets `autopilot-paused`, `-resumed`, `-answered`, `-nudged` and `-finished`.

- **Shape** (`shape`): while running, the planning → implementation `Offer` becomes a `Start`; paused, done or off, boundaries are left as saving mode has them. Each step's session gets an autopilot note on top of its prompt (`notes.rs`, in the app's language): nobody will answer, pick the recommended option, stop at `tasks.md` / at every task checked / at the outcome file.
- **Stall and nudge** (`stall`, from `handoff::on_turn_ended`): a turn end of the primary plan or implement session with no boundary reached gets one typed nudge per step (`Autopilot.nudged`, taken atomically); a second stall in the same step pauses ("tasks.md not written", "N tasks unchecked"). The review never stalls: its end is its outcome file ([#39](decisions.md)).
- **Outcome file:** the review is told to write `~/.delilah/tasks/<KEY>/autopilot/review-<session-id>.json` (`outcome` `ready`/`blocked`, `reason`, `pr_title`, `pr_body`), outside the repo. It is read only if it is a regular file under 256 KiB; missing or invalid means blocked. The session id is recorded in `Autopilot.review_session`, so an old file is never read for a newer review.
- **Finish** (`finish`, spawned at the review's turn end; one per task, `Finishing` guard): read the outcome (blocked pauses with its reason); refuse uncommitted changes in any repo (git failing to tell pauses too); start the project's preview if it has a command and wait until it runs or fails (a failed preview does not stop the PR); write `stage = opening_pr`; `pr_loop::check_task` to link a PR that exists already; for each repo with commits and no open PR, `pr_api::open_for(..., draft = true)` with the agent's title and body (else the proposal's), plus a "Preview:" line with the URL minus userinfo, query and fragment, only when its host is loopback ([#42](decisions.md)). Then `state = done` and `autopilot-finished` with the PR URLs. Every write only lands while autopilot still runs on that review (`Superseded` otherwise). A retryable failure (`gh`, network) is retried with a doubling back-off, three attempts, then pauses; one before any stage is recorded (which `recover` would never pick up) pauses at once.
- **Questions and permissions:** see the hook table. Permission prompts are never answered ([#43](decisions.md)).
- **Pause and resume** (`pause`, `resume`): a pause records reason and time, emits the inbox and sends one notification (none if the app is listening). The inbox derives an `autopilot` item from the paused state, with "Reanudar autopiloto" (`POST /tasks/{key}/autopilot` `resume`) and "Seguir a mano" (`off`, saving mode stays). Resume goes to `advance_with`: re-run an interrupted finish, leave a working session alone, retry or confirm a failed or offered handoff; an idle review whose outcome file still says blocked is typed a re-check (recorded as its nudge) instead of finishing into the same pause; else look at the turn as at a turn end.
- **Recovery** (`finish::recover`, on each scheduler tick): a running autopilot at stage `preview` or `opening_pr` with a `review_session` re-runs the (idempotent) finish. Autopilots stored without `review_session` are not recovered ([#47](decisions.md)).
- **API:** `POST /api/v1/tasks/{key}/autopilot` (`on`, `resume`, `off`); `autopilot` on task create and backlog create/`PATCH`; `launch: "autopilot"` on `POST /asks/{id}/answer` with `Crear` on a proposal (`proposals.rs` creates the task, starts it on autopilot, and tells Claude; if the start fails the task stays in the backlog). The CLI has `--autopilot` / `--no-autopilot` on `run` and `new`, and `delilah autopilot <KEY> on|off|resume`.
- **UI:** `AutopilotMark` (running/paused/done, the reason as tooltip), the option next to saving mode, the chain's preview and PR steps (`chain.ts`), the inbox item, and "Crear y lanzar en autopiloto" on `ProposalCard`.

### Previews (`preview.rs`)

A project's `preview_command` (project field, `PATCH /projects/{name}`) is run by the daemon, not the agent ([#41](decisions.md)). Any task with a workspace can start it by hand (`PreviewControl` in the task header, `POST /tasks/{key}/preview`); autopilot starts it at the finish.

- **Process:** `/bin/zsh -lc "<command>"` (a login shell: launchd's environment lacks the user's PATH) with the agents' clean environment (`delilah_agents::env::agent_env`, not the daemon's variables) in the task's workspace root, in its own process group, stdin null, stdout and stderr read into a ring of the last 200 lines (4 KiB per line).
- **URL:** the first `http(s)://` URL it prints (cut at whitespace, control characters, quotes and parentheses) becomes `url` and the state `running`. Exit, or no URL in 10 minutes, is `failed` with the last lines; once it runs, the timeout no longer applies. States: `starting`, `running`, `failed`. 409 `no-preview-command`, `no-workspace`, `running`.
- **Lifecycle:** memory only, one per task, announced on the `tasks` topic. Stopped by `DELETE`, when the task ends (done, failed, archived, deleted: `end_task_runtime`) and at daemon shutdown (`stop_all`); a restart does not adopt it, so it shows none. A stop sends SIGTERM to the group with `/bin/kill -- -<pgid>` (the workspace forbids `unsafe`), and the waiter, which owns the child, sends SIGKILL after 5 s only if the leader is not reaped yet, so a reused pid is never signalled ([#44](decisions.md)).
- **This repo:** `scripts/preview.sh` prints the UI URL first, so it is the one detected.

## Workspaces

- **Branch names:** `start_run` names the branch before the workspace exists.
  - The name is, in this order: the request's `branch` (normalised by `delilah_core::branch::custom_for`; with a project template it is kept as typed, only made git-safe, by `sanitize_typed`); else the project's branch template, if it has one (`settings.branch`, optional: `{ticket}` is the key, `{slug}` is Claude's short proposal, or the title's `slug()` when Claude fails); else `branch_name::propose`: Claude with a 5 s limit, else `title_slug`, else `fallback(key)`. `compose` gives `feature/<short-name>` or `feature/<KEY>-<short-name>` (the `LIBRE-n` of a free task is left out).
  - `branch_name::unique` then calls `delilah_git::free_branch` over every repo of the workspace (the repo, or each chosen submodule): a name is taken if `refs/heads/<name>` or `refs/remotes/*/<name>` exists (`branch_taken`), and `-2`… is tried. It applies in worktree and checkout mode alike, so an explicit `--branch` no longer switches to an existing branch.
  - The start dialog asks `POST /api/v1/projects/{name}/branch-name` (1 s after the title stops changing, 10 s limit) to fill the "Rama" field. With a project template it returns the template-expanded full branch, and the dialog first shows the instant guess `branchName(template, key, title)` (the TS mirror) until the answer arrives.
  - `Task.branch` is read from `workspaces.branch` by a `LEFT JOIN`; a task with no workspace (backlog, archived) has none.

- **Fresh base** (`delilah_git::base`, for every new task branch, in both modes and per repo or submodule):
  - `prepare_base` fetches `origin`, then `resolve_base` picks the default branch in this order: `origin/HEAD` target, `origin/main`, `origin/master`, `main`, `master`, `HEAD`. The branch starts at that commit's **sha**, so it has no upstream.
  - The whole start shares one `Deadline` of `delilah_git::NETWORK_LIMIT` (30 s, all repos together; `git_with_timeout` in `command.rs`: on timeout it kills git's process group, and it never waits on a pipe that a descendant keeps open past the deadline). The local reads around the fetch (`has_origin`, `resolve_base`) get 5 s each of their own, so a fetch that used up the deadline still finds the last known `origin`.
  - `base::locked_fetch` serialises fetches of the same repo with a process-wide lock keyed on its common git dir (a worktree and its main checkout share it), waits for it only until the deadline, and reuses the result of a fetch that finished less than 5 s ago, failure included, so starts queued on an unreachable remote don't each wait again. The PR loop's merge check fetches through it too (`workspace::fetch`, with its own `NETWORK_LIMIT`), but never reuses an earlier fetch.
  - A failed fetch never blocks: the branch starts from the last known `origin` state and `run.rs::report_bases` raises a `base-stale` notice (none for a repo with no origin). Its cause is the branch, new at every start, so a warning dismissed once still shows at a later start; its text carries git's `fatal:` line cut to 200 chars, without credentials in URLs (`short_reason`). It also logs each base ("task branch base").
  - Local `main` is never fast-forwarded. A start always gets a free branch name (see Branch names), so it always starts fresh; `use_checkout` still switches to an existing branch as it is (`Existing`, no fetch) if it is ever given one. A failed checkout-mode start goes back to the previous branches and deletes the branches it created.
- **Worktree mode** (in `workspaces/<KEY>/`):
  - **Parent projects:** a worktree of the parent, with only the chosen submodules initialised, all on the same branch, named as in Branch names below.
  - **Repo projects:** a plain worktree.
  - Several tasks can run in parallel.
- **Checkout mode** (the default for new tasks; `use_checkout` / `leave_checkout`):
  - The task works on a branch in the project's own folder, or in the chosen submodules of a parent.
  - The checkout must be clean to start, and only one active task may hold it.
  - A finished holder is released when a new task takes it.
  - The branches to go back to are stored in `workspaces.previous`.
- **Archive** (`actions::archive`, built on `archive.rs`):
  - **Checkout mode:** goes back to the previous branches. It never refuses: uncommitted files stay, and a folder that cannot switch is logged and left on the task's branch.
  - **Worktree mode:** refuses while there are uncommitted changes or commits on no remote, unless `?force=true`. It then runs `git worktree remove --force`.
  - Sessions are stopped and awaited first, so nothing writes while the workspace goes.
- **Archive on merge** (`pr_loop::merge::on_merged`), when every linked PR of the task is merged and none was closed unmerged:
  1. The task is marked done if it is not terminal, and its sessions are stopped and awaited.
  2. Every repo of the workspace is checked, not only those with a PR. A repo with a merged PR is fetched (a failed fetch blocks) and `delilah_git::check_merged` requires no uncommitted changes (the parent's too) and its local branch contained in the PR's head; the head is the reference, not the base, so squash and rebase merges pass. A repo without a PR (`check_repos`) must have nothing uncommitted and no unpushed commits, and keeps its branch.
  3. A failed check leaves everything as it is and raises an `archive-blocked` notice with the reason.
  4. Otherwise `archive::archive_workspace` releases the workspace with `force=false` (a checkout goes back to the previous branches first) and deletes the task's local branch only in repos with a merged PR, by compare-and-delete: `branch -D` (needed after a squash) at the sha verified in step 2, so a commit made meanwhile is never lost. The parent's branch stays.
  5. A branch that cannot be deleted raises `archive-partial`, which stores the repo labels and shas; the folders are recomputed under the project root. `POST /archive` by hand finishes it, and the item shows in the Bandeja even for an archived task.
  6. One notification per merge, "mergeada y archivada" or "mergeada, sin archivar"; the per-PR one is held back.

  It asks nothing and never deletes a remote branch. A PR closed without merging stops the automatic archive; the task stays and the user archives by hand.
- **Project settings in a run** (`run.rs`): the mode is the request's, else a backlog task's saved choice, else `settings.workspace` (`Option`; unset: a worktree, as before the settings). The branch follows Branch names above (the template is optional: with none, Delilah proposes the name; Rust and TS share `crates/delilah-core/tests/branch_names.json` for the template expansion). In worktree mode, after the worktree exists: shared entries (`share.rs`, notes in `tasks/<KEY>/workspace.log`), then the setup (`setup.rs`), then the agent. The launch inputs go to `tasks/<KEY>/launch.json` so retry/skip survive a restart; `always` submodules are merged into every run, and a rule naming a missing submodule is a 422 pointing at the settings.
- **Delete** (`DELETE /api/v1/tasks/{key}`, `delilah rm`): stops the task's agents, releases the workspace (the branch stays) and frees the key. The same force rule applies.

## Live updates

The UI does not poll:
- `GET /api/v1/events` sends topics only: `tasks`, `sessions`, `inbox`, `specs`, `usage`, `projects`, `prs` (a task's PRs or their summaries changed), `conversations`, or `resync`.
- The UI refetches that one endpoint (`app/src/live.ts`).
- On reconnect it refetches everything, and a full refresh runs every 30 s as a safety net.

## Updates

Only release builds update themselves (`cfg(not(debug_assertions))` in
`app/src-tauri/src/main.rs`, `canUpdate = isDesktop && !DEV` in `app/src/platform.ts`).
The Tauri updater plugin reads `latest.json` from the latest GitHub release of the
public repo and verifies the download against the public key in `tauri.conf.json`.

**Who checks.** The UI (`app/src/useUpdates.ts`, which holds the timings: 10 s after
start, then every 6 h, and a 10 s sessions poll); the menu item "Buscar actualizaciones…" and Ajustes › Actualizaciones › "Buscar ahora"
check on demand. Nothing installs by itself.

**State machine** (`updates.ts`): idle, checking, upToDate, available, confirmSessions,
waiting, downloading, installing, error; the download starts only after the user's
confirmation. "Más tarde" hides that version until a
manual check or a relaunch. The notice counts the live, open sessions from
`api.sessions()`, shell terminals included, because the relaunch restarts the daemon
and stops them too. "Cuando terminen" polls every 10 s and updates once none is left.

**The relaunch.** The app replaces itself and restarts; opening the new version
restarts the existing daemon with the new binary (as in any install), which rewrites
`plugins/delilah/`, copies the database to `backups/` if the version changed
(`upgrade.rs`) and runs the migrations.

## The UI (`app/src`)

- **Core files:**
  - `App.tsx`: layout, routing between views, shortcuts.
  - `api.ts`: every endpoint and type, mirroring the daemon's serde structs. **Keep the two in sync.**
  - `live.ts`: the events socket.
  - `phases.ts`: board columns.
  - `alerts.ts`: Dock badge and notifications while the app is open.
  - `platform.ts`: the only Tauri imports, all dynamic, so the UI runs in a plain browser.
- **Components:**
  - `Sidebar` (a project's idea conversations share one "Ideas" row; `ideas.ts` `ideaToOpen` picks which one it opens), `InboxView` (Bandeja), `BoardView`, `TaskScreen` (tabs: task, sessions, Cambios, files) and `TaskView`;
  - `PrView` (the PR tab), `PrBadge`, `OpenPrDialog`, `ProjectSettings` (the settings screen), `SetupBanner`, `SpecPanel`, `ChangesView`, `FileTree`, `TerminalPane` (xterm.js), `JiraView`, `ProjectsView`, `ProjectView`;
  - `NewTaskDialog`, `BacklogTaskView`, `SettingsView`, `LogsView`, `QuickOpen` (⌘P), `AskCard`, `ProposalCard`, `AutopilotMark`, `PreviewControl`, `Markdown`.
- **Editor:** `editor/` holds CodeMirror 6 and per-task tab state, kept in localStorage.
- **i18n:**
  - Texts are Spanish in the code, wrapped in `t()` / `tn()`, and English lives in `i18n/en.ts`, keyed by the Spanish text.
  - **Every new text needs an `en.ts` entry**; dev builds warn in the console.
  - The daemon picks its own texts with `lang().pick(es, en)`.
