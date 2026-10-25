# Tasks: UI v2

**Input**: Design documents from `specs/007-ui-v2/`
**Prerequisites**: plan.md, spec.md, research.md, data-model.md, contracts/api.md, contracts/ui.md, quickstart.md
**Source of truth for visuals and copy**: `docs/ui-redesign.md` (§ numbers below refer to it). Canvas boards: https://claude.ai/artifact/7V8faKddSQLgsCNjgnEcya

**Tests**: test-first. Write each task's tests first (vitest for `app/`, `cargo test` for crates, Playwright for e2e) and see them fail, then implement. Where a task rewrites a component, update its existing test file in the same task (listed under the task).

**Checks per task**: `cargo test -p <crate>`; `cd app && npx tsc --noEmit && npx vitest related --run <files>`; e2e tasks: `scripts/e2e.sh ui` / `scripts/e2e.sh api`. Shell: `cargo test --manifest-path app/src-tauri/Cargo.toml`.

**Rules for every UI task** (brief §8, contracts/ui.md):
- styles in a co-located `<Component>.css` (never new rules in `app/src/styles.css` after T026); tokens only, no raw hex;
- no `style={{` in the touched files except dynamic values (the ratchet in `app/src/inline-style.test.ts` must not go up; lower its number for the file);
- new/renamed copy: Spanish in `t()`/`tn()`, English in `app/src/i18n/en/<Component>.ts`; never delete keys from `app/src/i18n/en/legacy.ts` (T097 does);
- every visible control works, or is disabled with the reason in `title`;
- compare in a preview (`scripts/preview.sh`, never 7420) in both themes at 1440×900 and 1280 with the board named in the task's **Board:** line.

**Commits**: one commit (or more) per brief §8 step, at each phase checkpoint, all checks green. One branch, one PR, **one** version bump (T104).

## Format: `- [ ] [ID] [P?] [Story?] Description` — **Files**, **Acceptance**, **Depends** under each task

---

## Phase 1: Setup

**Purpose**: make parallel UI work possible without shared-file conflicts (research R2–R3).

- [ ] T001 [P] Split English texts per component. Move today's map from `app/src/i18n/en.ts` to `app/src/i18n/en/legacy.ts` (`export const EN`), and make `app/src/i18n/en.ts` merge every `app/src/i18n/en/*.ts` with `import.meta.glob("./en/*.ts", { eager: true })` into the exported `EN` (same type, same import for `i18n.ts`). Write `app/src/i18n/coverage.test.ts` first: it scans `app/src/**/*.tsx?` for `t("…")` / `tn(n, "…", "…")` literals and fails on any without an English entry; an orphan check (en keys no source uses) runs only when `STRICT_ORPHANS = true` (false now; T097 sets it).
  - **Files**: `app/src/i18n/en.ts`, `app/src/i18n/en/legacy.ts`, `app/src/i18n/coverage.test.ts`
  - **Acceptance**: `npx vitest run` green; the English UI is unchanged; adding a key to a new `app/src/i18n/en/X.ts` makes it visible to `t()`.
- [ ] T002 [P] Inline-style ratchet. Write `app/src/inline-style.test.ts`: counts `style={{` per file under `app/src` and fails when a file's count exceeds its entry in a baseline map in the same test (today's counts, 198 across 24 files) or when a file not in the map has any. Tasks lower their file's number; T096 replaces the map by the final allow-list (dynamic values only, e.g. `--tree-w` in `TaskScreen.tsx`).
  - **Files**: `app/src/inline-style.test.ts`
  - **Acceptance**: passes on the current tree; adding one `style={{` anywhere fails it.
- [ ] T003 [P] Shared blocking-kinds fixture `crates/delilah-daemon/tests/fixtures/blocking_kinds.json`: `["permission","question","error","waiting","pr-failed","pr-review","pr-blocked"]` (= `BLOCKING`, `app/src/api.ts:504`). Read by a Rust test in T013 and by `app/src/vocab.test.ts` in T028.
  - **Files**: `crates/delilah-daemon/tests/fixtures/blocking_kinds.json`
  - **Acceptance**: file exists; equal to `BLOCKING` today.

**Checkpoint**: commit `chore(ui-v2): per-component i18n files and guards`.

---

## Phase 2: Foundational — Daemon & API track (runs in parallel with Phases 3–8)

**Purpose**: every §6 data need (contracts/api.md, data-model.md). **T004 and T005 go first**: they are the only tasks that edit `app/src/api.ts`. T004 adds the read types (so `vocab.ts` and the kit can start at once), T005 the calls. Every UI task that consumes a new field (T004) or call (T005) depends on it, listed in each UI task. The rest of this phase never blocks UI tasks, only the preview/e2e check of the phase that shows the data (see each phase header).

**Daemon chains (shared files, strictly in this order)**: T008 → T015 → T020 → T021 (`permissions.rs`, `conversation.rs`, `hooks.rs`, `store_sessions.rs`, `runtime_api.rs`); T013 → T014 → T020 (`inbox.rs`, `hooks.rs`); T009 → T021 (`run.rs`); T016 → T017 (`gh/fake.rs`); T007 → T011/T013/T018/T019/T021 (the stub files).

**Independent Test**: `cargo test --workspace` and `scripts/e2e.sh api` green; the curl checks of quickstart.md answer as contracts/api.md says; `grep -rn "not_implemented" crates/delilah-daemon/src` finds only the constructor and its test.

- [ ] T004 [P] Client read types in `app/src/api.ts` (types only, no calls): `Health` (`home`), `Task.created_at`, `Session.runtime` (unchanged meaning: the live process's runtime, else the one it would launch with), `Session.effective_runtime`, `Session.runtime_override`, `Session.last_activity_at`, `Session.last_message`, `PrCheck.started_at/finished_at`, `PrSnapshot.review_comments` + `merge_methods`, `ReviewComment`, `MergeMethod`, `InboxItem.id`, `InboxKind` += `"turn-done" | "pr-opened"` (`BLOCKING` unchanged), `PermissionCardEntry.decision`, `JiraIssue.subtask_count/subtasks_done`, `SetupInfo`. Fix the doc comment of `ProjectSettings.workspace` («null: not chosen, so the checkout»). Type-level test in `app/src/api.test.ts` (a sample JSON of contracts/api.md assigns to each type).
  - **Files**: `app/src/api.ts`, `app/src/api.test.ts`
  - **Acceptance**: `npx tsc --noEmit` and `npx vitest related --run src/api.ts` green; every type field-for-field equal to contracts/api.md.
- [ ] T005 Client calls in `app/src/api.ts`. Tests first in `app/src/api.test.ts` (fetch mocked): method, path and body of `reopen`, `mergePr`, `setupInfo`, `setupLogUrl`, `dismissInbox` (the id is `encodeURIComponent`-encoded: test with an id containing `:` `/` `%` and spaces), `clearInbox`, `restoreInbox`, `jiraSubtasks`, `setSessionRuntime`. Also the connection observer `onRequestResult(cb: (ok: boolean, at: number) => void): () => void`, called by `request` after each response or network failure (no behaviour change otherwise).
  - **Files**: `app/src/api.ts`, `app/src/api.test.ts`
  - **Acceptance**: vitest green; calls equal to contracts/api.md. **Depends**: T004.
- [ ] T006 [P] Core: in `crates/delilah-core/src/task.rs`, tests first: `Task.created_at: DateTime<Utc>` (serde, set by `Task::new`); `Event::Reopen` (`reopened`) with `Done → WaitingForUser` in `next_status` and invalid from every other status; history-only events that keep the status like `StepStarted`: `Event::RuntimeSwitched { session: Option<String>, mode: String }` (`runtime-switched`), `Event::PermissionAsked { tool: String, summary: String }` (`permission-asked`) and `Event::PermissionExpired { tool: String, summary: String, outcome: String }` (`permission-expired`).
  - **Files**: `crates/delilah-core/src/task.rs`
  - **Acceptance**: `cargo test -p delilah-core` green, with tests named as sentences (`a_done_task_reopens_waiting_for_the_user`, `reopen_is_refused_unless_done`, `history_events_keep_the_status`).
- [ ] T007 Route skeleton. In `crates/delilah-daemon/src/api.rs` register every new route of contracts/api.md, each calling a handler in its owning module that returns `ApiError::not_implemented()` (add it: 501 with a JSON error, like the other constructors): `actions::reopen`, `inbox::{dismiss, clear, restore}`, `pr_api::merge::merge` (new file `crates/delilah-daemon/src/pr_api/merge.rs`, declared in `pr_api/mod.rs`), `setup::{info, log}`, `jira::subtasks`, `runtime_api::set_session_runtime`. Add `home` (absolute `config` home) to `Health` and fill it. Test first in `api.rs` tests: each new route answers non-404 and `/health` has `home`. Every stub is replaced by its owner task in this phase; none may survive Phase 2.
  - **Files**: `crates/delilah-daemon/src/api.rs`, `crates/delilah-daemon/src/actions.rs`, `crates/delilah-daemon/src/inbox.rs`, `crates/delilah-daemon/src/pr_api/mod.rs`, `crates/delilah-daemon/src/pr_api/merge.rs` (new), `crates/delilah-daemon/src/setup.rs`, `crates/delilah-daemon/src/jira.rs`, `crates/delilah-daemon/src/runtime_api.rs`
  - **Shares files with**: T011 (`actions.rs`), T013/T014 (`inbox.rs`, `pr_api/mod.rs`), T017 (`pr_api/merge.rs`), T018 (`setup.rs`), T019 (`jira.rs`), T021 (`runtime_api.rs`) — all run after it.
  - **Acceptance**: `cargo test -p delilah-daemon api::` green; no other behaviour changed. After this task no Phase 2 task edits `api.rs`.
- [ ] T008 Pending permission puts the task in «Te espera» (R8), with its recovery rules. Tests first in `crates/delilah-daemon/src/permissions.rs`, one per rule:
  - `a_pending_permission_puts_a_running_task_waiting` (insert while `running` → `AppState::transition(Event::AgentAsked)` and a `permission-asked` history event);
  - `only_the_last_pending_ask_puts_it_back_to_running` (two permission asks, or a permission and a question in `app.asks`: answering the first keeps `waiting-for-user`, the last → `Event::UserAnswered`);
  - `a_withdrawn_or_expired_last_ask_resumes_it`;
  - `after_a_restart_a_task_waiting_on_a_lost_permission_is_reconciled` (startup: a `waiting-for-user` task whose latest `permission-asked` has no later answer/expiry and that has no live pending ask gets `PermissionExpired { outcome: "withdrawn" }` and stays resumable like any task whose agent died; `waiting-for-user` from a finished turn is untouched). Call the reconcile from daemon start next to `recover_interrupted_setups`.
  - Emit `Topic::Tasks` and `Topic::Inbox`.
  - **Files**: `crates/delilah-daemon/src/permissions.rs`, `crates/delilah-daemon/src/main.rs` (one call at start)
  - **Shares files with**: T015 (`permissions.rs`) — T015 runs after it.
  - **Acceptance**: tests green; existing permissions tests unchanged; autopilot still pauses on a permission ask. **Depends**: T006.
- [ ] T009 Checkout is the default workspace (R9). Tests first in `crates/delilah-daemon/src/run.rs`: `no_choice_anywhere_works_in_the_checkout`, `a_saved_backlog_choice_wins_over_the_default`, `the_project_setting_wins_over_the_default`. Change `run.rs:267-276` to `.unwrap_or(true)`; update the doc comment at `run.rs:158` and the tests that asserted the worktree default. This also changes `delilah run`/`delilah new` without `--checkout`/`--worktree` (the CLI sends nothing then): said in the PR (plan.md › Versioning).
  - **Files**: `crates/delilah-daemon/src/run.rs`
  - **Shares files with**: T021 (`run.rs`) — T021 runs after it.
  - **Acceptance**: `cargo test -p delilah-daemon run::` green.
- [ ] T010 [P] Expose `created_at`. Test first in `crates/delilah-daemon/src/store.rs`: listed and fetched tasks carry the stored `created_at`. Read `t.created_at` in the task queries and set it on `Task`.
  - **Files**: `crates/delilah-daemon/src/store.rs`
  - **Acceptance**: `GET /api/v1/tasks` has `created_at` (RFC 3339). **Depends**: T006.
- [ ] T011 [P] Reopen (R10). Tests first in `crates/delilah-daemon/src/actions.rs`: `reopening_a_done_task_makes_it_wait_and_records_it`, `reopen_is_409_unless_done`, `reopen_is_409_after_archiving`. Replace the T007 stub: `transition(key, Event::Reopen)` after checking the workspace exists; emit `Topic::Tasks` and `Topic::Inbox`.
  - **Files**: `crates/delilah-daemon/src/actions.rs`
  - **Acceptance**: tests green; contracts/api.md errors. **Depends**: T006, T007.
- [ ] T012 [P] Inbox dismissals store. Write `crates/delilah-daemon/migrations/0020_inbox_dismissals.sql` (data-model.md) and a new `crates/delilah-daemon/src/store_inbox.rs` (`impl Store`: `dismiss(ids_with_task)`, `undismiss(ids)`, `dismissed() -> HashSet<String>`, `purge_dismissals(older_than)`), registered in `crates/delilah-daemon/src/lib.rs`. `inbox_dismissals` is the **only** record of a dismissal (notice-backed items included). Tests first in `store_inbox.rs` with `Store::in_memory()`: dismiss/undismiss round trip, unknown ids ignored, rows go with the task (`ON DELETE CASCADE`), purge removes rows older than 30 days only.
  - **Files**: `crates/delilah-daemon/migrations/0020_inbox_dismissals.sql`, `crates/delilah-daemon/src/store_inbox.rs` (new), `crates/delilah-daemon/src/lib.rs`
  - **Acceptance**: `cargo test -p delilah-daemon store_inbox` green.
- [ ] T013 Inbox ids, removal and restore (R11; no last-activity route: the client derives it, contracts/api.md). Tests first in `crates/delilah-daemon/src/inbox.rs`: `every_item_has_a_stable_id`, `a_new_occurrence_has_a_new_id`, `clear_dismisses_only_non_blocking_items_and_returns_their_ids`, `restore_brings_back_exactly_those`, `dismissing_a_blocking_item_is_409`, `a_notice_item_is_dismissed_through_inbox_dismissals_only` (the notice list joins `inbox_dismissals`; `POST /pr-notices/{id}/dismiss` now writes a dismissal row too, so both routes agree), `a_session_with_a_permission_ask_lists_only_the_permission_item` (no duplicate `waiting` item for the same session), `old_dismissals_are_purged` (at startup and at most hourly from `list`), and `blocking_kinds_match_the_shared_fixture` (reads `tests/fixtures/blocking_kinds.json`). Add `id` to `InboxItem`, `ItemKind::is_blocking()`, the dismissed filter in `list`, and replace the T007 stubs. Emit `Topic::Inbox`.
  - **Files**: `crates/delilah-daemon/src/inbox.rs`, `crates/delilah-daemon/src/pr_api/mod.rs` (the notice dismiss handler writes the dismissal row)
  - **Shares files with**: T007 (before), T014 (`inbox.rs`, `pr_api/mod.rs`, after).
  - **Acceptance**: tests green; `GET /inbox` shape per contracts/api.md. **Depends**: T003, T007, T012.
- [ ] T014 New avisos `turn-done` and `pr-opened` (R12; rows in `pr_notices`, reused on purpose — decision recorded in T101). Tests first: in `inbox.rs`, both kinds are items, non-blocking, removable; in `hooks.rs`, a `Stop` of a **non-primary** Claude session writes a `turn-done` notice (primary: unchanged `waiting`); in `pr_api` tests, opening or linking a PR writes a `pr-opened` notice, and detection (`prs.rs`) writes one once per PR.
  - **Files**: `crates/delilah-daemon/src/inbox.rs`, `crates/delilah-daemon/src/hooks.rs`, `crates/delilah-daemon/src/pr_api/open.rs`, `crates/delilah-daemon/src/pr_api/mod.rs`, `crates/delilah-daemon/src/prs.rs`, `crates/delilah-daemon/src/store_prs.rs` (the `pr_notices` writes: `insert_notice` with the kinds `turn-done` / `pr-opened`, and a once-per-PR guard for `pr-opened`)
  - **Shares files with**: T013 (`inbox.rs`, `pr_api/mod.rs`, before), T020 (`hooks.rs`, after).
  - **Acceptance**: tests green; existing inbox/hooks/pr tests unchanged. **Depends**: T013.
- [ ] T015 Keep ended permission cards; `permission-expired` history (R13). Tests first in `crates/delilah-daemon/src/conversation.rs` (`an_answered_permission_card_stays_with_its_decision`, `an_expired_card_stays_as_terminal`) and `permissions.rs` (`an_ask_ending_in_the_terminal_records_permission_expired`, `a_session_ending_with_an_open_ask_records_permission_expired_withdrawn`). Stop dropping `PermissionCardEntry` once answered; add `decision`; emit `Event::PermissionExpired` through the task's event log (no status change; T008's resume rule still applies).
  - **Files**: `crates/delilah-daemon/src/conversation.rs`, `crates/delilah-daemon/src/permissions.rs`
  - **Shares files with**: T008 (`permissions.rs`, before), T020 (`conversation.rs`, after).
  - **Acceptance**: tests green; `GET …/conversation` per contracts/api.md. **Depends**: T006, T008.
