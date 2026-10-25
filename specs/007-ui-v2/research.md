# Research: UI v2

Decisions that the brief (`docs/ui-redesign.md`) leaves to the implementer, and the code
facts each one rests on (checked on `feature/ui-v2` at e237d58). Format: decision,
rationale, alternatives.

## R1. One branch, one PR, commits per §8 step

- **Decision**: all of the brief ships on `feature/ui-v2` in one PR. Each §8 step (and the
  daemon/API track) ends with its own commit(s) and green checks; one `minor` bump at the end.
- **Rationale**: owner decision; it overrides brief §0 («one step = one PR»).
- **Alternatives**: six PRs (brief §0): rejected by the owner.

## R2. Where styles live

- **Decision**: `app/src/styles.css` keeps only the v2 tokens (dark `:root`, light under
  `@media (prefers-color-scheme: light)`), the zero-specificity reset, focus, scrollbars,
  checkboxes, type-scale utility classes and the shell grid. Each new component and each
  rewritten screen gets a co-located style sheet (`app/src/components/ui/Button.css`,
  `app/src/components/TaskHeader.css`, …) imported by the component. Old classes are deleted
  from `styles.css` by the task that replaces their last user.
- **Rationale**: ~90 tasks would otherwise all edit one 552-line file; per-component files
  let parallel tasks never share a style file, and Vite bundles them anyway.
