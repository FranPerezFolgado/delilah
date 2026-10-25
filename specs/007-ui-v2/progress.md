# Progress — 007-ui-v2

Branch: feature/ui-v2 · Base: 8264379 · Started: 2026-10-25
Mode: format=markdown commits=yes autonomy=auto

Orchestrator decisions: packages never raise the inline-style ratchet (WP-55 sets the final list); only real dependencies are respected (phases may overlap); WP-49 depends on WP-48; WP-09 may also edit the other implementers of the `Gh` trait; implementers map their own area (codegraph) instead of a separate code-researcher pass.

| WP | Phase | Group | Status | Attempts | Commit | Notes |
|----|-------|-------|--------|----------|--------|-------|
| WP-01 | 1 | 1A | done | 1 | 079f3d3 | Setup guards |
| WP-02 | 2 | 2A | done | 1 | 7874870 | Client read types and calls (api.ts) |
| WP-03 | 2 | 2A | done | 1 | 87b9863 | Core events and created_at |
| WP-04 | 2 | 2A | done | 1 | 8c35baa | Route skeleton |
| WP-05 | 2 | 2B | implementing | 1 |  | Pending permission waits; ended permission cards |
| WP-06 | 2 | 2B | done | 1 | 18ba302 | Checkout is the default workspace |
| WP-07 | 2 | 2B | implementing | 1 |  | created_at exposed and reopen |
| WP-08 | 2 | 2B | implementing | 1 |  | Inbox store, ids, removal and new avisos |
| WP-09 | 2 | 2B | implementing | 1 |  | PR check times, review comments, merge |
| WP-10 | 2 | 2B | implementing | 1 |  | Setup failure detail and Jira subtasks |
| WP-11 | 2 | 2B | done | 1 | 49ec9d7 | Reveal in Finder (Tauri + platform.ts) |
| WP-12 | 2 | 2C | pending | 0 | | Session activity and per-session runtime |
| WP-13 | 2 | 2D | pending | 0 | | CLI |
| WP-14 | 2 | 2D | pending | 0 | | API e2e flows |
| WP-15 | 3 | 3A | done | 1 | 1c4a8d0 | Token guards and v2 tokens |
| WP-16 | 3 | 3A | done | 1 | 8801386 | Vocabulary, relative time, icons |
| WP-17 | 3 | 3B | done | 1 | 1870515 | Rename old tokens in TSX |
| WP-18 | 3 | 3B | done | 1 | 15d2e53 | Kit: controls |
| WP-19 | 3 | 3B | done | 1 | c1e9df2 | Kit: lists, overlays, feedback |
| WP-20 | 3 | 3B | done | 1 | 2f75ab4 | Kit: status components |
| WP-21 | 3 | 3B | done | 1 | 7368434 | Live terminal and editor themes |
| WP-22 | 3 | 3C | done | 1 |  | Kit barrel |
| WP-23 | 4 | 4A | done | 1 | 48806d0 | Connection, breakpoints, sidebar shortcut |
| WP-24 | 4 | 4B | implementing | 1 |  | Logs view, statusbar, disconnected banner |
| WP-25 | 4 | 4B | implementing | 1 |  | Sidebar rewrite |
| WP-26 | 4 | 4C | pending | 0 | | App shell |
| WP-27 | 4 | 4D | pending | 0 | | E2E selectors for the shell |
| WP-28 | 5 | 5A | done | 1 | 1e376aa | History sentences and AskCard |
| WP-29 | 5 | 5A | implementing | 1 |  | Header controls and tabs |
| WP-30 | 5 | 5A | done | 1 | 7220728 | Cambios, PR and setup panels |
| WP-31 | 5 | 5B | pending | 0 | | Task header |
| WP-32 | 5 | 5B | implementing | 1 |  | Task tab |
| WP-33 | 5 | 5B | pending | 0 | | Chat tab |
| WP-34 | 5 | 5C | pending | 0 | | Task screen integration |
| WP-35 | 5 | 5D | pending | 0 | | E2E selectors for the task screen |
| WP-36 | 5 | 5D | pending | 0 | | E2E task-screen flow |
| WP-37 | 6 | 6A | implementing | 1 |  | Bandeja |
| WP-38 | 6 | 6A | done | 1 | 8b4f80a | First run and board |
| WP-39 | 6 | 6A | implementing | 1 |  | Jira |
| WP-40 | 6 | 6B | pending | 0 | | E2E Bandeja |
| WP-41 | 6 | 6B | pending | 0 | | E2E board |
| WP-42 | 7 | 7A | done | 1 | e360e8a | WherePicker |
| WP-43 | 7 | 7A | implementing | 1 |  | ProposalCard, delete task, Quick Open |
| WP-44 | 7 | 7A | pending | 0 | | Projects and project view |
| WP-45 | 7 | 7A | done | 1 | 16cdf42 | Settings and auto-update preference |
| WP-46 | 7 | 7B | pending | 0 | | Nueva tarea dialog |
| WP-47 | 7 | 7B | pending | 0 | | Backlog task and project settings |
| WP-48 | 7 | 7C | pending | 0 | | E2E dialogs and projects |
| WP-49 | 7 | 7C | pending | 0 | | E2E new task |
| WP-50 | 8 | 8A | pending | 0 | | Sidebar at scale |
| WP-51 | 8 | 8A | pending | 0 | | Bandeja and Jira at width |
| WP-52 | 8 | 8A | pending | 0 | | Task screen and board at width |
| WP-53 | 8 | 8A | pending | 0 | | Global notices |
| WP-54 | 8 | 8B | pending | 0 | | E2E widths |
| WP-55 | 9 | 9A | pending | 0 | | Remove replaced code and i18n cleanup |
| WP-56 | 9 | 9A | pending | 0 | | Preview seed |
| WP-57 | 9 | 9A | pending | 0 | | Decisions, status, README, architecture docs |
| WP-58 | 9 | 9B | pending | 0 | | Full checks |
| WP-59 | 9 | 9C | pending | 0 | | Before/after record |
| WP-60 | 9 | 9D | pending | 0 | | Version bump |