- [ ] T016 PR check times, review comments, merge methods (R17). Tests first in `crates/delilah-daemon/src/gh/graphql.rs` (parse a canned response with `startedAt/completedAt`, review threads, repo merge flags) and `pr_types.rs` (an old stored snapshot without the new fields still parses, `merge_methods: []`). Add `startedAt completedAt` (CheckRun), `createdAt` (StatusContext), `mergeCommitAllowed squashMergeAllowed rebaseMergeAllowed` (repository) to `SNAPSHOT_QUERY`; map to `PrCheck.started_at/finished_at`, `PrSnapshot.review_comments` (newest 50, review bodies without path) and `merge_methods`; update the canned responses in `gh/fake.rs`.
  - **Files**: `crates/delilah-daemon/src/gh/graphql.rs`, `crates/delilah-daemon/src/pr_types.rs`, `crates/delilah-daemon/src/gh/fake.rs`
  - **Shares files with**: T017 (`gh/fake.rs`, after).
  - **Acceptance**: `cargo test -p delilah-daemon gh:: pr_types` green.
- [ ] T017 Merge a PR (R17). Tests first in `crates/delilah-daemon/src/pr_api/merge.rs`: `merging_a_passing_pr_calls_gh_and_refreshes`, `a_draft_or_failing_pr_is_409`, `unknown_merge_methods_are_409`, `the_default_method_is_the_first_the_repo_allows`, `the_gh_account_is_checked_before_merging` (the logged-in `gh` user for the PR's host must be the task's account, as `TaskPrs.account`; otherwise 409 with the account problem, nothing run), `a_gh_failure_is_classified`. Add `pr_merge(url, method)` to the `Gh` trait and its CLI impl (`gh pr merge <url> --merge|--squash|--rebase`) in `gh/mod.rs`, the fake in `gh/fake.rs`, the handler (replacing the stub), and `pr merge` in `scripts/fake-gh.sh` (marks `pr-<n>.json` merged).
  - **Files**: `crates/delilah-daemon/src/gh/mod.rs`, `crates/delilah-daemon/src/gh/fake.rs`, `crates/delilah-daemon/src/pr_api/merge.rs`, `scripts/fake-gh.sh`
  - **Acceptance**: tests green; emits `Topic::Prs`; archive-on-merge (`pr_loop/merge.rs`) still applies. **Depends**: T007, T016.
- [ ] T018 [P] Setup failure detail (R15). Tests first in `crates/delilah-daemon/src/setup.rs`: `a_failed_setup_writes_setup_json_with_the_exit_code`, `info_returns_command_exit_code_duration_and_tail`, `log_returns_the_whole_log`, `info_is_404_without_a_setup`. Write `<task_dir>/setup.json` in `finish`/cancel/timeout; replace the `info`/`log` stubs.
  - **Files**: `crates/delilah-daemon/src/setup.rs`
  - **Acceptance**: tests green; `GET …/setup` per contracts/api.md. **Depends**: T007.
- [ ] T019 [P] Jira subtasks and counts (R18). Tests first in `crates/delilah-integrations/src/jira.rs` (`subtasks_of_a_parent_include_every_assignee_and_status`, `parents_carry_subtask_and_done_counts`, sample data has a parent with 20 children) and `crates/delilah-daemon/src/jira.rs` (route 200 / 404 / 409 not connected). Implement `subtasks(parent)` (JQL `parent = KEY`, ≤ 50), `subtask_count`/`subtasks_done`, the sample children, and the handler.
  - **Files**: `crates/delilah-integrations/src/jira.rs`, `crates/delilah-daemon/src/jira.rs`
  - **Acceptance**: `cargo test -p delilah-integrations -p delilah-daemon jira` green. **Depends**: T007.
- [ ] T020 Session activity and runtime override columns (R14, R16). Write `crates/delilah-daemon/migrations/0021_session_activity.sql` (data-model.md). Tests first in `store_sessions.rs` (columns round trip), `session_api.rs` (`runtime` keeps its meaning — live process, else what it would launch with; `effective_runtime` = override else the task's `chat_mode`), `hooks.rs` (`Stop` sets `last_activity_at` and Claude's last message clipped to 200; `UserPromptSubmit` sets the prompt) and `conversation.rs` (a sent message updates them). Expose `runtime_override`, `effective_runtime`, `last_activity_at`, `last_message` on `Session`. Readers of `runtime`, checked unchanged: `session_api.rs::runtime_name` (`:298`, `:306`), `run.rs::runtime_kind`, the conversation response's `runtime`, `app/src/components/ConversationView.tsx:126`, `app/src/components/TaskScreen.tsx:94,553,722,827`.
  - **Files**: `crates/delilah-daemon/migrations/0021_session_activity.sql`, `crates/delilah-daemon/src/store_sessions.rs`, `crates/delilah-daemon/src/session_api.rs`, `crates/delilah-daemon/src/hooks.rs`, `crates/delilah-daemon/src/conversation.rs`
  - **Shares files with**: T014 (`hooks.rs`), T015 (`conversation.rs`) — both before; T021 (`store_sessions.rs` reads) after.
  - **Acceptance**: tests green; `GET /sessions` per contracts/api.md. **Depends**: T014, T015.