- **Alternatives**: one big `styles.css` (constant merge conflicts); CSS modules (renames
  every class, breaks the brief's class vocabulary and the e2e move to roles is enough).

## R3. English texts per component

- **Decision**: `app/src/i18n/en.ts` becomes an aggregator: it keeps today's map as
  `app/src/i18n/en/legacy.ts` and merges every `app/src/i18n/en/*.ts`
  (`import.meta.glob("./en/*.ts", { eager: true })`, each exporting `EN`). A task adds its new
  keys to `app/src/i18n/en/<Component>.ts`. Old keys are not deleted mid-way (that would
  conflict on `legacy.ts`); a coverage test (`app/src/i18n/coverage.test.ts`) reports
  missing keys from the start and orphan keys at the end, and the final cleanup task deletes
  the orphans from `legacy.ts`.
- **Rationale**: every UI task changes copy (brief §3); one shared file would serialise them.
  Keys are the Spanish text, so the same key in two files has the same value.
- **Alternatives**: one `en.ts` with sequenced edits (kills parallelism).

## R4. Base components in `app/src/components/ui/`

- **Decision**: `Button`, `IconButton`, `StatusBadge`, `InboxKind`, `PrChip`, `PhaseBars`,
  `Toggle`, `Segmented`, `OptionCard`, `Row`, `Count`, `Skeleton`, `Dialog`,
  `ConfirmDialog`, `Menu`, `Toast` (+ `ToastHost`) in `app/src/components/ui/`, each with a
  `.test.tsx` and a `.css`; `index.ts` re-exports them. Mappings from domain values to
  labels/tones live in `app/src/vocab.ts` (pure, tested), so `StatusBadge`, `InboxKind`,
  `PrChip`, the sidebar dots and the statusbar counts share one table. `AskCard` stays in
  `components/` (it is domain, not kit). `components/status.ts` (`statusIcon`) is replaced by
  `vocab.ts` and deleted when its last caller moves.
- **Rationale**: brief §2; a folder keeps the kit apart from screens and avoids name clashes
  with today's `PrBadge`, `PhaseBadge`, `AutopilotMark` until they are removed.

## R5. Theme follows the OS, live

- **Decision**: `app/src/theme.ts` exports `cssVar(name)` (reads a token from
  `getComputedStyle(document.documentElement)`) and `onSchemeChange(cb)` (a
  `matchMedia("(prefers-color-scheme: light)")` listener). `TerminalPane` builds the xterm
  theme from `--term-bg`/`--term-tx`/`--tx-strong`/`--sel` and sets `terminal.options.theme`
  on change. `editor/codemirror.ts` puts the theme and highlight style in a `Compartment`
  and reconfigures it on change; it adds a light highlight style and neutral selection and
  search colours (`--sel`, `--hov`).
- **Rationale**: brief §1 («Hard-coded theme to move onto tokens and make live»); today both
  are read once at module load.

## R6. Breakpoints

- **Decision**: `app/src/useWidth.ts` exposes `useBreakpoints()` → `{ rail: <1300,
  overlays: <1200, compact: <=960 }` from `matchMedia` listeners; the rail can also be forced
  by the user (⌘\, persisted in `localStorage` as a per-window convenience). CSS uses the
  same numbers in `@media` rules where no logic is needed.
- **Rationale**: brief §5 Widths; one source for the numbers; components that change
  structure (overlay vs pane) need them in JS.

## R7. Disconnected state

- **Decision**: client only. `app/src/connection.ts` (`useConnection`) records the time of
  the last successful request (wrapping `request` in `api.ts` with an observer hook, no API
  change), polls `health()` every 3 s while down (backoff to 10 s after a minute), and
  exposes `{ state: "up" | "down", since, lastOk, retry() }`. The events socket reconnect in
  `live.ts` triggers a refresh when it comes back.
- **Rationale**: brief §6 «Already possible». SC-009 (≤ 5 s) fits a 3 s poll.

## R8. A pending permission puts the task in «Te espera»

- **Decision**: in `permissions.rs`, when an ask is inserted for a task whose status is
  `Running`, call `AppState::transition(key, Event::AgentAsked)`; when an ask is answered,
  withdrawn or expires and no other ask (question or permission) is pending for that task,
  and the task is `WaitingForUser`, call `transition(key, Event::UserAnswered)`. PTY
  permission prompts already reach `AgentAsked` through the Notification hook
  (`hooks.rs:168`), so nothing changes there.
- **Rationale**: owner decision; brief §9 seed caveat (LIBRE-5 showed «En ejecución»).
  Uses the existing transitions, so notifications and events fire.
- **Alternatives**: a new status (rejected: §3 mapping has no slot for it).

## R9. Checkout is the default workspace

- **Decision**: `run.rs:267-276` resolves `request → backlog task's saved in_checkout →
  project settings.workspace → checkout` (`.unwrap_or(true)`). Clients: `whereOf` fallback
  `"checkout"` (`WherePicker.tsx:11`), `BacklogTaskView.tsx:87` `w ?? "checkout"`,
  `WherePicker.tsx:30` «Por defecto (en tu checkout)», `ProjectSettings` help text, the
  `ProjectSettings.workspace` doc comments in `api.ts` and the core type. Saved choices are
  untouched (no migration).
- **Rationale**: owner decision; decision 16 (A). It reverses decision 57's last clause
  («else a worktree»), so `docs/decisions.md` gets a new entry next to 16 **and** a note on 57.
- **Risk**: a checkout task needs a clean checkout; a dirty one now refuses more often. The
  refusal message is shown once with an action (FR edge case).

## R10. Reopen after «Marcar como hecha»

- **Decision**: a new core event `Reopen` with the transition `Done → WaitingForUser`, and
  `POST /api/v1/tasks/{key}/reopen` (`actions.rs`). Refused (409) when the task is not done or
  has no workspace (archived). Sessions are not restarted (done killed them); the user resumes
  the primary from the Tarea tab (`reopenSession` exists). Autopilot, if it was on, stays
  as it was persisted (paused/done); the reopen does not restart it. The CLI gets
  `delilah reopen <key>`.
- **Rationale**: an undo must be honest after the kill; a delayed client-side «done» would
  lie to other windows and the CLI.
- **Alternatives**: delay the `done` call until the toast expires (other windows disagree for
  seconds; a crash loses the click).

## R11. Inbox removals and restore

- **Facts**: inbox items are derived on every `GET /inbox` (`inbox.rs:233`), with no id;
  only PR/base-stale notices are rows (`pr_notices`, dismissable).
- **Decision**:
  - each item gets a stable `id`: `<kind>:<task>:<session|notice|->:<since as unix ms>`
    (computed, not stored); a new occurrence has a new `since`, so a new id;
  - migration `0020_inbox_dismissals.sql`: `inbox_dismissals(id TEXT PRIMARY KEY, task_key
    TEXT NOT NULL REFERENCES tasks(key) ON DELETE CASCADE, dismissed_at TEXT NOT NULL)`;
  - store functions in a new `store_inbox.rs`; `GET /inbox` drops dismissed ids;
  - `POST /api/v1/inbox/{id}/dismiss` (409 for a blocking kind), `POST /api/v1/inbox/clear`
    (dismisses every non-blocking item now listed, returns their ids), `POST
    /api/v1/inbox/restore {ids}` (deletes those rows; unknown ids ignored); all emit
    `Topic::Inbox`. Notice-backed items (`pr`, `notice`) are dismissed through the same
    endpoint, which also marks the notice row dismissed (so the existing `dismissNotice`
    keeps working and restore clears both).
  - `BLOCKING` is mirrored in Rust (`ItemKind::is_blocking`) and a shared test fixture
    lists the kinds, so the two lists cannot drift.
- **Alternatives**: client-side hiding in `localStorage` (other windows and the Dock badge
  disagree); deleting the causes (would lose PR/branch state).

## R12. New avisos: secondary turn finished, PR opened

- **Decision**: both are `pr_notices` rows (the table already carries non-PR notices like
  `base-stale`): kind `turn-done` written in `hooks.rs` when a **non-primary** Claude
  session's `Stop` arrives (text: its last message, clipped); kind `pr-opened` written when a
  PR is opened or linked (`pr_api/open.rs`, `pr_api` link, and detection in `prs.rs`).
  Inbox kinds `turn-done` and `pr-opened`, non-blocking, «Quitar» works.
- **Rationale**: a primary session's finished turn is already `waiting` (blocking Pregunta,
  `inbox.rs:600`); making it an Aviso would change what blocks. Assumption recorded in spec.