## Open findings

| Phase | Reviewer | Severity | Finding | File |
|-------|----------|----------|---------|------|
| 3 | WP-15 | MEDIUM | Light `--tx3` (#6b6e75) on `--side` (#efefec) is 4.43:1 (KNOWN_LOW in tokens.test.ts). Fix: light `--tx3` → #686b72 (4.6:1) and drop the KNOWN_LOW entry. | app/src/styles.css, app/src/tokens.test.ts |
| 1 | WP-01 | LOW | i18n coverage test never seen failing; the regex may pass vacuously. Check it finds literals. | app/src/i18n/coverage.test.ts |
| 2 | WP-06 | MEDIUM | `delilah run`/`new` without --checkout/--worktree now use the checkout: update CLI help (WP-13) and docs decisions 16/57 + README (WP-57); say it in the PR. | crates/delilah-cli, docs |
| 2 | WP-06 | LOW | Daemon tests `conversation::a_message_sent_while_a_card_is_pending_is_queued` and `preview::a_running_one_outlives_the_url_timeout` flaky under heavy load (pass alone). | crates/delilah-daemon |
| 3 | WP-19 | MEDIUM | ConfirmDialog uses `btn-danger`/`btn-p` classes instead of the kit `Button` (`.b-d`/`.b-p`); unify when the barrel lands (WP-22). | app/src/components/ui/ConfirmDialog.tsx |
| 6 | WP-38 | HIGH | Board PR chips need `PrSummary.number` and Hecha cards need the task's finished date: neither exists in the API. Add both (daemon + api.ts) in a fix package, then wire `BoardView` props in App.tsx (WP-26 shell). | crates/delilah-daemon, app/src/api.ts, app/src/App.tsx |
| 6 | WP-38 | LOW | «Responder» English key shadowed by legacy.ts merge order (harmless); clean in WP-55. | app/src/i18n/en |
| 7 | WP-42 | HIGH | `whereOf()` still falls back to "worktree" when unset (asserted in phases.test.ts); decision: unset = checkout. Fix whereOf + test in WP-47 (Backlog uses it). | whereOf file, app/src/phases.test.ts |
| 5 | WP-30 | NOTE | WP-34 must wire `PrView onOpenTerminal`, `SetupBanner onOpenTerminal` and `onOpenSettings` in TaskScreen/TaskView; WP-36 e2e: PR `#n` moved into `.pr-chip`. Run vitest with `--no-cache` (stale glob for new en/ files). | app/src/components/TaskScreen.tsx |