- [ ] T021 Per-session runtime (R14). Tests first in `crates/delilah-daemon/src/runtime_api.rs` with the fake PTY/stream programs of 006: `switching_one_session_leaves_the_others`, `the_override_survives_a_restart`, `a_running_turn_is_409`, `a_shell_session_is_409`, `both_routes_record_runtime_switched`, `the_task_route_clears_every_session_override`. Replace the stub with the switch (reuse `set_runtime_with`'s settle and `--resume`), store the override; in `run.rs` the launch picks `session.runtime_override.unwrap_or(task.chat_mode)`; the task route also clears the overrides of the task's sessions and emits `RuntimeSwitched { session: None }`.
  - **Files**: `crates/delilah-daemon/src/runtime_api.rs`, `crates/delilah-daemon/src/run.rs`
  - **Shares files with**: T007 (`runtime_api.rs`), T009 (`run.rs`) — both before.
  - **Acceptance**: tests green; existing runtime tests unchanged; emits `Topic::Sessions`, `Topic::Tasks`. **Depends**: T006, T007, T009, T020.
- [ ] T022 [P] «Mostrar en Finder» (R20). Tauri command `reveal_path { path }` in `app/src-tauri/src/main.rs`: canonicalise the path first (resolving `..` and symlinks), then require it to exist and to be under the canonical `$HOME`; `open -R`. Unit tests on the validation function: a plain path under home passes; `~/../other` is refused; a symlink in home pointing outside is refused; a missing path is refused. `revealInFinder(path)` in `app/src/platform.ts` (no-op outside `isDesktop`), with a vitest for the no-op.
  - **Files**: `app/src-tauri/src/main.rs`, `app/src/platform.ts`, `app/src/platform.test.ts` (new)
  - **Acceptance**: `cargo test --manifest-path app/src-tauri/Cargo.toml` and the vitest green. Native: owner checks in the desktop app.
- [ ] T023 CLI. Tests first in `crates/delilah-cli` (argument parsing and request paths, as its existing tests do): `delilah reopen <KEY>`; `delilah runtime <KEY> <chat|terminal> [--session <ID>]`; `delilah show` prints `created`; the `--checkout`/`--worktree` help says the default is the checkout.
  - **Files**: `crates/delilah-cli/src/main.rs`
  - **Acceptance**: `cargo test -p delilah-cli` green. **Depends**: T011, T021.
- [ ] T024 API e2e flows in a new `crates/delilah-e2e/tests/ui_v2.rs` (pattern of `states.rs`): a chat-mode task whose stream fake (`crates/delilah-e2e/src/fake/stream.rs`) asks a permission is `waiting-for-user`, then `running` after the answer; `clear` never returns a blocking id and `restore` brings the rest back; `done` → `reopen` → `waiting-for-user`; a project with setup `exit 3` → `GET …/setup` has `exit_code: 3`; a run with no workspace choice works in the checkout; every new route answers something other than 501 (no stub left).
  - **Files**: `crates/delilah-e2e/tests/ui_v2.rs` (new)
  - **Acceptance**: `scripts/e2e.sh api -- ui_v2` green; `grep -rn "not_implemented()" crates/delilah-daemon/src` only in `api.rs` (constructor + its test). **Depends**: T008–T021.

**Checkpoint**: `cargo test --workspace` + `scripts/e2e.sh api` green; no 501 stub left; contract review (contract-reviewer: `api.rs` + handlers ↔ `api.ts` ↔ CLI, contracts/api.md); commit `feat(daemon): data for UI v2`.

---

## Phase 3: User Story 1 — Tokens and base components (Priority: P1) 🎯 MVP · brief §8 step 1

**Goal**: v2 tokens, the kit, live terminal/editor themes; visual only, no behaviour change.

**Independent Test**: flip the OS colour scheme with a terminal and an editor open; compare with **Sistema v2 › Sistema**. Done when no old token name is left and terminal/editor re-theme without reload.

- [ ] T025 [US1] Token and scale guards (fail now). `app/src/styles.test.ts`: no raw hex in any `app/src/**/*.css` outside the two token blocks of `styles.css`; none of `--acc --accbg --acctx --elev --done --term[^-] --pausebg --addbg --delbg --hunkbg` in `app/src/**/*.{ts,tsx,css}`; every `font-size` in `app/src/**/*.css` is 19 / 15 / 13.5 / 12 px and every `border-radius` is 7 / 10 / 14 / 11 px, except the sub-element allow-list of contracts/ui.md (kept in the test, each entry with a comment). `app/src/tokens.test.ts`: parses both token blocks and checks WCAG contrast ≥ 4.5:1 for `--tx`/`--tx-strong`/`--tx2` on `--bg --side --panel --card --dialog --sel`, `--tx3` on `--bg --side --panel` only, each `--{wait,run,pause,ok,err,neutral}-tx` on its `-bg`, `--primary-tx` on `--primary`, `--add-tx`/`--del-tx` on their `-bg`, `--term-tx` on `--term-bg`, in dark and light.
  - **Files**: `app/src/styles.test.ts` (new), `app/src/tokens.test.ts` (new)
  - **Acceptance**: fail before T026/T027, pass after (SC-001, SC-003, SC-004). A token pair under 4.5:1 is reported to the owner, not silently changed.
- [ ] T026 [US1] Tokens and base in `app/src/styles.css` (brief §1, appendix): replace `:root` (`:2-17`) and the diff tokens (`:237-240`) with the v2 dark `:root` and the light block in `@media (prefers-color-scheme: light)`, add `--wait-on`; zero-specificity reset `:where(button,input,textarea,select)`, `:focus-visible` ring, themed scrollbars and checkboxes/radios, `.mono .t-view .t-sec .t-label .muted .faint .small`, font stacks and type scale; raw shadows/backdrops (`:111,113,179,187`) → `var(--shadow)` / `var(--scrim)`; rename the old tokens used by the remaining rules by meaning (brief §1 «Old → new»; amber never by default; `.etab.on`, `.jrow.on` → `--tx-strong`/`--sel`). Values follow contracts/ui.md, which supersedes the appendix on two points: `.b.sm` font-size 12 (not 12.5) and `.prchip` radius 7 (not 6).
  - **Files**: `app/src/styles.css`
  - **Acceptance**: no old token defined; both themes render readable on every screen (no white native scrollbar). **Board**: Sistema v2 › Sistema.
- [ ] T027 [US1] Rename old tokens left in TSX (inline `var(--acc)` etc.) by meaning, mechanically, **strictly before any Phase 4+ task starts** (it touches their files), in `app/src/App.tsx` and `app/src/components/*.tsx` **except** `Icon.tsx`, `TerminalPane.tsx` (T030, T046 own them) and `app/src/editor/*` (T047). No other change.
  - **Files**: `app/src/App.tsx`, `app/src/components/*.tsx` (those with old token names; not Icon/TerminalPane)
  - **Acceptance**: T025 passes together with T046/T047; `npx vitest run` unchanged. **Depends**: T026.
- [ ] T028 [P] [US1] Vocabulary `app/src/vocab.ts` (+ `vocab.test.ts` first): `statusOf`, `kindOf`, `prStateOf`, `sessionDot` exactly as brief §3 and contracts/ui.md; the test also checks `BLOCKING` equals `crates/delilah-daemon/tests/fixtures/blocking_kinds.json` and that every `InboxKind` (incl. `turn-done`, `pr-opened`) is mapped.
  - **Files**: `app/src/vocab.ts`, `app/src/vocab.test.ts`, `app/src/i18n/en/vocab.ts`
  - **Acceptance**: vitest green. **Depends**: T003, T004.
- [ ] T029 [P] [US1] Relative time `app/src/time.ts` (+ test first): «hace un momento», «hace 4 min», «hace 3 h», «ayer», «hace 2 días», dates beyond a week; Spanish and English via `Intl.RelativeTimeFormat` with `locale()`.
  - **Files**: `app/src/time.ts`, `app/src/time.test.ts`, `app/src/i18n/en/time.ts`
  - **Acceptance**: vitest green with a fixed `now`.
- [ ] T030 [P] [US1] Icons needed by v2 in `app/src/components/Icon.tsx` (stroke 1.5, viewBox 16, `aria-hidden` by default): rail/sidebar, logs, plus, more (⋯), folder, repo, PR, merge, check, chevron, filter, external link, retry, priority, beta-less; remove emoji anywhere in `Icon.tsx`; rename old tokens there.
  - **Files**: `app/src/components/Icon.tsx`
  - **Acceptance**: tsc green; icons render in both themes. **Board**: Sistema v2 › Sistema.
- [ ] T031 [P] [US1] `Button` + `IconButton` (contracts/ui.md) with tests first (variants → classes, sizes 32/28 with `sm` font-size 12, disabled needs `title`, `IconButton` requires `label`).
  - **Files**: `app/src/components/ui/Button.tsx`, `Button.css`, `Button.test.tsx`, `IconButton.tsx`, `IconButton.test.tsx` (all under `app/src/components/ui/`)
  - **Acceptance**: first caller T053, T062 (a kit component with no caller in this PR is dropped); vitest green; danger has a red border. **Board**: Sistema.
- [ ] T032 [P] [US1] `StatusBadge` (tests first: every `TaskStatus.type` + backlog → tone/text/detail per §3; `title` carries reason / «se reanuda a las HH:MM»).
  - **Files**: `app/src/components/ui/StatusBadge.tsx`, `StatusBadge.css`, `StatusBadge.test.tsx`
  - **Acceptance**: first caller T062, T073 (a kit component with no caller in this PR is dropped); vitest green. **Board**: Sistema, Cabecera de tarea › Estados. **Depends**: T028.
- [ ] T033 [P] [US1] `InboxKind` (icon + Pregunta/Permiso/Aviso, no background), tests first.
  - **Files**: `app/src/components/ui/InboxKind.tsx`, `InboxKind.css`, `InboxKind.test.tsx`
  - **Acceptance**: first caller T071 (a kit component with no caller in this PR is dropped); vitest green. **Board**: Sistema. **Depends**: T028.
- [ ] T034 [P] [US1] `PrChip` (#n + Borrador/Abierta/Checks fallan/Fusionada; Abierta neutral; radius 7), tests first.
  - **Files**: `app/src/components/ui/PrChip.tsx`, `PrChip.css`, `PrChip.test.tsx`
  - **Acceptance**: first caller T062, T073 (a kit component with no caller in this PR is dropped); vitest green. **Board**: Sistema. **Depends**: T028.
- [ ] T035 [P] [US1] `PhaseBars` (4 segments + «Plan · 2/4», «· 6/14 tareas» in Implementación from `HeaderPhase`; `mini`; `labelInTooltip`; nothing for done; «Sin iniciar» for setup-failed; neutral colours only), tests first (cases from `PhaseBadge.test.tsx`).
  - **Files**: `app/src/components/ui/PhaseBars.tsx`, `PhaseBars.css`, `PhaseBars.test.tsx`
  - **Acceptance**: first caller T062, T073 (a kit component with no caller in this PR is dropped); vitest green. **Board**: Cabecera de tarea › Main, Estados.
- [ ] T036 [P] [US1] `Toggle` (`aria-pressed`, switch, `iconOnly`, disabled with `title`), tests first.
  - **Files**: `app/src/components/ui/Toggle.tsx`, `Toggle.css`, `Toggle.test.tsx`
  - **Acceptance**: first caller T059 (a kit component with no caller in this PR is dropped); vitest green. **Board**: Sistema.
- [ ] T037 [P] [US1] `Segmented` (`mode: "radio"` → radiogroup/radio/aria-checked with arrow keys; `"pressed"` → buttons with aria-pressed; same look; `tag` «beta»), tests first.
  - **Files**: `app/src/components/ui/Segmented.tsx`, `Segmented.css`, `Segmented.test.tsx`, `app/src/i18n/en/Segmented.ts`
  - **Acceptance**: first caller T051, T060 (a kit component with no caller in this PR is dropped); vitest green. **Board**: Sistema.
- [ ] T038 [P] [US1] `OptionCard` (radio role, ✓ when checked), tests first.
  - **Files**: `app/src/components/ui/OptionCard.tsx`, `OptionCard.css`, `OptionCard.test.tsx`
  - **Acceptance**: first caller T077 (a kit component with no caller in this PR is dropped); vitest green. **Board**: Sistema.
- [ ] T039 [P] [US1] `Row` (32 px and 52 px two-line; `aria-current`/`aria-selected`; leading/trailing slots), tests first.
  - **Files**: `app/src/components/ui/Row.tsx`, `Row.css`, `Row.test.tsx`
  - **Acceptance**: first caller T054 (a kit component with no caller in this PR is dropped); vitest green. **Board**: Sistema v2 › Lateral.
- [ ] T040 [P] [US1] `Count` (neutral / hot with `--wait`/`--wait-on`), tests first.
  - **Files**: `app/src/components/ui/Count.tsx`, `Count.css`, `Count.test.tsx`
  - **Acceptance**: first caller T054, T061 (a kit component with no caller in this PR is dropped); vitest green.
- [ ] T041 [P] [US1] `Skeleton` (`aria-busy` container, lines/width), tests first.
  - **Files**: `app/src/components/ui/Skeleton.tsx`, `Skeleton.css`, `Skeleton.test.tsx`
  - **Acceptance**: first caller T066, T071 (a kit component with no caller in this PR is dropped); vitest green. **Board**: Cargando (when drawn).
- [ ] T042 [P] [US1] `Dialog` + `ConfirmDialog` (role dialog, aria-modal, aria-labelledby, focus trap, Esc, focus return, scrim; footer Cancelar · Acción; `blocked` note disables the action), tests first.
  - **Files**: `app/src/components/ui/Dialog.tsx`, `Dialog.css`, `Dialog.test.tsx`, `ConfirmDialog.tsx`, `ConfirmDialog.test.tsx`, `app/src/i18n/en/Dialog.ts`
  - **Acceptance**: first caller T059, T060 (a kit component with no caller in this PR is dropped); vitest green. **Board**: Diálogos y estados › Eliminar.
- [ ] T043 [P] [US1] `Menu` (role menu/menuitem, ↑↓ Home End, Esc, focus return, danger items), tests first.
  - **Files**: `app/src/components/ui/Menu.tsx`, `Menu.css`, `Menu.test.tsx`
  - **Acceptance**: first caller T054, T061 (a kit component with no caller in this PR is dropped); vitest green. **Board**: Sistema.
- [ ] T044 [P] [US1] `Toast`, `ToastHost`, `useToast` (role status, action button, ttl, one stack), tests first.
  - **Files**: `app/src/components/ui/Toast.tsx`, `Toast.css`, `Toast.test.tsx`
  - **Acceptance**: first caller T055, T064 (a kit component with no caller in this PR is dropped); vitest green. **Board**: AvisosGlobales (when drawn).
- [ ] T045 [US1] Barrel `app/src/components/ui/index.ts` re-exporting T031–T044.
  - **Files**: `app/src/components/ui/index.ts` (new)
  - **Acceptance**: tsc green. **Depends**: T031–T044.
- [ ] T046 [US1] Live terminal theme (R5): `app/src/theme.ts` (`cssVar`, `onSchemeChange`, tested with a mocked `matchMedia`); `TerminalPane.tsx:10-15` builds the xterm theme from `--term-bg/--term-tx` (cursor `--tx-strong`, selection `--sel`, no amber) and updates `terminal.options.theme` on change.
  - **Files**: `app/src/theme.ts`, `app/src/theme.test.ts`, `app/src/components/TerminalPane.tsx`
  - **Acceptance**: vitest green; in a preview the terminal flips with the OS scheme without reload. **Depends**: T026.
- [ ] T047 [US1] Live editor theme (R5): `app/src/editor/codemirror.ts` puts theme + highlight style in a `Compartment`, reconfigures on `onSchemeChange`, adds a light highlight style, neutral selection/search (`:21-29`); test first in `app/src/editor/codemirror.test.ts` (the compartment is reconfigured when the scheme changes).
  - **Files**: `app/src/editor/codemirror.ts`, `app/src/editor/codemirror.test.ts` (new)
  - **Acceptance**: vitest green; editor flips live. **Depends**: T046.

**Checkpoint**: T025 green; `npx vitest run`, `scripts/e2e.sh ui` green; compare Sistema in both themes; commit `feat(ui-v2): step 1 — tokens and base components`.

---

## Phase 4: User Story 2 — Shell, sidebar, statusbar, disconnected (Priority: P1) · brief §8 step 2

**Goal**: FR-016–FR-021. **Depends**: Phase 3 (T027 done before anything here); T004 (`Health`), T005 (`onRequestResult`). No daemon task needed.

**Independent Test**: stop the preview daemon → disconnected state; start it → recovers alone. **Boards**: Sistema v2 › Lateral; Diálogos y estados › Desconectado, Logs.

- [ ] T048 [P] [US2] Connection state `app/src/connection.ts` (`useConnection`, R7) with tests first (fake timers): goes `down` on a failed request or health, polls every 2 s, backing off to at most 5 s (never longer, so SC-009 holds), `up` on the first success, `lastOk`, `retry()`.
  - **Files**: `app/src/connection.ts`, `app/src/connection.test.ts`
  - **Acceptance**: vitest green. **Depends**: T005 (`onRequestResult`).
- [ ] T049 [P] [US2] Breakpoints `app/src/useWidth.ts`: `useBreakpoints()` for the **window** (rail < 1300, overlays < 1200; mocked `matchMedia`) and `useContainerWidth(ref)` (a `ResizeObserver`) for the **main area**: the task header's compact mode is main-area width ≤ 960, not the window. Tests first.
  - **Files**: `app/src/useWidth.ts`, `app/src/useWidth.test.ts`
  - **Acceptance**: vitest green.
- [ ] T050 [P] [US2] Shortcut `toggle-sidebar` (⌘\, group «Ventana», «Plegar barra lateral») in `app/src/shortcuts.ts`, shown and rebindable in Ajustes › Atajos; test first in `app/src/shortcuts.test.ts` (new).
  - **Files**: `app/src/shortcuts.ts`, `app/src/shortcuts.test.ts`, `app/src/i18n/en/shortcuts.ts`
  - **Acceptance**: vitest green.
- [ ] T051 [P] [US2] Logs view: `app/src/logLevel.ts` (R21, test first) and `app/src/components/LogsView.tsx` rewrite: Segmented radio «Todo · Info · Advertencias · Errores», search, «Seguir», «Copiar», INFO `--tx2`, WARN `--tx-strong` + word, ERROR `--err-tx`; lines stay in English.
  - **Files**: `app/src/logLevel.ts`, `app/src/logLevel.test.ts`, `app/src/components/LogsView.tsx`, `LogsView.css`, `LogsView.test.tsx` (new), `app/src/i18n/en/LogsView.ts`
  - **Acceptance**: vitest green. **Board**: Diálogos y estados › Logs.
- [ ] T052 [P] [US2] Statusbar `app/src/components/StatusBar.tsx` (brief §5 Shell): connection dot + text, counts as buttons (`onFilter(status)`) using `vocab.statusOf`, «con error» `--err-tx`, usage «Claude 5 h 82 % · Semana 35 %», Logs button (`onLogs`); tests first in `StatusBar.test.tsx` (new): counts from tasks, «Agentes trabajando» gone, disconnected text.
  - **Files**: `app/src/components/StatusBar.tsx`, `StatusBar.css`, `StatusBar.test.tsx`, `app/src/i18n/en/StatusBar.ts`
  - **Acceptance**: vitest green. **Board**: Sistema v2 › Lateral, Desconectado. **Depends**: T028, T048.
- [ ] T053 [P] [US2] `app/src/components/DisconnectedBanner.tsx`: «Sin conexión con delilahd · reintentando», «Reintentar ahora», how to start it, «Datos de hace N min» (T029); tests first.
  - **Files**: `app/src/components/DisconnectedBanner.tsx`, `DisconnectedBanner.css`, `DisconnectedBanner.test.tsx`, `app/src/i18n/en/DisconnectedBanner.ts`
  - **Acceptance**: vitest green. **Board**: Desconectado. **Depends**: T029, T031, T048.
- [ ] T054 [P] [US2] Sidebar rewrite `app/src/components/Sidebar.tsx` (brief §5 Sidebar, FR-019, FR-020): `Row`/`Count`, nav (Bandeja hot = blocking, Tareas, Jira, Proyectos; shortcuts in tooltips), «Proyectos» + «Añadir proyecto», project header (chevron, name as accessible name, hover «Preguntar a Claude» / «Nueva tarea en este proyecto»), Ideas row, conversation rows with dot, task rows (dot + title first, key on hover/active, tooltip key · title · status), «Backlog · N» (`aria-expanded`), «N hechas», «Sin proyecto», Ajustes + «Plegar barra lateral»; task rows' context menu (`Menu`: Abrir la tarea, Marcar como hecha, Eliminar tarea… → `ConfirmDialog`, the path `screens.spec.ts` uses to delete); `rail` prop renders the 56 px icon rail; remove «+ Tarea». Update `Sidebar.test.tsx` first.
  - **Files**: `app/src/components/Sidebar.tsx`, `Sidebar.css`, `Sidebar.test.tsx`, `app/src/i18n/en/Sidebar.ts`
  - **Acceptance**: vitest green; long titles and project names end in an ellipsis with the full text in the tooltip; no title cut to ~10 chars at 248 px. **Board**: Sistema v2 › Lateral. **Depends**: T028, T030, T039, T040, T042, T043, T050.
- [ ] T055 [US2] Shell in `app/src/App.tsx` (+ new `app/src/App.css`): titlebar 38 / statusbar 28 / sidebar 248 (rail 56); breadcrumb «Delilah / <vista>» and «Delilah / <proyecto> / <KEY>» with `data-tauri-drag-region` on every part and room for the traffic lights; rail from ⌘\ (persisted per window, try/catch) or `useBreakpoints().rail`; disconnected: banner over main, content greyed (`inert` + class), actions disabled; Logs from the statusbar in the browser too (keep the `delilah:show-logs` event for the menu); board status/project filter state fed by the statusbar; mount `ToastHost`; ⌘N stays «Nueva tarea». Test first in a new `app/src/App.test.tsx` (breadcrumb text, rail toggle, disconnected greys content).
  - **Files**: `app/src/App.tsx`, `app/src/App.css` (new), `app/src/App.test.tsx` (new), `app/src/i18n/en/App.ts`
  - **Acceptance**: vitest green; stopping the preview daemon shows the disconnected state and it recovers alone (SC-009); an action whose request fails because the daemon dropped mid-action (e.g. sending a reply) shows one human error, is not retried silently, and the disconnected state takes over; a long project name or task title in the breadcrumb ends in an ellipsis with the full text in the tooltip, and the drag regions stay. **Board**: Lateral, Desconectado. **Depends**: T044, T049, T051–T054.
- [ ] T056 [US2] E2E selectors for the shell: in `app/e2e/screens.spec.ts` the sidebar «Tarea» button → board «New task»/⌘N, «Backlog (2)» → «Backlog · 2», Logs via the statusbar button; `app/e2e/projects.spec.ts` `.pgroup .phead` → sidebar button by project name; `app/e2e/smoke.spec.ts` if it uses the nav.
  - **Files**: `app/e2e/screens.spec.ts`, `app/e2e/projects.spec.ts`, `app/e2e/smoke.spec.ts`
  - **Acceptance**: `npx tsc --noEmit -p e2e` and `scripts/e2e.sh ui` green. **Depends**: T055.

**Checkpoint**: checks green; boards compared; commit `feat(ui-v2): step 2 — shell, sidebar, statusbar, disconnected state`.

---

## Phase 5: User Story 3 — Task screen (Priority: P1) · brief §8 step 3

**Goal**: FR-022–FR-036. **Depends**: Phase 4; T004 for every new field, T005 for every new call. For the preview check: T008, T011, T015, T016, T017, T018, T021 merged.

**Independent Test**: a pending ask is answered from the Tarea tab without opening the Bandeja. **Boards**: Cabecera de tarea › Main, Estados (Antes for reference); Pantallas › Tarea; Diálogos y estados › TareaSetup; TareaHecha, TareaCaida, TareaLibre when drawn.

- [ ] T057 [P] [US3] History sentences `app/src/history.ts` (R22) with tests first: every core event name (`crates/delilah-core/src/task.rs` `Event::name`, incl. `reopened`, `runtime-switched`, `permission-expired`, `setup-failed`, `autopilot-*`) → a human sentence; unknown → generic sentence, never the slug.
  - **Files**: `app/src/history.ts`, `app/src/history.test.ts`, `app/src/i18n/en/history.ts`
  - **Acceptance**: vitest green. **Depends**: T029.
- [ ] T058 [P] [US3] Shared `AskCard` (brief §2, §5 Bandeja): one component for Pregunta (numbered options, free reply, Enviar ⌘↵), Permiso (mono command, «Permitir una vez» / «Permitir siempre en esta tarea» (only with a rule) / «Denegar», no free text) and Aviso (its action + «Quitar» when not blocking); variants `inbox | inline | chat`; ended permission outcomes (incl. «Caducada · se respondió en la terminal»). `PermissionCard.tsx` becomes a thin wrapper over it (deleted in T096). Move the cases of `PermissionCard.test.tsx` into `AskCard.test.tsx` first.
  - **Files**: `app/src/components/AskCard.tsx`, `AskCard.css`, `AskCard.test.tsx`, `app/src/components/PermissionCard.tsx`, `PermissionCard.test.tsx`, `app/src/i18n/en/AskCard.ts`
  - **Acceptance**: vitest green. **Board**: Pantallas › Bandeja, Tarea. **Depends**: T004, T031, T033, T037.
- [ ] T059 [P] [US3] `app/src/components/AutopilotControls.tsx`: Autopiloto and Ahorro `Toggle`s; turning autopilot on opens a confirmation popover («Activar autopiloto», runs alone until it opens the PRs and uses quota, Ahorro stays on; Cancelar · Activar); while on, Ahorro forced on + disabled with the reason; paused → «Autopiloto en pausa · Reanudar» + reason («límite de 5 h · se reanuda a las 18:40»); Ahorro tooltip with decision 33 wording; `iconOnly` for compact. Tests first (cases moved from `AutopilotMark.test.tsx`; `AutopilotMark.tsx` stays for the board until T073).
  - **Files**: `app/src/components/AutopilotControls.tsx`, `AutopilotControls.css`, `AutopilotControls.test.tsx`, `app/src/i18n/en/AutopilotControls.ts`
  - **Acceptance**: vitest green. **Board**: Cabecera de tarea › Main, Estados. **Depends**: T036, T042.
- [ ] T060 [P] [US3] `app/src/components/SessionViewSwitch.tsx` (R14, FR-024): `Segmented` pressed «Chat» (tag «beta») | «Terminal» for the open session; PTY + Chat/Terminal → view callback (`chooseView`); stream session → Terminal, or PTY → chat mode: `ConfirmDialog` then `api.setSessionRuntime` (mapping chat↔stream, terminal↔pty as contracts/api.md; reads `runtime` for what runs now and `effective_runtime` for what the next launch uses); disabled with reason mid-turn. Tests first.
  - **Files**: `app/src/components/SessionViewSwitch.tsx`, `SessionViewSwitch.css`, `SessionViewSwitch.test.tsx`, `app/src/i18n/en/SessionViewSwitch.ts`
  - **Acceptance**: vitest green. **Board**: Cabecera de tarea › Main. **Depends**: T004, T005, T037, T042.
- [ ] T061 [P] [US3] `app/src/components/TaskTabs.tsx` (FR-026): tablist Tarea|Proyecto · session tabs (dot) · file tabs (mono, closable) · Cambios (closable, `Count`) · PR; dashed «+ Sesión» `Menu` (Claude ⌘⇧T / Terminal ⌘T, from `shortcuts.ts`); overflow scrolls tabs, «+ Sesión» stays visible. Tests first.
  - **Files**: `app/src/components/TaskTabs.tsx`, `TaskTabs.css`, `TaskTabs.test.tsx`, `app/src/i18n/en/TaskTabs.ts`
  - **Acceptance**: vitest green. **Board**: Pantallas › Tarea. **Depends**: T040, T043.
- [ ] T062 [US3] `app/src/components/TaskHeader.tsx` (FR-022): key (mono, `white-space: nowrap`, never shrinks) · editable title (`flex: 1`, ellipsis; click → input; empty refused; «Antes: <old> · Restaurar» from `previous_title`, logic as `TaskScreen.tsx:594-599`) · `StatusBadge` · `PhaseBars` (a button opening the phase popover: a `Menu` listing the 4 phases with the current one checked; choosing one calls `api.setPhase`, as decision 17) · `SessionViewSwitch` slot · `AutopilotControls` · `PreviewControl` (restyled: «Preview» off / «Preview :1503» + ok dot, menu Abrir · Detener) · tree toggle (`IconButton`). Done → Hecha + `PrChip` + relative date (latest `mark-done`/`agent-finished`), no bars; setup-failed → «Sin iniciar». Tests first in `TaskHeader.test.tsx`; update `PreviewControl.test.tsx`.
  - **Files**: `app/src/components/TaskHeader.tsx`, `TaskHeader.css`, `TaskHeader.test.tsx`, `app/src/components/PreviewControl.tsx`, `PreviewControl.test.tsx`, `app/src/i18n/en/TaskHeader.ts`
  - **Acceptance**: vitest green; a 200-char title never wraps the key or pushes the controls off. **Board**: Cabecera de tarea › Main, Estados; TareaHecha. **Depends**: T032, T034, T035, T043, T059, T060.
- [ ] T063 [US3] Integrate in `app/src/components/TaskScreen.tsx`: use `TaskHeader` and `TaskTabs`; `SessionChain` row and the base-stale notice as thin neutral bars under the header («main avanzó N commits · Actualizar rama», today `:657-668`); project variant (`@name`) shows «Proyecto» rendering `ProjectView`; remove `.view-switch` from the tab row (`:708-728`), «Probar modo chat (experimental)» (`:827-831`), the experimental pill (`:603`), «Desactivar» buttons (`:605-608`, `:614`), `saving-on` reuse (`:622`, `:627`), `.saving-pill`, in-tab «+ Claude/+ Terminal»; tree auto-hidden on Cambios; inline styles removed except `--tree-w`. Update `TaskScreen.test.tsx`, `SessionChain.test.tsx`, `TreeResizer.test.tsx` first.
  - **Files**: `app/src/components/TaskScreen.tsx`, `TaskScreen.css`, `TaskScreen.test.tsx`, `app/src/components/SessionChain.tsx`, `SessionChain.test.tsx`, `app/src/components/TreeResizer.tsx`, `TreeResizer.test.tsx`, `app/src/i18n/en/TaskScreen.ts`
  - **Acceptance**: vitest green; `inline-style.test.ts` number for TaskScreen = 1. **Board**: Cabecera de tarea › Main, Estados; Pantallas › Tarea. **Depends**: T061, T062.
- [ ] T064 [P] [US3] Tarea tab `app/src/components/TaskView.tsx` (FR-027, FR-028, FR-036): no stepper; meta line (proyecto · flujo · rama mono); «Marcar como hecha» (secondary) → `api.done` + toast «Deshacer» → `api.reopen` (if the reopen is refused, e.g. archived meanwhile, one human error); «Más» `Menu` (Abrir cambios, Archivar workspace, Eliminar tarea… via ConfirmDialog); pending ask inline with `AskCard` variant `inline`; Sesiones rows (dot, name, kind, Abrir); Tareas de la spec collapsible by phase + progress + «Abrir tasks.md» (`SpecPanel.tsx`); Historial with `history.ts` + `time.ts`; failed/crashed shows the reason and «Reanudar» (TareaCaida); free workflow with several Claude sessions lists them all (TareaLibre); `HandoffActions` restyled; skeletons while loading. Update `TaskView.test.tsx`, `HandoffActions.test.tsx` first.
  - **Files**: `app/src/components/TaskView.tsx`, `TaskView.css`, `TaskView.test.tsx`, `app/src/components/SpecPanel.tsx`, `app/src/components/HandoffActions.tsx`, `HandoffActions.test.tsx`, `app/src/i18n/en/TaskView.ts`
  - **Acceptance**: vitest green; a question pending on the task is answered from this tab (SC-005); the «Deshacer» toast disappears as soon as the task's workspace is archived (no reopen offered after that). **Board**: Pantallas › Tarea; TareaCaida, TareaLibre when drawn. **Depends**: T004, T005, T041, T043, T044, T057, T058.
- [ ] T065 [P] [US3] Chat tab `app/src/components/ConversationView.tsx` + `ToolCard.tsx` (FR-030, FR-031): `role="log"` «Conversación»; tool calls collapsed to one line (`role="group"` named by summary); permission cards live and ended (outcome, decision, command summary) via `AskCard` variant `chat`; composer «Enviar» / «Detener»; after «Detener» the transcript line reads «Detenida: detuviste a Claude.» (glossary, brief §3); skeleton while loading. Update `ConversationView.test.tsx`, `ToolCard.test.tsx` first.
  - **Files**: `app/src/components/ConversationView.tsx`, `ConversationView.css`, `ConversationView.test.tsx`, `app/src/components/ToolCard.tsx`, `ToolCard.css`, `ToolCard.test.tsx`, `app/src/i18n/en/ConversationView.ts`
  - **Acceptance**: vitest green. **Board**: Pantallas › Tarea (chat). **Depends**: T004, T041, T058.
- [ ] T066 [P] [US3] Cambios `app/src/components/ChangesView.tsx` (FR-032): files grouped by folder with a filter, `Skeleton` while loading (empty text only after load), «Recargar»; `DiffRows` on diff tokens (`--add-bg/-tx`, `--del-bg/-tx`, `--hunk-bg`); `FileTree.tsx` refreshes after commits and says «0 archivos» through `tn`. Tests first: new `ChangesView.test.tsx`, update `DiffRows.test.tsx`.
  - **Files**: `app/src/components/ChangesView.tsx`, `ChangesView.css`, `ChangesView.test.tsx` (new), `app/src/components/DiffRows.tsx`, `DiffRows.test.tsx`, `app/src/components/FileTree.tsx`, `app/src/i18n/en/ChangesView.ts`
  - **Acceptance**: vitest green. **Board**: Pantallas › Tarea (Cambios). **Depends**: T041.
- [ ] T067 [P] [US3] PR tab `app/src/components/PrView.tsx` + `OpenPrDialog.tsx` (FR-033, FR-034): one friendly error with actions (no `gh` session → «gh auth login», «Abrir una terminal», «Recargar»; never raw stderr twice); «Abrir PR» disabled with reason; checks list (`role="list"` «Checks»; running neutral with running time; finished with duration); review comments (author, file, text, relative time); «Abrir en GitHub»; «Fusionar» (`api.mergePr`, method = first of `merge_methods`, shown) disabled with reason until `ci === "success"`, and with `merge_methods: []` disabled with «No se pudo saber qué métodos de fusión permite el repositorio»; clicking it asks first in a `ConfirmDialog` (PR number, method, base branch; Cancelar · Fusionar); `OpenPrDialog` on `Dialog`. Update `PrView.test.tsx`, `OpenPrDialog.test.tsx` first.
  - **Files**: `app/src/components/PrView.tsx`, `PrView.css`, `PrView.test.tsx`, `app/src/components/OpenPrDialog.tsx`, `OpenPrDialog.test.tsx`, `app/src/i18n/en/PrView.ts`
  - **Acceptance**: vitest green; after a 409 on merge (e.g. checks turned failing) the tab shows the reason once and refreshes the PR (`api.taskPrs`). **Board**: Pantallas › Tarea (PR). **Depends**: T004, T005, T029, T034, T042.
- [ ] T068 [P] [US3] Setup failed panel `app/src/components/SetupBanner.tsx` (FR-035): `api.setupInfo` → exit code, duration, command (mono), output tail; «Reintentar setup», «Iniciar sin setup», «Abrir el log completo» (`api.setupLogUrl`, opened with `openExternal`), link to the project's setup command (opens project settings); empty log says so. Update `SetupBanner.test.tsx` first.
  - **Files**: `app/src/components/SetupBanner.tsx`, `SetupBanner.css`, `SetupBanner.test.tsx`, `app/src/i18n/en/SetupBanner.ts`
  - **Acceptance**: vitest green. **Board**: Diálogos y estados › TareaSetup. **Depends**: T004, T005, T031.
- [ ] T069 [US3] E2E selectors for the task screen (contracts/ui.md table): `.view-switch`, `.view-switch-wrap`, `.conv`, `.tool-card`, `.perm`, `.perm-rule`, `.pr-view .pr-card`, `.pr-checks li`, `.pr-ci-failure`, «Chat»/«Terminal» (pressed buttons in the header), «Principal|Main» → «Claude principal»/«Main Claude», «Ver cambios» → «Abrir cambios».
  - **Files**: `app/e2e/chat.spec.ts`, `app/e2e/conversation.spec.ts`, `app/e2e/pr.spec.ts`, `app/e2e/screens.spec.ts`
  - **Acceptance**: `scripts/e2e.sh ui` green. **Depends**: T063–T068.
- [ ] T070 [US3] New UI flow `app/e2e/task-screen.spec.ts` (English UI): a task whose fake asks a question is answered from the Task tab without visiting the Inbox, then status shows «Working»; «Mark as done» → «Undo» → status «Waiting for you»; a long title keeps the key on one line (bounding boxes).
  - **Files**: `app/e2e/task-screen.spec.ts` (new)
  - **Acceptance**: `scripts/e2e.sh ui` green. **Depends**: T064, T011.

**Checkpoint**: checks green; compare boards; spec-compliance + test review of Phases 2–5; commit `feat(ui-v2): step 3 — task screen`.

---

## Phase 6: User Stories 4 and 5 — Bandeja, board, first run, Jira (Priority: P2) · brief §8 step 4

**Goal**: FR-037–FR-047. **Depends**: Phase 5 (AskCard); T004, T005. For the preview check: T013, T014, T019, T020 merged.

**Independent Test**: «Limpiar avisos» never removes a blocking item and «Deshacer» restores the rest. **Boards**: Pantallas › Bandeja, Tablero, Jira; Diálogos y estados › PrimerUso.

- [ ] T071 [P] [US4] Bandeja `app/src/components/InboxView.tsx` (FR-037–FR-041): list (380, `role="listbox"`, items `option`) + detail; `InboxKind`; relative times; `AskCard` variant `inbox` per kind; «Quitar» → `api.dismissInbox`; «Limpiar avisos» → `api.clearInbox` + toast «Deshacer» → `api.restoreInbox(ids)` (disabled with reason when only blocking items); keys ↑↓, 1–3, ⌫ only outside text fields + hint line; empty «Nada te espera» + the latest activity line, derived client-side (newest `last_activity_at` + `last_message` across `/sessions`, else the newest task event; refreshed on `Topic::Sessions` / `Topic::Tasks`; no route); skeleton while loading; new kinds `turn-done`, `pr-opened` with «Abrir la tarea» / «Abrir PR». Update `InboxView.test.tsx` first.
  - **Files**: `app/src/components/InboxView.tsx`, `InboxView.css`, `InboxView.test.tsx`, `app/src/i18n/en/InboxView.ts`
  - **Acceptance**: vitest green (incl. «clear never sends a blocking id»). **Board**: Pantallas › Bandeja. **Depends**: T004, T005, T029, T033, T041, T044, T058.
- [ ] T072 [P] [US5] First run `app/src/components/FirstRun.tsx` (FR-044): checklist «Añadir un proyecto» → «Conectar Jira (opcional)» (with «Omitir») → «Crear tu primera tarea», each marked from projects/Jira/tasks; state in preference `first-run` (`api.setPreference`/get); shown when there are no projects; hidden once all 3 steps are done (Jira connected or skipped) or dismissed. Tests first.
  - **Files**: `app/src/components/FirstRun.tsx`, `FirstRun.css`, `FirstRun.test.tsx`, `app/src/i18n/en/FirstRun.ts`
  - **Acceptance**: vitest green; «Omitir» survives a reload. **Board**: Diálogos y estados › PrimerUso. **Depends**: T031.
- [ ] T073 [US5] Board `app/src/components/BoardView.tsx` (FR-042, FR-043): columns Backlog + 4 phases + Hecha (`phases.ts`), flexible, empty phases folded to 40 px; project filter (Todos / per project) and `statusFilter` prop (from the statusbar, clearable); primary «Nueva tarea»; cards (`role="link"` «KEY title»): key, `StatusBadge` (always text), 2-line title, project, mini `PhaseBars` (none for Backlog/Hecha), `PrChip`, autopilot marker (a small autopilot icon with its state in the tooltip, from `Icon.tsx`; not `AutopilotControls`); no highlight borders; mounts `FirstRun` when there are no projects (it hides itself once its 3 steps are done); skeleton while loading. Tests first in a new `BoardView.test.tsx` (+ `phases.test.ts` if columns change).
  - **Files**: `app/src/components/BoardView.tsx`, `BoardView.css`, `BoardView.test.tsx` (new), `app/src/phases.ts`, `app/src/phases.test.ts`, `app/src/i18n/en/BoardView.ts`
  - **Acceptance**: vitest green. **Board**: Pantallas › Tablero. **Depends**: T032, T034, T035, T041, T072.
- [ ] T074 [P] [US5] Jira `app/src/components/JiraView.tsx` (FR-045–FR-047): filter chips (De quién, Estado, Sprint) as `Menu`s, «Agrupar por» `Segmented` radio Estado | Épica (grouping by `status` name); two-line rows (key + full title / epic · sprint · subtareas N/M); avatar column only when «De quién» ≠ «Asignados a mí»; priority only Alta/Crítica (icon + word); detail «Abrir en Jira ↗», linked task row (phase · session, status, «Abrir ›») else primary «Analizar con Claude» + one line; fields, description; subtasks via `api.jiraSubtasks` grouped by status with progress; empty «Ningún ticket con estos filtros» + «Quitar filtros»; connect form Cloud / Server / Datos de ejemplo; translate «sample data», «Filter tickets…», «Connection», «Assigned/Open/All», «Analyse with Claude»; skeleton. Tests first in a new `JiraView.test.tsx`.
  - **Files**: `app/src/components/JiraView.tsx`, `JiraView.css`, `JiraView.test.tsx` (new), `app/src/i18n/en/JiraView.ts`
  - **Acceptance**: vitest green. **Board**: Pantallas › Jira. **Depends**: T004, T005, T032, T037, T041, T043.
- [ ] T075 [US4] E2E Bandeja: `app/e2e/inbox.spec.ts` `.inbox-item` → `option`; add a case: with a blocking question and two avisos, «Clear notices» leaves the question and «Undo» restores both (SC-006).
  - **Files**: `app/e2e/inbox.spec.ts`
  - **Acceptance**: `scripts/e2e.sh ui` green. **Depends**: T071, T013.
- [ ] T076 [US5] E2E board: `app/e2e/board.spec.ts` (`.board .kcard`, `.kcard-status`, `.pr-chip`, `.pr-meta` → roles/text) and the board/inbox/Jira shots of `app/e2e/screens.spec.ts`.
  - **Files**: `app/e2e/board.spec.ts`, `app/e2e/screens.spec.ts`
  - **Acceptance**: `scripts/e2e.sh ui` green. **Depends**: T073, T074.

**Checkpoint**: checks green; boards compared; commit `feat(ui-v2): step 4 — Bandeja, board and first run, Jira`.

---

## Phase 7: User Story 6 — Nueva tarea, backlog, projects, settings, dialogs, checkout default (Priority: P2) · brief §8 step 5

**Goal**: FR-048–FR-055, FR-049 (checkout default). **Depends**: Phase 6; T004; T009 (daemon default) for the preview check; T022 for Diagnóstico.

**Independent Test**: the Nueva tarea height never changes and «Iniciar» is always visible. **Boards**: Diálogos y estados › NuevaTarea, Eliminar, QuickOpen; Pantallas › BacklogTarea, Proyectos, ProyectoVista, Ajustes; Conversacion.

- [ ] T077 [US6] `app/src/components/WherePicker.tsx` (R9): `OptionCard` radios «En tu checkout» («por defecto») / «Aislado (worktree)»; `whereOf` fallback `"checkout"` (`:11-12`) and doc comment; `withDefault` option «Por defecto (en tu checkout)» (`:30`). Tests first in a new `WherePicker.test.tsx`.
  - **Files**: `app/src/components/WherePicker.tsx`, `WherePicker.css`, `WherePicker.test.tsx` (new), `app/src/i18n/en/WherePicker.ts`
  - **Acceptance**: vitest green. **Board**: NuevaTarea. **Depends**: T038.
- [ ] T078 [US6] Nueva tarea `app/src/components/NewTaskDialog.tsx` (FR-048): `Dialog` 600×700, scrolling body, sticky footer «Cancelar · Guardar en backlog · Iniciar» (replaces «Lanzar»/«Empezar», `:281`, `:477`); origin `Segmented` radio Ticket de Jira | Tarea libre | Con Claude; `WherePicker`; `AutopilotControls` (same confirmation); «Opciones avanzadas»: Chat | Terminal `Segmented` (Chat «beta», default `chat` as `:61`) replacing the «Vista» select (`:363-370`) and the chat-mode checkbox (`:371-376`), Rama, Primer mensaje. When starting in the checkout is refused because it is dirty, show one human error «Tu checkout tiene cambios sin guardar…» with the action «Iniciar en un worktree» (retries with `in_checkout: false`) and «Abrir una terminal», never the raw daemon message. Update `NewTaskDialog.test.tsx` and `NewTaskDialog.project-settings.test.tsx` first (labels, checkout default).
  - **Files**: `app/src/components/NewTaskDialog.tsx`, `NewTaskDialog.css`, `NewTaskDialog.test.tsx`, `NewTaskDialog.project-settings.test.tsx`, `app/src/i18n/en/NewTaskDialog.ts`
  - **Acceptance**: vitest green; dialog box height identical across every origin/advanced state (SC-007). **Board**: NuevaTarea. **Depends**: T037, T042, T059, T077.
- [ ] T079 [P] [US6] Con Claude proposal step `app/src/components/ProposalCard.tsx` per the Conversacion board (restyle on the kit; «Crear», «Crear e iniciar en autopiloto» wording with «Iniciar»). Update `ProposalCard.test.tsx` first.
  - **Files**: `app/src/components/ProposalCard.tsx`, `ProposalCard.css`, `ProposalCard.test.tsx`, `app/src/i18n/en/ProposalCard.ts`
  - **Acceptance**: vitest green. **Board**: Conversacion (when drawn). **Depends**: T031.
- [ ] T080 [P] [US6] Backlog task `app/src/components/BacklogTaskView.tsx` (FR-050): Nueva tarea form language; «Iniciar» (primary), «Guardar», «Eliminar…» (danger, far right, ConfirmDialog); «Creada ayer» from `created_at` (`time.ts`); `w ?? "checkout"` (`:87`); after «Iniciar» shows Trabajando + toast «<KEY> iniciada · Abrir»; a dirty-checkout refusal shows the same human error and actions as Nueva tarea. Update `BacklogTaskView.test.tsx` first.
  - **Files**: `app/src/components/BacklogTaskView.tsx`, `BacklogTaskView.css`, `BacklogTaskView.test.tsx`, `app/src/i18n/en/BacklogTaskView.ts`
  - **Acceptance**: vitest green. **Board**: Pantallas › BacklogTarea. **Depends**: T004, T029, T042, T044, T077.
- [ ] T081 [P] [US6] Project settings `app/src/components/ProjectSettings.tsx` (FR-052): real `Dialog`, help text for every field, «Comando de preview» (saved with `updateProject` + `updateProjectSettings`, two calls), workspace via `WherePicker` with the new default. Update `ProjectSettings.test.tsx` first (default assertion).
  - **Files**: `app/src/components/ProjectSettings.tsx`, `ProjectSettings.css`, `ProjectSettings.test.tsx`, `app/src/i18n/en/ProjectSettings.ts`
  - **Acceptance**: vitest green. **Board**: Pantallas › Proyectos. **Depends**: T042, T077.
- [ ] T082 [P] [US6] Proyectos `app/src/components/ProjectsView.tsx` + new `AddProjectDialog.tsx` (FR-051): table (name with kind icon and «Repo · N tareas activas», Dónde trabaja, Carpeta mono, actions); «Añadir proyecto» opens the dialog (the add form moves there; also opened from the sidebar «+»); row «Ajustes» + ⋯ `Menu` (Renombrar via `updateProject({name})`, Quitar…); Quitar `ConfirmDialog` with `blocked` note + disabled when a task holds a workspace (from tasks; the daemon's 409 shown the same way); rows open the project screen; danger buttons replace inline `color: var(--err)` (`:117`, `:219`). Tests first (new `ProjectsView.test.tsx`, `AddProjectDialog.test.tsx`).
  - **Files**: `app/src/components/ProjectsView.tsx`, `ProjectsView.css`, `ProjectsView.test.tsx` (new), `app/src/components/AddProjectDialog.tsx` (new), `AddProjectDialog.test.tsx` (new), `app/src/i18n/en/ProjectsView.ts`
  - **Acceptance**: vitest green. **Board**: Pantallas › Proyectos. **Depends**: T030, T042, T043.
- [ ] T083 [P] [US6] Project view `app/src/components/ProjectView.tsx` (FR-053): folder icon + name + path once; «Preguntar a Claude» (primary), «Nueva tarea», «Terminal»; Conversaciones / Ideas rows with `last_message` + relative `last_activity_at`; Tareas activas; Comando de preview with help + «Guardar»; README preview; file tree (`@name` owner). Update `ProjectView.test.tsx` first.
  - **Files**: `app/src/components/ProjectView.tsx`, `ProjectView.css`, `ProjectView.test.tsx`, `app/src/i18n/en/ProjectView.ts`
  - **Acceptance**: vitest green. **Board**: Pantallas › ProyectoVista. **Depends**: T004, T029, T039.
- [ ] T084 [P] [US6] Ajustes `app/src/components/SettingsView.tsx` + `app/src/useUpdates.ts` (FR-054): section index (General, Preguntas, Modo ahorro, Actualizaciones, Atajos de teclado, Diagnóstico), content ≤ 720 px; «Por defecto» only on changed shortcuts, «Restaurar todos»; Modo ahorro wording (decision 33); «Buscar actualizaciones automáticamente» (preference `auto-update`; `useUpdates.ts` skips scheduled checks when false, manual check unchanged); Diagnóstico: «Abrir Logs», daemon version, data folder (`health.home`), «Mostrar en Finder» (`revealInFinder`, desktop only); «REMOTE CONTROL» → «Control remoto». Update `SettingsView.test.tsx`, `useUpdates.test.ts` first.
  - **Files**: `app/src/components/SettingsView.tsx`, `SettingsView.css`, `SettingsView.test.tsx`, `app/src/useUpdates.ts`, `app/src/useUpdates.test.ts`, `app/src/i18n/en/SettingsView.ts`
  - **Acceptance**: vitest green. **Board**: Pantallas › Ajustes. **Depends**: T004, T022, T037.
- [ ] T085 [P] [US6] Delete task `app/src/components/DeleteTaskDialog.tsx` on `ConfirmDialog` (one destructive pattern). Update `DeleteTaskDialog.test.tsx` first.
  - **Files**: `app/src/components/DeleteTaskDialog.tsx`, `DeleteTaskDialog.test.tsx`, `app/src/i18n/en/DeleteTaskDialog.ts`
  - **Acceptance**: vitest green. **Board**: Diálogos y estados › Eliminar. **Depends**: T042.
- [ ] T086 [P] [US6] Quick Open `app/src/components/QuickOpen.tsx`: on `Dialog`, files only, matched characters bold, key hints; «Search N files…» through `tn`. Tests first in a new `QuickOpen.test.tsx`.
  - **Files**: `app/src/components/QuickOpen.tsx`, `QuickOpen.css`, `QuickOpen.test.tsx` (new), `app/src/i18n/en/QuickOpen.ts`
  - **Acceptance**: vitest green. **Board**: Diálogos y estados › QuickOpen. **Depends**: T042.
- [ ] T087 [US6] E2E selectors for dialogs and projects: «Start», «New task» (dialog), «Free task» (radio), «Project» (combobox) in every spec that creates tasks; `app/e2e/projects.spec.ts` (add-project dialog, row menu); `app/e2e/quick-open.spec.ts`; `app/e2e/screens.spec.ts` («Empezar» → «Iniciar», delete through the sidebar row's context menu → `ConfirmDialog`).
  - **Files**: `app/e2e/projects.spec.ts`, `app/e2e/quick-open.spec.ts`, `app/e2e/screens.spec.ts`, `app/e2e/fixtures.ts` (if it holds the create-task helper)
  - **Acceptance**: `scripts/e2e.sh ui` green. **Depends**: T078–T086.
- [ ] T088 [US6] New UI flow `app/e2e/new-task.spec.ts`: the dialog's bounding box is the same for each origin and with advanced options open, «Start» in the viewport (SC-007); a free task created with no workspace choice in a project with no setting reports the checkout (task API `in_checkout`/workspace path); with an uncommitted file in that checkout, «Start» shows the dirty-checkout error and «Start in a worktree» starts it in a worktree.
  - **Files**: `app/e2e/new-task.spec.ts` (new)
  - **Acceptance**: `scripts/e2e.sh ui` green. **Depends**: T078, T009.

**Checkpoint**: checks green; boards compared; spec-compliance review of Phases 6–7; commit `feat(ui-v2): step 5 — dialogs, projects, settings, backlog, checkout by default`.

---

## Phase 8: User Story 7 — Widths, scale, loading, global notices (Priority: P3) · brief §8 step 6

**Goal**: FR-056–FR-059. **Depends**: Phase 7.

**Independent Test**: at 1024 nothing scrolls horizontally except the board columns. **Boards**: Anchos y escala › Tarea1280, Tablero1280, Jira1024, EscalaLateral, EscalaJira; Cargando; AvisosGlobales.

- [ ] T089 [US7] Sidebar at scale `app/src/components/Sidebar.tsx`: first 6 active tasks per project + «N más» (expands), «Filtrar tareas» field filtering every project; collapsed counts in the rail. Update `Sidebar.test.tsx` first.
  - **Files**: `app/src/components/Sidebar.tsx`, `Sidebar.css`, `Sidebar.test.tsx`, `app/src/i18n/en/Sidebar.ts`
  - **Acceptance**: vitest green (15 tasks → 6 + «9 más»). **Board**: Anchos y escala › EscalaLateral.
- [ ] T090 [P] [US7] Bandeja at scale/width `app/src/components/InboxView.tsx`: group items by task with a count; below 1200 (`useBreakpoints().overlays`) the detail opens as an overlay (Esc closes). Update `InboxView.test.tsx` first.
  - **Files**: `app/src/components/InboxView.tsx`, `InboxView.css`, `InboxView.test.tsx`, `app/src/i18n/en/InboxView.ts`
  - **Acceptance**: vitest green. **Board**: Anchos y escala (inbox), Pantallas › Bandeja.
- [ ] T091 [P] [US7] Jira at 1024 `app/src/components/JiraView.tsx`: detail as a drawer below 1200, filters collapse to «Filtros · N». Update `JiraView.test.tsx` first.
  - **Files**: `app/src/components/JiraView.tsx`, `JiraView.css`, `JiraView.test.tsx`, `app/src/i18n/en/JiraView.ts`
  - **Acceptance**: vitest green. **Board**: Anchos y escala › Jira1024, EscalaJira.
- [ ] T092 [P] [US7] Task screen at 1280/960: `TaskHeader.tsx` compact when the main area (`useContainerWidth`) is ≤ 960 (bars only, label in tooltip; toggles `iconOnly`); `TaskScreen.tsx` hides the tree below 1300 (user can still open it). Update `TaskHeader.test.tsx`, `TaskScreen.test.tsx` first.
  - **Files**: `app/src/components/TaskHeader.tsx`, `TaskHeader.css`, `TaskHeader.test.tsx`, `app/src/components/TaskScreen.tsx`, `TaskScreen.css`, `TaskScreen.test.tsx`
  - **Acceptance**: vitest green. **Board**: Anchos y escala › Tarea1280.
- [ ] T093 [P] [US7] Board at 1280: columns min 220 px with horizontal scroll inside the board only.
  - **Files**: `app/src/components/BoardView.css`
  - **Acceptance**: covered by T095. **Board**: Anchos y escala › Tablero1280.
- [ ] T094 [US7] Global notices: new `app/src/components/GlobalNotices.tsx` (quota notice from `usage` ≥ 90 %, autopilot pause reasons across tasks; only existing `/usage` and `/tasks` data, no new route) and `UpdateNotice.tsx` restyled as the update banner, toasts on the same style; mounted in `app/src/App.tsx`. Update `UpdateNotice.test.tsx`, tests first for `GlobalNotices.test.tsx`.
  - **Files**: `app/src/components/GlobalNotices.tsx` (new), `GlobalNotices.css`, `GlobalNotices.test.tsx` (new), `app/src/components/UpdateNotice.tsx`, `UpdateNotice.test.tsx`, `app/src/App.tsx`, `app/src/i18n/en/GlobalNotices.ts`
  - **Acceptance**: vitest green. **Board**: AvisosGlobales (when drawn).
- [ ] T095 [US7] New UI flow `app/e2e/widths.spec.ts`: at 1024×768 visit every screen and assert `document.scrollingElement.scrollWidth <= innerWidth` (the board columns' own scroller excepted); below 1200 the inbox and Jira details are overlays; at 1280 the sidebar is a rail.
  - **Files**: `app/e2e/widths.spec.ts` (new)
  - **Acceptance**: `scripts/e2e.sh ui` green (SC-008). **Depends**: T089–T094.

**Checkpoint**: checks green; boards compared at 1280 and 1024; commit `feat(ui-v2): step 6 — widths, scale and global notices`.

---

## Phase 9: Polish, record, docs, release

- [ ] T096 Remove what v2 replaced: `PhaseBadge.tsx`/`.test.tsx`, `PrBadge.tsx`/`.test.tsx`, `AutopilotMark.tsx`/`.test.tsx`, `PermissionCard.tsx`/`.test.tsx`, `components/status.ts` (callers already moved; fix any leftover import), and the old classes in `app/src/styles.css` (contracts/ui.md «Removed at the end»); set `app/src/inline-style.test.ts` to the final allow-list (dynamic values only).
  - **Files**: the files above, `app/src/styles.css`, `app/src/inline-style.test.ts`
  - **Acceptance**: tsc + `npx vitest run` green; SC-001, SC-002.
- [ ] T097 i18n cleanup: delete orphan keys from `app/src/i18n/en/legacy.ts`, set `STRICT_ORPHANS = true` in `app/src/i18n/coverage.test.ts`; run the English dev build through every screen with no `[i18n] sin traducir`, and the Spanish one with no English outside product names and log lines.
  - **Files**: `app/src/i18n/en/legacy.ts`, `app/src/i18n/coverage.test.ts`
  - **Acceptance**: vitest green; SC-010.
- [ ] T098 [P] Seed: `scripts/seed-preview.sh` also creates (with the fake `gh` where needed) a task waiting on a question, a chat-mode task with a pending permission («Te espera»), a setup failure (project with setup `exit 3`), a done task with a merged PR, a paused task, a backlog task created yesterday-ish, inbox avisos of each kind, and a Jira sample connection; header comment lists them. (Plain `failed` has no fake path: note it.)
  - **Files**: `scripts/seed-preview.sh`
  - **Acceptance**: a `--fresh` preview seeded with it shows every state of the boards.
- [ ] T099 Full checks: `cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace`, the shell's fmt/clippy/test, `cd app && npx tsc --noEmit && npx tsc --noEmit -p e2e && npx vitest run`, `scripts/e2e.sh` (api + ui); `grep -rn "not_implemented()" crates/delilah-daemon/src` finds no stub left. Fix fallout only in the files of the failing area.
  - **Files**: as needed by failures
  - **Acceptance**: SC-013.
- [ ] T100 Before/after record (brief §9): `cargo build -p delilah-daemon -p delilah-cli -p delilah-e2e && cd app && DELILAH_SCREENS=despues npx playwright test screens`; fix only selectors in `app/e2e/screens.spec.ts` (roles/text), never framing or data; commit the 46 images.
  - **Files**: `app/e2e/screens.spec.ts`, `docs/redesign/despues/*.png`
  - **Acceptance**: 46 images with the same names as `docs/redesign/antes/` (SC-012). Tell the owner (for the canvas «Antes y después» page) in the PR description.
- [ ] T101 [P] Decisions in `docs/decisions.md`: checkout is the default (next to 16, and a note on 57 whose «else a worktree» it reverses; `WherePicker` label); a pending permission makes the task «Te espera»; Chat | Terminal per session (`sessions.runtime` override; the task's chat mode is the default); inbox removals by computed id + restore, with `inbox_dismissals` as the single source of truth (notice reads join it); the two new avisos (secondary turn finished, PR opened) stored in `pr_notices`, reused on purpose (a rename to a neutral `notices` table is pending in status.md); reopen after done (`Done → WaitingForUser`); the glossary of brief §3; per-component CSS and English files; «Ver N más» → «N más».
  - **Files**: `docs/decisions.md`
  - **Acceptance**: each entry says what, why and what was ruled out.
- [ ] T102 [P] `docs/status.md` (built: UI v2, the §6 data; pending: colour-blind mode, boards still being drawn), `README.md` (user-facing behaviour: statusbar, disconnected state, Logs button, rail ⌘\, inline answers, undo done, merge, clear avisos, first run, checkout by default, auto-update preference), and the «Status: design only» line of `docs/ui-redesign.md`.
  - **Files**: `docs/status.md`, `README.md`, `docs/ui-redesign.md`
  - **Acceptance**: doc-checker reports nothing stale.
- [ ] T103 [P] `docs/architecture.md` (status table: `Reopen`, permission asks → `AgentAsked`/`UserAnswered`; modules `store_inbox`, `pr_api/merge`; `setup.json` recorded as a file-based exception to the database (lifecycle: written at the end of each setup run, overwritten on retry, removed with the task dir on delete); sessions columns (`runtime` vs `effective_runtime`); the permission reconcile at start; UI section: `components/ui/`, `vocab.ts`, `theme.ts`, `connection.ts`, per-component CSS and `i18n/en/`) and `docs/development.md` (Conventions: CSS and English per component; the guards).
  - **Files**: `docs/architecture.md`, `docs/development.md`
  - **Acceptance**: doc-checker reports nothing stale.
- [ ] T104 Version bump, last commit on the branch: `scripts/bump-version.sh minor --commit` (from `origin/main`; re-check if `main` moved) → `chore: bump version to X.Y.Z`. The PR says **minor** and why: new user-facing features, additive API and migrations; and it states that the checkout default changes what `POST /runs` and `delilah run`/`new` do without a workspace flag, why that is not **major** (no request or response shape breaks, no data migration, saved choices kept; the user is told in the release notes), plus the native-only manual checks below.
  - **Files**: `Cargo.toml`, `app/src-tauri/Cargo.toml`, `app/package.json`, `app/src-tauri/tauri.conf.json`, lockfiles (by the script)
  - **Acceptance**: the four versions agree; `node scripts/release/check.mjs` passes. **Depends**: T096–T103.

**Checkpoint**: contract review (final), spec-compliance review against spec.md; PR open with the level and the before/after note.

---

## Dependencies & Execution Order

### Phase dependencies

- **Phase 1** → everything. T001–T003 in parallel.
- **Phase 2 (daemon/API track)** starts right after Phase 1 and runs **in parallel** with Phases 3–8. Inside it:
  - first, in parallel: T004 (read types), T006 (core), T007 (route skeleton; not `[P]` because later tasks reuse its files); T005 (calls) right after T004;
  - then in parallel: T009, T010, T012, T016, T022; and after T007: T011, T018, T019;
  - chain A (permissions → conversation → sessions → runtime): **T008 → T015 → T020 → T021**;
  - chain B (inbox): T012 → T013 → T014 → T020;
  - T009 → T021 (`run.rs`); T016 → T017 (`gh/fake.rs`); T011 + T021 → T023; T024 last (checks that no 501 stub is left).
- **Phase 3 (US1)** after Phase 1; T028 needs T004 (types) and T003. **T027 finishes before any Phase 4+ task starts.**
- **Phase 4 (US2)** after Phase 3; T048 needs T005.
- **Phase 5 (US3)** after Phase 4; needs T004/T005. Preview/e2e check needs T008, T011, T015–T018, T021 (T070 needs T011).
- **Phase 6 (US4, US5)** after Phase 5 (AskCard T058); preview/e2e needs T013, T014, T019 (T075 needs T013).
- **Phase 7 (US6)** after Phase 6; preview/e2e needs T009 (T088), Diagnóstico needs T022.
- **Phase 8 (US7)** after Phase 7.
- **Phase 9** after all.

### UI tasks that consume the API contract (T004 types, T005 calls)

Types (T004): T028, T058, T060, T064, T065, T067, T068, T071, T074, T080, T083, T084. Calls (T005): T048, T060, T064, T067, T068, T071, T074. their live data arrives with: T058/T065 ← T015; T060 ← T021; T064 ← T011; T067 ← T016, T017; T068 ← T018; T071 ← T013, T014; T074 ← T019; T080 ← T010; T083 ← T020; T084 ← T007 (`home`), T022.

### Shared files, sequenced on purpose

- `app/src/api.ts`: only T004 → T005.
- `crates/delilah-daemon/src/api.rs`: only T007.
- `crates/delilah-core/src/task.rs`: only T006.
- `inbox.rs`: T007 → T013 → T014. `hooks.rs`: T014 → T020. `conversation.rs`: T015 → T020. `permissions.rs`: T008 → T015. `run.rs`: T009 → T021. `gh/fake.rs`: T016 → T017. `runtime_api.rs`: T007 → T021. `actions.rs`, `setup.rs`, `jira.rs`: T007 → their owner (T011, T018, T019). `pr_api/mod.rs`: T007 → T013 → T014. `session_api.rs`, `store_sessions.rs`: only T020.
- `TaskScreen.tsx`: T027 (Phase 3) → T063 → T092. `TaskHeader.tsx`: T062 → T092. `Sidebar.tsx`: T027 → T054 → T089. `InboxView.tsx`: T027 → T071 → T090. `JiraView.tsx`: T027 → T074 → T091. `BoardView.*`: T073 → T093. `App.tsx`: T027 → T055 → T094.
- `app/e2e/screens.spec.ts`: T056 → T069 → T076 → T087 → T100 (one per phase).
- `app/src/styles.css`: T026 → T096 (no other task adds rules there).

### Within each task

Tests first and failing → implementation → the task's checks green → lower the file's number in `inline-style.test.ts` when touched.

## Parallel Examples

```text
# Phase 2 start (with Phase 3 starting in parallel):
T004 api.ts read types | T006 core events | T007 route skeleton   (then T005 api.ts calls)
# then:
T009 checkout default | T010 created_at | T012 inbox store | T016 PR graphql | T022 Tauri reveal | T011 reopen | T018 setup | T019 Jira
# chains, one task at a time each:
T008 → T015 → T020 → T021        T013 → T014 (→ T020)        T016 → T017

# Phase 3 kit, all at once after T026 (T028 also needs T004):
T028 vocab | T029 time | T030 Icon | T031 Button | T035 PhaseBars | T036 Toggle | T037 Segmented | T038 OptionCard
T039 Row | T040 Count | T041 Skeleton | T042 Dialog | T043 Menu | T044 Toast   (T032–T034 once T028 is done)

# Phase 5 components before the integration:
T057 history | T058 AskCard | T059 AutopilotControls | T060 SessionViewSwitch | T061 TaskTabs | T066 ChangesView | T067 PrView | T068 SetupBanner

# Phase 7 screens:
T079 ProposalCard | T080 BacklogTaskView | T081 ProjectSettings | T082 ProjectsView | T083 ProjectView | T084 SettingsView | T085 DeleteTaskDialog | T086 QuickOpen
```

## Implementation Strategy

- **MVP**: Phases 1 + 3 (US1): the new look everywhere with no behaviour change; Phase 2 runs alongside.
- **Increments**: each later phase is one brief §8 step, committed green and compared with its boards; the PR is opened only after Phase 9.
- **Reviews**: contract review at the end of Phase 2 and Phase 9; spec-compliance + test reviews after Phases 5, 7 and 9; ui-verifier on the preview at each checkpoint (both themes, 1440×900 and 1280; 1024 in Phase 8).

## Existing tests that change (brief §8 «Tests that will break»)

- **vitest** (`app/src/components/*.test.tsx`): Sidebar (T054, T089), TaskScreen (T063, T092), TaskView (T064), AutopilotMark (moved to AutopilotControls T059, deleted T096), PhaseBadge (cases → PhaseBars T035, deleted T096), PrBadge (→ PrChip T034, deleted T096), PrView (T067), PreviewControl (T062), NewTaskDialog ×2 (T078), BacklogTaskView (T080), ProjectSettings (T081), AskCard + PermissionCard (T058), SetupBanner (T068), DeleteTaskDialog (T085), ProposalCard (T079), ToolCard + ConversationView (T065), SessionChain + TreeResizer (T063), OpenPrDialog (T067), DiffRows (T066), InboxView (T071, T090), SettingsView (T084), ProjectView (T083), UpdateNotice (T094), HandoffActions (T064); `api.test.ts` (T004), `phases.test.ts` (T073), `useUpdates.test.ts` (T084).
- **UI e2e** (`app/e2e/*.spec.ts`): class selectors and labels per contracts/ui.md, moved in T056, T069, T075, T076, T087, T100.
- **Daemon**: tests asserting the worktree default in `run.rs` (T009); permissions tests that assumed `running` with a pending ask (T008); conversation tests that expected answered permission cards to disappear (T015).

## Manual checks (owner, desktop app; not automatable in a preview)

- [ ] Titlebar: every part of the breadcrumb drags the window; the traffic lights are not covered (T055).
- [ ] Notifications and the Dock badge still count only blocking items, including a pending permission (T008).
- [ ] The native menu's Logs item and the statusbar's Logs button open the same view (T055).
- [ ] «Mostrar en Finder» reveals the data folder; hidden in the browser build (T022, T084).
- [ ] Auto-update off: no scheduled check in a release build; «Buscar actualizaciones…» still works (T084).
- [ ] Theme follows the OS setting live in the packaged app, terminal and editor included (T046, T047).

## Notes

- `[P]` = parallel with the other `[P]` tasks of its phase once its **Depends** are met; no two `[P]` tasks of a phase share a file.
- Native-only checks (titlebar drag regions, notifications, Dock badge, menu, «Mostrar en Finder») are for the owner in the desktop app; say so in the PR.
- Never 7420, never `scripts/dev-daemon.sh`, never stop a daemon by name.
