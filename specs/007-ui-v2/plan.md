# Implementation Plan: UI v2

**Branch**: `feature/ui-v2` | **Date**: 2026-10-25 | **Spec**: [spec.md](spec.md)

**Input**: [spec.md](spec.md); the brief [docs/ui-redesign.md](../../docs/ui-redesign.md)
(source of truth for tokens, components, copy and screens); canvas «Delilah · Cabecera de
tarea» (https://claude.ai/artifact/7V8faKddSQLgsCNjgnEcya, boards named per task). Builds on
[003-task-phase-header](../003-task-phase-header/) (phase in the header, `HeaderPhase`) and
[006-chat-view](../006-chat-view/) (chat view, chat mode, permission cards, runtime switch).

## Summary

A full refactor of `app/` to the v2 design, plus the daemon/API data the new screens need.

- **UI**: v2 tokens (dark default, light via `prefers-color-scheme`) and a base kit in
  `app/src/components/ui/` (Button, StatusBadge, InboxKind, PrChip, PhaseBars, Toggle,
  Segmented, OptionCard, Row, Count, Skeleton, Dialog/ConfirmDialog, Menu, Toast), one
  vocabulary table (`vocab.ts`), live terminal/editor themes; then every screen in the order
  of brief §8: shell + sidebar + statusbar + disconnected state; task screen (new header,
  tabs, inline asks, chat, Cambios, PR, setup failed, done); Bandeja, board + first run,
  Jira; dialogs, projects, settings, backlog task, Nueva tarea, checkout default; widths,
  scale, loading and global notices.
- **Daemon/API** (a parallel track that starts first): task `created_at`; reopen after done;
  pending permission → «Te espera»; checkout as default workspace; inbox item ids,
  dismiss/clear/restore and last activity; two new avisos; kept permission cards and new
  history events; PR check times, review comments and merge; setup failure detail and full
  log; Jira subtasks and counts; session last activity/message and a per-session runtime;
  `health.home`; a Tauri `reveal_path` command; CLI `reopen` and `runtime --session`.
- **Release**: e2e selectors moved to roles/text, the `despues` screenshots, docs, one
  `minor` bump.

## Technical Context

**Language/Version**: Rust (edition 2021 workspace; `app/src-tauri` outside it); TypeScript + React 19 (Vite) in `app/`

**Primary Dependencies**: axum, sqlx (SQLite), tokio, serde; xterm.js, CodeMirror 6 (`Compartment`), Playwright, vitest + Testing Library. No new dependency (npm or crate)

**Storage**: SQLite, new additive migrations `0020_inbox_dismissals.sql`, `0021_session_activity.sql`; `<task_dir>/setup.json`; existing `preferences` keys `first-run`, `auto-update`

**Testing**: `cargo test -p delilah-core|delilah-daemon|delilah-integrations|delilah-cli`; `cargo test --manifest-path app/src-tauri/Cargo.toml`; `cd app && npx tsc --noEmit && npx tsc --noEmit -p e2e && npx vitest related --run <files>` (all: `npx vitest run`); `scripts/e2e.sh ui` and `scripts/e2e.sh api`

**Target Platform**: macOS desktop (Tauri, `minWidth: 1024`); the same UI in a plain browser (previews)

**Project Type**: daemon + CLI + desktop web UI

**Performance Goals**: theme switch reaches terminal/editor < 1 s; disconnected state ≤ 5 s after the daemon stops, recovery ≤ 5 s after it returns (retry interval capped at 5 s); inbox clear/restore round trip < 300 ms locally

**Constraints**: brief §0 (previews only, never 7420; both themes; Tauri drag regions; native-only parts not tried in a preview); no raw hex outside token blocks; no `style={{` except dynamic values; every copy change with its en.ts key; additive migrations; status changes via `AppState::transition`

**Scale/Scope**: ~30 screens/components rewritten, up to 16 kit components (only those with a caller), 9 new routes, 7 changed responses, 2 migrations, 104 tasks

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

`.specify/memory/constitution.md` is the unfilled template; the ground rules in `CLAUDE.md`
are the gates (as in specs 003–006):

| Gate | Status |
|---|---|
| Nothing from the work repos; fixtures only | ✅ seed via `scripts/seed-preview.sh`, fixture repos, Jira sample data, fake `gh`/Claude |
| Agent hooks/settings outside the repo, clean env | ✅ untouched; the per-session runtime reuses `claude::command`/stream launch of 006 |
| Try changes only in a preview; never 7420, never `pkill` | ✅ [quickstart.md](quickstart.md); the disconnected test stops the preview's own process |
| Remote operations only as the confirmed account | ✅ none added; merge uses the user's `gh` like «Abrir PR» (decision 26/40) and the fake `gh` in tests |
| Migrations: never edit a shipped one | ✅ new `0020`, `0021`, additive |
| Contracts `api.rs` ↔ `api.ts` (+ CLI) | ⚠️ changed, additive: [contracts/api.md](contracts/api.md). Contract review at the end of Phase 2 and of Phase 9 |
| Status only through `AppState::transition`; Topics emitted | ✅ new `Reopen` row; permission asks use `AgentAsked`/`UserAnswered` |
| Versioning: one bump per Spec Kit task | ✅ one `minor` at the end (R24) |
| Decisions recorded | ⚠️ amends 16 and **57** (checkout default), adds: pending permission = «Te espera», Chat \| Terminal per session, inbox removals, glossary. Docs task T101 |

Post-design re-check: no new violation. The brief's «one PR per step» (§0) is overridden by
the owner (R1); recorded, not a gate.

## Project Structure

### Documentation (this feature)

```text
specs/007-ui-v2/
├── spec.md, plan.md, research.md, data-model.md, quickstart.md
├── contracts/api.md        HTTP + CLI + Tauri contract
├── contracts/ui.md         kit props, roles, e2e selectors, copy renames
├── checklists/requirements.md
└── tasks.md
```

### Source Code

```text
crates/delilah-core/src/task.rs              Task.created_at; Event::Reopen, RuntimeSwitched, PermissionExpired
crates/delilah-integrations/src/jira.rs      subtasks(parent), subtask_count/subtasks_done, sample children
crates/delilah-daemon/
├── migrations/0020_inbox_dismissals.sql     NEW
├── migrations/0021_session_activity.sql     NEW (last_activity_at, last_message, runtime)
├── tests/fixtures/blocking_kinds.json       NEW, shared with app/src/vocab.test.ts
└── src/
    ├── api.rs              new routes, Health.home
    ├── actions.rs          reopen
    ├── inbox.rs            item ids, dismissed filter (joins inbox_dismissals), dismiss/clear/restore, kinds turn-done/pr-opened
    ├── store_inbox.rs      NEW  dismissal rows
    ├── permissions.rs      pending ask → AgentAsked / UserAnswered; PermissionExpired
    ├── conversation.rs     keep ended permission cards (+decision); last activity on send
    ├── hooks.rs            turn-done notice; session last activity/message
    ├── run.rs              checkout default; launch honours the session runtime override
    ├── runtime_api.rs      POST /sessions/{id}/runtime; RuntimeSwitched on both routes
    ├── store_sessions.rs   new columns, effective runtime
    ├── setup.rs            setup.json; GET setup, setup/log
    ├── jira.rs             GET jira/issues/{key}/subtasks
    ├── gh/graphql.rs, gh/mod.rs, gh/fake.rs   check times, review comments, merge methods, pr_merge
    ├── pr_types.rs         PrCheck/PrSnapshot fields, ReviewComment
    ├── pr_api/merge.rs     NEW  POST …/prs/{id}/merge
    └── pr_api/open.rs, pr_api/mod.rs, prs.rs  pr-opened notice
crates/delilah-cli/src/main.rs               reopen; runtime --session; show created
crates/delilah-e2e/tests/ui_v2.rs            NEW API flows
app/src-tauri/src/main.rs                    reveal_path command
app/src/
├── styles.css              v2 tokens + base only
├── api.ts (+api.test.ts)   every type/call of contracts/api.md; request observer
├── vocab.ts, time.ts, history.ts, theme.ts, useWidth.ts, connection.ts, logLevel.ts   NEW (each with a test)
├── shortcuts.ts            toggle-sidebar
├── platform.ts             revealInFinder
├── useUpdates.ts           auto-update preference
├── i18n/en.ts → aggregator; i18n/en/legacy.ts + i18n/en/<Component>.ts; i18n/coverage.test.ts
├── inline-style.test.ts, styles.test.ts   guards
├── editor/codemirror.ts    live theme (Compartment)
├── App.tsx (+App.css)      shell, breadcrumb, rail, disconnected, filters, ToastHost, GlobalNotices
└── components/
    ├── ui/                 NEW kit: Button, IconButton, StatusBadge, InboxKind, PrChip, PhaseBars, Toggle,
    │                       Segmented, OptionCard, Row, Count, Skeleton, Dialog, ConfirmDialog, Menu, Toast, index.ts
    ├── TaskHeader, TaskTabs, SessionViewSwitch, AutopilotControls, DisconnectedBanner, FirstRun,
    │   AddProjectDialog, GlobalNotices   NEW
    └── rewritten: Sidebar, StatusBar, LogsView, TaskScreen, TaskView, AskCard, ConversationView, ToolCard,
        ChangesView, DiffRows, FileTree, PrView, OpenPrDialog, SetupBanner, SessionChain, PreviewControl,
        InboxView, BoardView, JiraView, NewTaskDialog, WherePicker, ProposalCard, BacklogTaskView,
        ProjectsView, ProjectSettings, ProjectView, SettingsView, DeleteTaskDialog, QuickOpen, UpdateNotice,
        TerminalPane, HandoffActions, SpecPanel, TreeResizer, Icon
        removed at the end: PhaseBadge, PrBadge, AutopilotMark, PermissionCard, status.ts
app/e2e/                    selectors → roles/text; NEW task-screen.spec.ts, new-task.spec.ts, widths.spec.ts
scripts/seed-preview.sh, scripts/fake-gh.sh  more states; `pr merge`
docs/                       decisions.md, status.md, architecture.md, development.md, README.md, ui-redesign.md status, redesign/despues/
```

**Structure Decision**: the kit is a folder of its own with co-located CSS and per-component
English files (R2–R4), so tasks that share no component share no file. Screens are split
into header/tabs/switch components before being rewritten, so `TaskScreen.tsx` is edited by
few, sequenced tasks. The API contract lands first in `api.ts` (one task), so UI tasks build
against types while the daemon side is still being written; the daemon adds every route in
one skeleton task (501 stubs) so later daemon tasks never touch `api.rs`.

## Phasing

Commits follow brief §8 (one or more per step, each green). One PR, one bump.

| Phase | Content | Brief §8 | Depends on |
|---|---|---|---|
| 1 Setup | i18n per-component files, guard ratchets, blocking-kinds fixture | — | — |
| 2 Daemon & API track | contract in `api.ts` first, then core, route skeleton, every §6 item, CLI, API e2e | §6 | Phase 1 (fixture) |
| 3 US1 | tokens + kit + live themes | step 1 | Phase 1 |
| 4 US2 | shell, sidebar, statusbar, disconnected, Logs, rail | step 2 | Phase 3 (T027 first); T004, T005 |
| 5 US3 | task screen | step 3 | Phase 4; T004 (types), T005 (calls), daemon T008–T021 for real data in previews |
| 6 US4 + US5 | Bandeja, board + first run, Jira | step 4 | Phase 5 (AskCard); T004, T005 |
| 7 US6 | Nueva tarea, backlog, projects, project view, settings, dialogs, checkout default | step 5 | Phase 6; T004, T009, T022 |
| 8 US7 | widths, scale, global notices | step 6 | Phase 7 |
| 9 Polish | dead code, i18n cleanup, seed, full checks, `despues` capture, docs, bump | §9 | all |

Phase 2 runs **in parallel** with Phases 3–8. UI tasks depend only on **T004** (read types)
and **T005** (calls; vitest mocks `api`), so they never wait for the daemon; a phase's preview check
(and its e2e) needs the daemon tasks listed in its header to be merged first.

**Daemon chains** (tasks that share files run one after another, never `[P]`):
- **T008 → T015 → T020 → T021**: pending permission (`permissions.rs`) → kept cards and
  expiry (`permissions.rs`, `conversation.rs`) → session activity columns (`conversation.rs`,
  `hooks.rs`, `store_sessions.rs`) → per-session runtime (`runtime_api.rs`, `run.rs`).
- T012 → T013 → T014 → T020 (inbox store → inbox ids → avisos → `hooks.rs`).
- T009 → T021 (`run.rs`); T016 → T017 (`gh/fake.rs`); T007 → every stub owner.
- Every 501 stub of T007 is gone by the end of Phase 2 (checked in T024 and again in T099).

**Ordering in the UI track**: T027 (mechanical token rename across components) finishes
before any Phase 4+ task starts, since it touches their files.

## Versioning

One **minor** bump (T104). The PR must justify it: the checkout default changes what
`POST /runs` and `delilah run`/`new` do without a workspace flag; that is a behaviour
change, but no request/response shape breaks, no data needs migrating and saved choices
are kept, so it is not major. The release notes say it.

## Review amendments (2026-10-25; supersede research.md where they differ)

- R7: reconnect retries every 2 s, backing off to **at most 5 s** (not 10 s), so SC-009 holds.
- R8: recovery rules — only the last pending ask resumes the task; a session ending with an
  open ask expires it; a startup reconcile handles waits lost in a restart (new history
  event `permission-asked`); the inbox lists only the permission item for such a session.
- R11: `inbox_dismissals` is the single source of truth (notice reads join it;
  `pr-notices/{id}/dismiss` writes a row); old rows are purged after 30 days; the client
  percent-encodes ids. **No `GET /inbox/last-activity`**: «última actividad» is derived on the
  client from `/sessions` `last_activity_at`/`last_message` + task events (Topics `Sessions`,
  `Tasks`).
- R12: `pr_notices` reused for `turn-done`/`pr-opened` on purpose; rename later (decisions.md).
- R14: `Session.runtime` keeps its meaning (live process); new `effective_runtime` and
  `runtime_override`; the task-wide runtime route clears per-session overrides.
- R17: merge checks the `gh` account explicitly, 409 with unknown merge methods; the UI
  confirms before merging and explains a disabled «Fusionar».
- R20: `reveal_path` canonicalises before the `$HOME` prefix check (tests with `..` and a
  symlink).
- R4: the kit only contains components with a caller in this PR.
- Scale: `.b.sm` 12 px and `.prchip` radius 7 (appendix superseded); font sizes and radii are
  checked by a test, plus a token contrast test.
- Breakpoints: `compact` (≤ 960) is the main-area width (`ResizeObserver`), not the window.
- `api.ts` is split: T004 read types (so `vocab.ts` starts early), T005 calls.
- Checkout default: a dirty checkout is one human error with «Iniciar en un worktree».
- `setup.json` is a file-based exception, documented with its lifecycle in architecture.md.
- `GlobalNotices` uses only existing `/usage` and `/tasks` data.

## Risks

- **Per-session runtime (R14)** is the largest daemon change; it reuses the 006 switch and
  is sequenced after the sessions migration. If it slips, the header switch still works for
  PTY views and the task-wide runtime; the task stays in this PR (owner: all of §6).
- **«Turn finished» avisos** interpreted as secondary sessions only (spec Assumptions).
- **Checkout default** makes the «checkout must be clean» refusal more frequent; the
  error is shown once with an action.
- **Boards still being drawn** (Conversacion, Cargando, AvisosGlobales, TareaLibre, TareaHecha,
  TareaCaida): follow the brief text; compare when they land.
- **Big-bang PR**: mitigated by commits per step, each green, and phase reviews
  (spec-compliance, contract, test reviewers) at the end of Phases 2, 5, 7 and 9.
- **e2e churn**: selectors move with the renames in the same phase; `screens.spec.ts` must
  keep passing at every phase end.

## Complexity Tracking

| Addition | Why needed | Simpler alternative rejected because |
|---|---|---|
| `inbox_dismissals` + computed ids | «Limpiar avisos» + «Deshacer» across windows (FR-038) | client-only hiding: windows and Dock badge disagree |
| `sessions.runtime` override + route | Chat \| Terminal per session (owner: all of §6) | task-only runtime: the header switch would change other sessions |
| Per-component CSS and en files | ~100 tasks in parallel without shared-file conflicts | one `styles.css` / `en.ts`: serialises every UI task |
| Route skeleton with 501 stubs | daemon tasks in parallel without touching `api.rs` | each task adds its route: 7 tasks on one file |