## R13. Permission cards kept after the answer; history events

- **Facts**: `CardBase.outcome` has `pending | answered | terminal | withdrawn`; permission
  card entries are dropped from the conversation once answered (`api.ts:436`).
- **Decision**: `conversation.rs` keeps permission card entries with their final `outcome`,
  the `summary` and, when answered, the `decision` (`allow_once | allow_always | deny`).
  «Caducada · se respondió en la terminal» is the client's text for `terminal`. New core
  events (no status change): `PermissionExpired { tool, summary }` (emitted in
  `permissions.rs` when an ask ends as `terminal` or `withdrawn`), `RuntimeSwitched {
  session: Option<String>, mode }` (emitted by both runtime endpoints), `Reopened` (by
  reopen). The UI maps every event name to a sentence (`app/src/history.ts`).

## R14. Chat | Terminal per session

- **Facts**: runtime is per task (`tasks.chat_mode`, `POST /tasks/{key}/runtime`,
  `runtime_api.rs`); `Session.runtime` is read-only; the view of a PTY session is already
  client-side (`chooseView`).
- **Decision**: `sessions.runtime TEXT NULL` (`'pty' | 'stream'`; NULL = the task's
  `chat_mode`), in migration `0021_session_activity.sql` together with R16's columns.
  `POST /api/v1/sessions/{id}/runtime {mode: "chat"|"terminal"}` switches that Claude session
  between turns (same settle logic as the task endpoint, `--resume`), stores the override
  and emits `RuntimeSwitched`; 409 mid-turn, for shell/setup sessions, or when unchanged.
  The launch path picks `session.runtime.unwrap_or(task.chat_mode)`. The task endpoint
  keeps working (switches the primary and the default for new sessions) for the CLI and old
  clients. The UI: PTY session + Chat = view only; stream session + Terminal, or PTY session
  + «modo chat» = runtime switch after a confirmation.
- **Rationale**: brief §6 «per session (a canvas decision)»; owner: all of §6.
- **Risk**: the largest daemon change here; it reuses the switch code of 006 (decision 67)
  and is covered by the existing fake-claude tests plus new per-session ones.

## R15. Setup failure detail

- **Facts**: `setup.rs` keeps the log at a path in the task dir and passes only the tail as
  `reason`; `pty.exit_code()` is known in `finish` (`setup.rs:273`).
- **Decision**: `setup.rs` writes `<task_dir>/setup.json` `{command, exit_code, started_at,
  finished_at, cancelled, timed_out}` at the end of each run (overwritten on retry). `GET
  /api/v1/tasks/{key}/setup` returns it plus `tail` and `log_bytes`; `GET
  /api/v1/tasks/{key}/setup/log` returns the whole log as `text/plain`. 404 when no setup
  ran. No migration.

## R16. Session last activity and last message

