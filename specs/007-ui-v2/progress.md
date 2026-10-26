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
| WP-05 | 2 | 2B | done | 1 | d9b6b49 | Pending permission waits; ended permission cards |
| WP-06 | 2 | 2B | done | 1 | 18ba302 | Checkout is the default workspace |
| WP-07 | 2 | 2B | done | 1 | a3b376d | created_at exposed and reopen |
| WP-08 | 2 | 2B | done | 1 | a224417 | Inbox store, ids, removal and new avisos |
| WP-09 | 2 | 2B | done | 1 | 47ffad7 | PR check times, review comments, merge |
| WP-10 | 2 | 2B | done | 1 | 718c733 | Setup failure detail and Jira subtasks |
| WP-11 | 2 | 2B | done | 1 | 49ec9d7 | Reveal in Finder (Tauri + platform.ts) |
| WP-12 | 2 | 2C | done | 1 | c1339b7 | Session activity and per-session runtime |
| WP-13 | 2 | 2D | done | 1 | 4d98457 | CLI |
| WP-14 | 2 | 2D | done | 1 | 05ad935 | API e2e flows |
| WP-15 | 3 | 3A | done | 1 | 1c4a8d0 | Token guards and v2 tokens |
| WP-16 | 3 | 3A | done | 1 | 8801386 | Vocabulary, relative time, icons |
| WP-17 | 3 | 3B | done | 1 | 1870515 | Rename old tokens in TSX |
| WP-18 | 3 | 3B | done | 1 | 15d2e53 | Kit: controls |
| WP-19 | 3 | 3B | done | 1 | c1e9df2 | Kit: lists, overlays, feedback |
| WP-20 | 3 | 3B | done | 1 | 2f75ab4 | Kit: status components |
| WP-21 | 3 | 3B | done | 1 | 7368434 | Live terminal and editor themes |
| WP-22 | 3 | 3C | done | 1 | 530e3af | Kit barrel |
| WP-23 | 4 | 4A | done | 1 | 48806d0 | Connection, breakpoints, sidebar shortcut |
| WP-24 | 4 | 4B | done | 1 | 1537fa3 | Logs view, statusbar, disconnected banner |
| WP-25 | 4 | 4B | done | 1 | 973db8b | Sidebar rewrite |
| WP-26 | 4 | 4C | done | 1 | 4b8c8cc | App shell |
| WP-27 | 4 | 4D | done | 1 | 761b7b8 | E2E selectors for the shell |
| WP-28 | 5 | 5A | done | 1 | 1e376aa | History sentences and AskCard |
| WP-29 | 5 | 5A | done | 1 | 8a1cdce | Header controls and tabs |
| WP-30 | 5 | 5A | done | 1 | 7220728 | Cambios, PR and setup panels |
| WP-31 | 5 | 5B | done | 1 | c313d31 | Task header |
| WP-32 | 5 | 5B | done | 1 | 91dc283 | Task tab |
| WP-33 | 5 | 5B | done | 1 | 3d93e8a | Chat tab |
| WP-34 | 5 | 5C | done | 1 | 191d0de | Task screen integration |
| WP-35 | 5 | 5D | pending | 0 | | E2E selectors for the task screen |
| WP-36 | 5 | 5D | pending | 0 | | E2E task-screen flow |
| WP-37 | 6 | 6A | done | 1 | a54c617 | Bandeja |
| WP-38 | 6 | 6A | done | 1 | 8b4f80a | First run and board |
| WP-39 | 6 | 6A | done | 1 | 8797d5f | Jira |
| WP-40 | 6 | 6B | pending | 0 | | E2E Bandeja |
| WP-41 | 6 | 6B | pending | 0 | | E2E board |
| WP-42 | 7 | 7A | done | 1 | e360e8a | WherePicker |
| WP-43 | 7 | 7A | done | 1 | da2920b | ProposalCard, delete task, Quick Open |
| WP-44 | 7 | 7A | done | 1 | e4bd446 | Projects and project view |
| WP-45 | 7 | 7A | done | 1 | 16cdf42 | Settings and auto-update preference |
| WP-46 | 7 | 7B | done | 1 | ab5a100 | Nueva tarea dialog |
| WP-47 | 7 | 7B | done | 1 | f80fe4c | Backlog task and project settings |
| WP-48 | 7 | 7C | pending | 0 | | E2E dialogs and projects |
| WP-49 | 7 | 7C | pending | 0 | | E2E new task |
| WP-50 | 8 | 8A | done | 1 | 864dcdc | Sidebar at scale |
| WP-51 | 8 | 8A | done | 1 | a809674 | Bandeja and Jira at width |
| WP-52 | 8 | 8A | done | 1 | 5c2a3fa | Task screen and board at width |
| WP-53 | 8 | 8A | done | 2 | 42e2199 | Global notices |
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
| 4 | WP-25 | NOTE | WP-26: pass Sidebar `rail`, `onToggleRail`, `onAddProject`, `onDone`; drop `onNewTask`; `onDelete` now fires after its own confirm — App must not open DeleteTaskDialog again. Board: wire StatusBar filter + BoardView new props. Banner over main with content greyed. | app/src/App.tsx |
| 5 | WP-32 | NOTE | WP-34: pass TaskView `asks` (task InboxItem[]) and `onOpenTask`. Check `ManualResume` is allowed from failed (Reanudar). «Main Claude» legacy EN key should read «Main Claude» — fine. | app/src/components/TaskScreen.tsx |
| 2 | WP-10 | NOTE | Sample Jira data now has 20 sub-tasks of PROJ-1244 (affects e2e lists and the before/after Jira pair); client `SetupInfo.exit_code` is `number|null`; data-model.md setup.json gains `log`. | app/src/api.ts, app/e2e |
| 5 | WP-31 | NOTE | WP-34: render SessionChain row + base-stale bar under TaskHeader; TaskHeaderProps in WP-31 report (task, phase, owner, hasWorkspace, showTree, onToggleTree, sessionSwitch, autopilotBusy, onAutopilot, onSavingMode, pr, doneAt, onChanged, onError). | app/src/components/TaskScreen.tsx |
| 9 | orchestrator | MEDIUM | i18n coverage test fails under vitest because the transformed `en.ts` glob list is cached while files are added in parallel; re-run with a clean cache after integration (WP-55/58) and confirm it really finds literals. | app/src/i18n |
| 8 | WP-50 | LOW | The 6-task cap can hide the selected task; always show the current task even past the cap. | app/src/components/Sidebar.tsx |
| 7 | WP-47 | MEDIUM | Dirty-checkout refusal is detected by matching error text; give it a daemon error code (`checkout-dirty`) and use it in BacklogTaskView + NewTaskDialog. | crates/delilah-daemon/src/run.rs, app |
| 5 | WP-34 | HIGH | Base-stale bar can only show the notice text: «main avanzó N commits · Actualizar rama» needs the commit count and an update-branch endpoint (daemon + api.ts + TaskScreen). Same API-gap fix package as PrSummary.number / finished date. | crates/delilah-daemon, app/src/api.ts, TaskScreen.tsx |
| 9 | orchestrator | HIGH | styles.test: ~40 font-size/radius values outside the scale across new component CSS (AskCard, AutopilotControls, BacklogTaskView, BoardView, ChangesView, ConversationView, …) and an old token/raw colour in BoardView.css. One fix package after WP-52/WP-44/WP-26. | app/src/components/*.css |
| 2 | WP-05 | MEDIUM | `setup::tests::exit_zero_runs_the_task_and_launches_the_agent` fails in full daemon runs (maybe WP-10 setup.json write order or load); investigate in WP-14/58. | crates/delilah-daemon/src/setup.rs |
| 4 | WP-26F | MEDIUM | Delete with unpushed work: sidebar confirm, then DeleteTaskDialog opens at step 1 again before «Eliminar igualmente». Add a `startForced` prop to DeleteTaskDialog and use it from App/TaskView. | app/src/components/DeleteTaskDialog.tsx, App.tsx |
| 9 | WP-SCALE | LOW | Round dots and the first-run mark lost `50%` (not in the allow-list): allow `border-radius: 50%` for dots/marks in styles.test.ts and restore circles; secondary text mapped 13px→13.5 could be 12 in places (jv-sub, tv-meta, qo-dir). | app/src/styles.test.ts, component css |
| 2 | migration-reviewer | MEDIUM | 30-day purge of dismissals by `dismissed_at` makes still-open causes (failed/paused/base-stale, unresolved turn-done/pr-opened notices) reappear; resolve the backing pr_notice on dismiss and purge only ids no longer listed. | store_inbox.rs:55, inbox.rs |
| 2 | migration-reviewer | MEDIUM | `dismiss` silently drops dismissals for items whose owner is not a task (projects) but returns 204; return 409/404 or confirm unreachable. Add tests: migrations on a DB with existing rows; task delete cascades dismissals. | store_inbox.rs:17 |
| 2 | migration-reviewer | LOW | turn-done/pr-opened rows accumulate (never resolved); text-hash ids change with language. Note downgrade (old binary refuses DB at v21) in release notes. | store_prs.rs, inbox.rs |
| 2 | security-reviewer | LOW→fix | Merge account guard only fires with a cached gh user; with an empty cache (after restart) a swapped login merges as the other account. Refuse (409 `account-unknown`) when nothing is cached. Also put `--` before the url / check https in `pr_merge`. | pr_api/merge.rs:~58, gh/mod.rs:~431 |