- **Decision**: columns `sessions.last_activity_at TEXT NULL` and `sessions.last_message
  TEXT NULL` (clipped to 200 chars) in `0021_session_activity.sql`, written by `hooks.rs` on
  `Stop` (Claude's last message) and `UserPromptSubmit` (the prompt), and by
  `conversation::send`. Exposed on `Session`. NULL for old rows until their next turn.
- **Alternatives**: one `conversation()` per row (N requests per project view), reading
  transcripts in the list endpoint (slow on big transcripts).

## R17. PR checks timing, review comments, merge

- **Facts**: `gh/graphql.rs:10-30` already fetches reviews and review threads (author,
  path, line, body, time) and check runs (`name status conclusion detailsUrl`).
- **Decision**:
  - add `startedAt completedAt` to `CheckRun` (and `createdAt` for `StatusContext`, used as
    the start); `PrCheck.started_at/finished_at` (optional);
  - `PrSnapshot.review_comments: ReviewComment[]` (`author, path?, line?, body, at`,
    newest 50, review bodies included as comments without path);
  - merge: `Gh::pr_merge(url, method)` runs `gh pr merge <url> --merge|--squash|--rebase`;
    `POST /api/v1/tasks/{key}/prs/{id}/merge {method?}`; the method defaults to the first
    the repo allows (`mergeCommitAllowed / squashMergeAllowed / rebaseMergeAllowed`, added to
    the repo query). 409 when draft, not open or `ci != success`; gh failures go through
    `classify` (AccessProblem) as today. After a merge the PR is refreshed, and the existing
    archive-on-merge policy (`pr_loop/merge.rs`) applies. `scripts/fake-gh.sh` learns `pr merge`.

## R18. Jira subtasks

- **Decision**: `delilah-integrations::jira` gets `subtasks(parent) -> Vec<JiraIssue>`
  (JQL `parent = KEY`, any assignee/status, up to 50; the sample data has a parent with 20
  children) and `subtask_count` / `subtasks_done` (from the parent's `subtasks` field,
  done = `status_category == "done"`) on every issue it lists. Route `GET
  /api/v1/jira/issues/{key}/subtasks`. The UI groups by `status` name, not category.

## R19. Preferences for first run and updates

- **Decision**: no new endpoint: the existing `GET/PUT /api/v1/preferences/{key}` with keys
  `first-run` (`{ jira_skipped: bool, dismissed: bool }`) and `auto-update` (`bool`, default
  true). `useUpdates.ts` skips the scheduled check when `auto-update` is false; the manual
  check («Buscar actualizaciones…») still works.
- **Rationale**: brief §6 asks for «a persisted setting»; preferences are that, shared
  across windows.

## R20. Health and «Mostrar en Finder»

- **Decision**: `Health` gains `home` (the daemon's `DELILAH_HOME`, absolute). The Tauri
  shell gets a `reveal_path(path)` command (`open -R`, path must exist and be under the
  user's home); `platform.ts` exports `revealInFinder(path)` that is a no-op outside the
  desktop, and the button is hidden there.

## R21. Logs level filter

- **Decision**: client only: parse the tracing level token (`INFO|WARN|ERROR|DEBUG|TRACE`)
  from each plain-text `/logs` line (`app/src/logLevel.ts`, tested); lines without one
  inherit the previous line's level (multi-line messages).

## R22. Relative times and history sentences

- **Decision**: `app/src/time.ts` (`relative(iso, now)`: «hace 4 min», «ayer», «hace 2 días»,
  with `Intl.RelativeTimeFormat` in the current locale) and `app/src/history.ts`
  (`describeEvent(EventRecord)` for every core event name, including the three new ones; an
  unknown name falls back to a generic sentence, never the slug). The done date is the
  latest `mark-done` / `agent-finished` event.

## R23. E2E selectors

- **Decision**: move the class selectors listed in brief §8 to roles and text in the step
  that renames them (Phases 4–7), and keep `screens.spec.ts` passing throughout. Labels
  follow the renames: «Start» (Iniciar), «New task», «Free task» (radio), «Project»
  (combobox), «Chat»/«Terminal» (pressed buttons).

## R24. Version level

- **Decision**: **minor**: new user-facing features (disconnected state, Logs button,
  inline answers, undo done, merge, inbox clear, first run), additive API and migrations.
  The checkout default is a behaviour change but needs no migration or client change for old
  clients (CLI sends the workspace only with a flag), so not major.
