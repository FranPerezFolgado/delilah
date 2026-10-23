---
description: "Task list for PR loop"
---

# Tasks: PR loop

**Input**: Design documents from `specs/003-pr-loop/`

**Prerequisites**: plan.md, spec.md, research.md, data-model.md, contracts/api.md, quickstart.md

**Tests**: included. The plan asks for tests first, bottom-up, in the inline `#[cfg(test)]` style of each crate (in-memory `Store`, a fake `Gh`, fixture repos with a local bare remote), and vitest for the app.

**Organization**: grouped by user story (US1 see the PR, US2 notices, US3 open a PR, US4 archive on merge, US5 changesets).

> **Layout change after the Phase 2 review (WP-02F):** data types live in `crates/delilah-daemon/src/pr_types.rs`; `prs.rs` holds only pure logic (`ci_outcome`, `pick`, `summary`, `diff`, `Backoff`); the I/O orchestration (`check_task`, `spawn`, `nudge`, `on_merged`) goes in a new `crates/delilah-daemon/src/pr_loop.rs`; `gh` is a directory (`gh/mod.rs`, `gh/graphql.rs`, `gh/fake.rs`) and `AppState.gh` is a `GhClient`. Where a task below says `prs.rs` for those functions or their tests, use `pr_loop.rs`.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies on incomplete tasks)
- **[Story]**: US1–US5 (spec.md)

## Path Conventions

- Git: `crates/delilah-git/src/workspace.rs`, `crates/delilah-git/tests/workspace.rs`
- Daemon: `crates/delilah-daemon/src/{gh.rs,prs.rs,pr_api.rs,store_prs.rs,inbox.rs,actions.rs,events.rs,phases.rs,state.rs,api.rs,main.rs,lib.rs}`, `crates/delilah-daemon/migrations/0011_pull_requests.sql`
- App: `app/src/api.ts`, `app/src/live.ts`, `app/src/alerts.ts`, `app/src/components/{PrView,PrBadge,OpenPrDialog,TaskScreen,Sidebar,BoardView,InboxView}.tsx`, `app/src/i18n/en.ts`, `app/src/styles.css`
- Scripts: `scripts/fake-gh.sh`

Try everything in a preview (`scripts/preview.sh`), never against the daemon on 7420. No remote operation against GitHub without confirming the account (`gh auth status` → FranPerezFolgado).

---

## Phase 1: Setup

- [X] T001 Run `cargo test --workspace`, `cargo test --manifest-path app/src-tauri/Cargo.toml` and `cd app && npx tsc --noEmit && npx vitest run`; note any pre-existing failure before starting (no files changed)
- [X] T002 [P] Add migration `crates/delilah-daemon/migrations/0011_pull_requests.sql` with the tables `pull_requests`, `pr_unlinked` and `pr_notices` exactly as in data-model.md (FKs to `tasks(key)` ON DELETE CASCADE, `UNIQUE (task_key, repo)` and `UNIQUE (task_key, repo, kind, cause)`)
- [X] T003 [P] Add `Topic::Prs` (serialized `"prs"`, doc comment "`/prs`: a task's PRs or their summaries changed") in `crates/delilah-daemon/src/events.rs`, with a serialization test next to the existing ones
- [X] T004 [P] Write `scripts/fake-gh.sh` (executable, bash): answers `gh` from JSON files in `$FAKE_GH_DIR` — `repo view` → `repo.json`, `api user` → `user.json` (missing file → exit 1 with "You are not logged into any GitHub hosts. To log in, run: gh auth login"), `pr list` → `pr-list.json`, `api graphql … -F number=N` → `pr-N.json`, `pr create` → appends to `pr-list.json` and prints the new URL; anything else → exit 1 with the args on stderr (quickstart.md)

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: the `gh` wrapper, the pure PR types and the store, needed by every story.

### Tests first

- [X] T005 [P] Write failing unit tests in `crates/delilah-daemon/src/gh.rs` (`#[cfg(test)]`): parse `gh repo view --json nameWithOwner,url,defaultBranchRef` (host from the URL, incl. an Enterprise host); parse `gh pr list --json number,url,state,updatedAt`; parse a GraphQL PR fixture into `Snapshot` (check runs with conclusions SUCCESS/FAILURE/CANCELLED/SKIPPED/NEUTRAL/TIMED_OUT, a commit status ERROR, reviews, issue and review-thread comments with databaseIds); `classify` table: spawn ENOENT → Missing, "gh auth login" → SignedOut, "Could not resolve to a Repository" / HTTP 404 / 403 → NoAccess, "none of the git remotes configured for this repository point to a known GitHub host" → UnsupportedRemote, "API rate limit exceeded" → RateLimited, timeout / "could not connect" / unknown → Network
- [X] T006 [P] Write failing unit tests in `crates/delilah-daemon/src/prs.rs` (`#[cfg(test)]`) for the pure helpers: `ci_outcome` (any failure/timed-out/action-required/startup-failure/error → Failure; cancelled/skipped/neutral/stale ignored; pending if any queued/in-progress and no failure; empty → None); `pick` (open > merged > closed, newest first in each group); `summary` of several rows (worst CI failure > pending > success; state open while any open; count)
- [X] T007 [P] Write failing store tests in `crates/delilah-daemon/src/store_prs.rs` (`#[cfg(test)]`, `Store::in_memory()`): `link_pr` upserts on (task, repo) replacing the old PR; `prs_of`, `pr(id)`; `unlink_pr` deletes the row, resolves its notices and records the URL so `is_unlinked` is true; `insert_notice` with the same (task, repo, kind, cause) twice keeps one row; `save_check` writes snapshot, marks, `checked_at`, new notices and resolutions atomically; `open_notices` skips resolved ones; deleting the task cascades to all three tables; `open_prs` returns rows with NULL/open/draft snapshots of tasks not archived (workspace exists or status not terminal) and skips merged/closed and archived ones

### Implementation

- [X] T008 Implement `crates/delilah-daemon/src/gh.rs` per plan §3: `GhError`, `AccessProblem` (with `RateLimited { reset: Option<DateTime<Utc>> }`), `classify`, trait `Gh: Send + Sync` (`fn run(&self, dir: Option<&Path>, args: Vec<String>, stdin: Option<String>) -> futures_util::future::BoxFuture<'_, Result<String, GhError>>`, object-safe so `AppState` can hold `Arc<dyn Gh>`; no new dependency), `CliGh` (binary from `DELILAH_GH_BIN` or `gh`, PATH from `delilah_agents::env`'s login PATH, env `GH_PROMPT_DISABLED=1` and `NO_COLOR=1`, 20 s timeout via `tokio::time::timeout`, kill on timeout), `FakeGh` (args-prefix → output/error map, records calls; `#[cfg(test)]` or `pub` behind a `test-util` cfg), and helpers `repo_view`, `user` (cached per host in a `Mutex<HashMap>`), `pr_list`, `pr_snapshot` (GraphQL query of research R2 as a `const`), `pr_view_url`, `pr_create`; make T005 pass
- [X] T009 Implement the pure types and helpers in `crates/delilah-daemon/src/prs.rs` (`Snapshot`, `PrState`, `Check`, `Outcome`, `ReviewDecision`, `Review`, `Comment`, `CommentKind`, `Marks`, `ListedPr`, `ci_outcome`, `pick`, `summary` with a `PrSummary` serde type matching contracts/api.md, all `Serialize`/`Deserialize` with `rename_all = "kebab-case"` where the contract uses kebab values); make T006 pass
- [X] T010 Implement `crates/delilah-daemon/src/store_prs.rs` (`impl Store`: `link_pr`, `prs_of`, `pr`, `unlink_pr`, `is_unlinked`, `open_prs`, `save_check`, `set_pr_error`, `insert_notice`, `open_notices`, `notice`, `resolve_notice`, `resolve_notices_of(key, repo, kinds)`; row types `PrRow`, `NoticeRow`, `NewNotice`); register the module in `crates/delilah-daemon/src/lib.rs`; make T007 pass
- [X] T011 Add `gh: Arc<dyn Gh>` (default `CliGh`) and `pr_backoff: Arc<Mutex<HashMap<String, (u32, Instant)>>>` to `AppState` in `crates/delilah-daemon/src/state.rs`, with a constructor variant or setter so tests inject `FakeGh`; register `gh`, `prs` in `crates/delilah-daemon/src/lib.rs`

**Checkpoint**: `cargo test -p delilah-daemon` green; nothing user-visible yet.

---

## Phase 3: User Story 1 - See the task's PR and its state (Priority: P1) 🎯 MVP

**Goal**: a PR opened from a task's branch is found within a minute and shown in a PR tab and as a badge; it can be linked by URL and unlinked.

**Independent Test**: preview with `scripts/fake-gh.sh`; write `pr-list.json` with an open PR for the task's branch → within 30 s the PR tab and the badge show it with its checks (quickstart steps 1–2).

### Tests first

- [X] T012 [US1] Write failing tests in `crates/delilah-daemon/src/prs.rs` for `check_task` with `FakeGh`, in-memory store and a fixture repo task in phase review: a PR from the branch is linked (`linked_by = detected`), its snapshot saved, the phase stays/moves to review, `Topic::Prs` emitted; an unlinked URL is not linked again; a task below review without a PR is not searched unless `force`; a network error keeps the previous snapshot, sets `error` and backs off (second call within the back-off window makes no `gh` call); an access problem (SignedOut) is notified once per project (setting `pr_access:<project>`) and cleared on the next success
- [X] T013 [P] [US1] Write failing API tests for `GET /api/v1/tasks/{key}/prs`, `GET /api/v1/prs`, `POST /api/v1/tasks/{key}/prs/refresh`, `POST /api/v1/tasks/{key}/prs/link` (bad URL → 400; good URL → 200, moves the phase to review), `DELETE /api/v1/tasks/{key}/prs/{id}` (→ 204, then not re-detected) in `crates/delilah-daemon/src/pr_api.rs` (`#[cfg(test)]`, router with `FakeGh`), shapes per contracts/api.md

### Implementation

- [X] T014 [US1] Implement in `crates/delilah-daemon/src/prs.rs`: `check_task(app, key, force)` (detection per workspace repo via `Workspace::repos()` + `repo_view` + `pr_list` + `pick`, skip `is_unlinked`, `link_pr`, `phases::advance(Review)`; snapshot of each open row via `pr_snapshot`, `save_check` with no notices yet; access problems → `set_pr_error` + once-per-project notification through `app.notifier` only when `!app.events.listening()`), back-off per (host, owner_name) of research R6 (30 s × 2ⁿ up to 10 min, or the rate-limit reset), `spawn(app)` (30 s `interval`, `MissedTickBehavior::Delay`, `open_prs()` plus tasks in review or later with no rows), `nudge(app, key)` (sleep 15 s then `check_task(force = true)`); make T012 pass
- [X] T015 [US1] Implement handlers `task_prs`, `summaries`, `refresh`, `link`, `unlink` in `crates/delilah-daemon/src/pr_api.rs` (account via `gh::user(host)`, `problem` from the rows' `error`), register the routes in `crates/delilah-daemon/src/api.rs` and the module in `crates/delilah-daemon/src/lib.rs`; make T013 pass
- [X] T016 [US1] Start `prs::spawn(app.clone())` next to `scheduler::spawn` in `crates/delilah-daemon/src/main.rs`; in `crates/delilah-daemon/src/phases.rs` `on_hook`, when `signal_phase` matched a `gh pr create` / `git push` command, call `prs::nudge(app, &key)`; add a test in `phases.rs` that the nudge is requested for those commands only
- [X] T017 [P] [US1] Add the PR types and calls of contracts/api.md (`PrCheck`, `PrSnapshot`, `PullRequest`, `TaskPrs`, `PrSummary`, `taskPrs`, `prSummaries`, `refreshPrs`, `linkPr`, `unlinkPr`) to `app/src/api.ts`, and handle the `prs` topic in `app/src/live.ts` (refetch summaries and the open task's PRs)
- [X] T018 [P] [US1] Write `app/src/components/PrBadge.tsx` (state icon draft/open/merged/closed + CI dot pending/success/failure, tooltip with the review decision) and `app/src/components/PrBadge.test.tsx` (renders each state; a changeset summary shows the worst)
- [X] T019 [US1] Write `app/src/components/PrView.tsx` per plan §7 (cards per PR: `#n title` link via `platform`, state chip, base, checks with links, review decision, comments count, "comprobado hace …", repo label when not `.`; header with the account and the problem text with its fix; "Actualizar", "Desvincular"; empty state "Aún no hay PR" + "Vincular por URL"; the "Abrir PR" button is added in US3) and `app/src/components/PrView.test.tsx` (states, problem text, empty state, link by URL calls `linkPr`)
- [X] T020 [US1] Add the `PR` tab next to Cambios in `app/src/components/TaskScreen.tsx` (constant like `CHANGES`, shown for tasks with a workspace or linked PRs; `refreshPrs(key)` when the task screen opens), and `PrBadge` in task rows of `app/src/components/Sidebar.tsx` and cards of `app/src/components/BoardView.tsx` (summaries merged by task key)
- [X] T021 [P] [US1] Add every new US1 string to `app/src/i18n/en.ts` and the PR tab / badge styles to `app/src/styles.css`

**Checkpoint**: US1 works alone in a preview with the fake `gh`.

---

## Phase 4: User Story 2 - Be told when the PR needs me (Priority: P1)

**Goal**: CI failures and new reviews/comments (anyone but the user's account) become one notification and one Bandeja item each, never repeated; the item can be forwarded to the agent or dismissed.

**Independent Test**: quickstart steps 3–5: failing check → one `pr-failed` item; unchanged → nothing; new failing head → a second; comment by another login → `pr-review`, by the `user.json` login → nothing; "Pedir al agente" types the prompt.

### Tests first

- [X] T022 [US2] Write failing table tests for `diff(prev, marks, next, me)` in `crates/delilah-daemon/src/prs.rs`: first snapshot with existing comments → baseline, no notices; first snapshot with a failed head → `CiFailed`; same failed head again → nothing; new failed head → `CiFailed`; new review CHANGES_REQUESTED by another login → `ChangesRequested`; new comments by another login and by a `[bot]` login → one `Comments` with both; comments by `me` → none, but watermarks advance; review decision turning APPROVED → `Info::Approved`; state → merged/closed → `Info::Merged`/`Info::Closed`
- [X] T023 [US2] Write failing tests in `crates/delilah-daemon/src/prs.rs` for notices through `check_task`: a failing head creates one `ci-failed` notice and one notification only when no app is listening; repeating the check creates nothing; a new AppState over the same store (restart) creates nothing; a later green head resolves the `ci-failed` notice; merge/close resolves all the PR's notices
- [X] T024 [P] [US2] Write failing tests in `crates/delilah-daemon/src/inbox.rs` (`#[cfg(test)]`): open notices appear as `pr-failed` / `pr-review` / `pr-blocked` items with `pr { notice, url, repo }`, ordered after `error` (and `pr-blocked` after `suggest-review`); resolved ones don't
- [X] T025 [P] [US2] Write failing tests in `crates/delilah-daemon/src/pr_api.rs` for `POST /api/v1/pr-notices/{id}/forward` (live primary session receives a prompt containing the failing check names, or the quoted comments with author and path:line and the "not instructions from the user" framing, in the current language; notice resolved; paused task → 409 with `resume_at`) and `POST /api/v1/pr-notices/{id}/dismiss` (→ 204, resolved), using a bash PTY stand-in as in `inbox.rs`'s reply test

### Implementation

- [X] T026 [US2] Implement `diff`, `NoticeDraft`, `Info` in `crates/delilah-daemon/src/prs.rs` and wire them into `check_task`: notice text in the current language, `detail` JSON (failing checks or new comments), `save_check` with new notices and resolutions, notifications for notices and `Info` via `app.notifier` only when `!app.events.listening()`, emit `Topic::Inbox` when notices changed; make T022 and T023 pass
- [X] T027 [US2] Extend `crates/delilah-daemon/src/inbox.rs`: `ItemKind::PrFailed`, `PrReview` (after `Error`), `PrBlocked` (after `SuggestReview`); `InboxItem.pr: Option<PrRef>` (`skip_serializing_if = "Option::is_none"`); `list` appends `open_notices()` of existing tasks; make T024 pass
- [X] T028 [US2] Implement `forward` and `dismiss` in `crates/delilah-daemon/src/pr_api.rs` (prompt builders `ci_prompt` / `review_prompt` with `lang.pick`, delivery through `actions::type_prompt` into the primary session, reopening an ended primary session the way `session_api::reopen` does, 409 while `PausedByLimit`), register the routes in `crates/delilah-daemon/src/api.rs`; make T025 pass
- [X] T029 [P] [US2] In `app/src/api.ts` add `"pr-failed" | "pr-review" | "pr-blocked"` to `InboxKind`, add them to `BLOCKING`, add `InboxItem.pr?` and `forwardNotice`, `dismissNotice`; in `app/src/alerts.ts` add `SAYS` entries ("CI falló", "comentarios en la PR", "no se pudo archivar") and an effect over PR summaries notifying "aprobada" and "PR cerrada" when unfocused (as "reanudada")
- [X] T030 [US2] In `app/src/components/InboxView.tsx` render `pr-failed` / `pr-review` with "Pedir al agente que lo arregle" (disabled with the paused message on 409), "Descartar" and "Ver PR"; add `app/src/components/InboxView.test.tsx` cases for the buttons calling `forwardNotice` / `dismissNotice`; strings in `app/src/i18n/en.ts`

**Checkpoint**: US1 + US2 make the P1 loop: the agent works, Delilah watches, Fran is called only when needed.

---

## Phase 5: User Story 3 - Open the PR from Delilah (Priority: P2)

**Goal**: "Abrir PR" proposes an editable title/description with the Jira link, pushes if needed and creates the PR.

**Independent Test**: quickstart (fake `gh`): task with commits and no PR → "Abrir PR" → accept → `pr-list.json` has it and the task shows it; with no commits the button is disabled.

### Tests first

- [X] T031 [P] [US3] Write failing tests in `crates/delilah-git/tests/workspace.rs` for new helpers `commit_subjects(dir, base, branch)`, `needs_push(dir, branch)` (no upstream or ahead) and `push_branch(dir, branch)` against a temp repo with a local bare remote
- [X] T032 [US3] Write failing tests in `crates/delilah-daemon/src/pr_api.rs`: proposal with Jira configured (cloud) and key `PROJ-1` → body starts with `<site>/browse/PROJ-1` and lists the commit subjects; free task `LIBRE-1` → no link; no commits beyond base → 409 `no-commits`; task with an open PR → 409 `has-pr`; `POST /tasks/{key}/prs` pushes to the bare remote, calls `gh pr create` with `--head`, `--base`, `--title`, `--body-file -` (and `--draft` when asked), links (`opened`) and moves to review; a push failure → 409 with git's stderr and no row

### Implementation

- [X] T033 [US3] Implement `commit_subjects`, `needs_push`, `push_branch` in `crates/delilah-git/src/workspace.rs` (export from `crates/delilah-git/src/lib.rs`); make T031 pass
- [X] T034 [US3] Implement `proposal` and `open` handlers in `crates/delilah-daemon/src/pr_api.rs` (Jira site and mode from the daemon's Jira settings in `crates/delilah-daemon/src/jira.rs`; git calls in `spawn_blocking`), register routes in `crates/delilah-daemon/src/api.rs`; make T032 pass
- [X] T035 [P] [US3] Add `PrProposal`, `prProposal`, `openPr` to `app/src/api.ts`
- [X] T036 [US3] Write `app/src/components/OpenPrDialog.tsx` (loads the proposal; editable title and body; "Borrador" checkbox; errors inline; the submodule select is added in US5) with `app/src/components/OpenPrDialog.test.tsx`, and add the "Abrir PR" button to the empty state of `app/src/components/PrView.tsx` (disabled with the 409 reason); strings in `app/src/i18n/en.ts`

**Checkpoint**: every PR can carry the Jira link, without depending on the agent.

---

## Phase 6: User Story 4 - Clean up when the PR is merged (Priority: P2)

**Goal**: a merged PR marks the task done and archives it (workspace released, local branch deleted) unless work would be lost.

**Independent Test**: quickstart step 6: `"state": "MERGED"` → task done, worktree and branch gone, notification; with an uncommitted file → `pr-blocked`, nothing removed.

### Tests first

- [X] T037 [P] [US4] Write failing tests in `crates/delilah-git/tests/workspace.rs` for `fetch`, `check_merged(ws, heads)` and `delete_branch`: a branch squash-merged on the remote passes (local head is the PR head); a local commit after the PR head → `NotMerged`; an uncommitted file → `Dirty`; for a parent, a dirty parent refuses but a submodule pointer change alone does not; `delete_branch` of a missing branch is Ok, of the current branch errors
- [X] T038 [US4] Write failing tests in `crates/delilah-daemon/src/prs.rs` for `on_merged` through `check_task` (fixture repo with bare remote): running task + merged PR → done, sessions stopped, worktree removed, local branch deleted, "merged and archived" notification; checkout-mode task → back to the previous branch, then branch deleted; dirty workspace → done, nothing removed, one `archive-blocked` notice; branch deletion failing after release → `archive-partial` notice; closed unmerged → only a notification, task unchanged
- [X] T039 [P] [US4] Write a failing test in `crates/delilah-daemon/src/actions.rs` that `POST /tasks/{key}/archive` on a task with an open `archive-partial` notice deletes the leftover branch and resolves the notice

### Implementation

- [X] T040 [US4] Implement `fetch`, `check_merged`, `delete_branch` and `GitError::NotMerged { location }` in `crates/delilah-git/src/workspace.rs` (+ `crates/delilah-git/src/command.rs` for the error); make T037 pass
- [X] T041 [US4] Make `release_workspace` `pub(crate)` in `crates/delilah-daemon/src/actions.rs` and let `archive` finish an `archive-partial` (delete the remaining branches, resolve the notice); make T039 pass
- [X] T042 [US4] Implement `on_merged(app, key)` in `crates/delilah-daemon/src/prs.rs` per research R7 (MarkDone if not terminal → `fetch` + `check_merged` in `spawn_blocking` → `archive-blocked` notice, or `release_workspace(force = true)` + `delete_workspace` + `delete_branch` per repo → `archive-partial` on failure → notification), called from `check_task` when every row is merged and none closed unmerged; make T038 pass
- [X] T043 [US4] In `app/src/components/InboxView.tsx` render `pr-blocked` with its text, "Archivar igualmente" (`archive(key, true)`) and "Descartar"; in `app/src/alerts.ts` notify "mergeada y archivada" from summaries when unfocused; strings in `app/src/i18n/en.ts`; extend `app/src/components/InboxView.test.tsx`

**Checkpoint**: the loop closes on its own for plain repo projects.

---

## Phase 7: User Story 5 - Changesets: one PR per submodule (Priority: P3)

**Goal**: parent tasks track one PR per touched submodule; archive only when all are merged; "Abrir PR" per submodule.

**Independent Test**: quickstart step 7 with `scripts/make-fixture-parent.sh`: two submodules, two PRs; merging one keeps the task, merging both archives it.

### Tests first

- [X] T044 [US5] Write failing tests in `crates/delilah-daemon/src/prs.rs` with a parent fixture (two submodules, fake `gh` per repo folder): one PR detected per submodule (`repo` = submodule path); one merged → not archived; both merged → archived, submodule branches deleted, parent branch kept; one merged + one closed → not archived, notification; `summary` reports the worst state and `count = 2`
- [X] T045 [P] [US5] Write a failing test in `crates/delilah-daemon/src/pr_api.rs`: proposal and open with `repo = <submodule>` push and create from that submodule's folder; an unknown repo → 400

### Implementation

- [X] T046 [US5] Make `check_task`, `on_merged` and the merged/closed rule in `crates/delilah-daemon/src/prs.rs` work per submodule (only submodules get PRs for parents; FR-038, FR-039); make T044 pass
- [X] T047 [US5] Validate and use `repo` in `proposal` / `open` of `crates/delilah-daemon/src/pr_api.rs`; make T045 pass
- [X] T048 [US5] Add the submodule select to `app/src/components/OpenPrDialog.tsx` (parents only, submodules without an open PR) and group cards by submodule in `app/src/components/PrView.tsx`; extend their tests

**Checkpoint**: all stories work independently.

---

## Phase 8: Polish & Cross-Cutting Concerns

- [X] T049 [P] Update `docs/status.md` (built: PR loop; pending: cross-linked changeset PRs, QMD learning on merge), `docs/architecture.md` (modules `gh`, `prs`, `pr_api`, `store_prs`; tables; the `prs` topic; merge archive rules), `docs/decisions.md` (gh instead of a token; polling not webhooks; notices as rows; archive on merge without asking, with the no-lost-work check; comments from anyone but the user) and `README.md` (PR tab, badge, Bandeja items, "Abrir PR", archive on merge)
- [X] T050 Run `cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --manifest-path app/src-tauri/Cargo.toml` and `cd app && npx tsc --noEmit && npx vitest run`; fix anything red
- [X] T051 Run quickstart.md in a preview with `DELILAH_GH_BIN=scripts/fake-gh.sh` (steps 1–8) and record the result per step in `specs/003-pr-loop/quickstart.md` under "Results"

---

## Dependencies & Execution Order

- **Setup (T001–T004)** → **Foundational (T005–T011)** → stories.
- **US1** first (it creates `check_task`, the routes and the PR tab).
- **US2** depends on US1 (`check_task`, PrView); **US3** depends on US1 (PrView, routes) and can run in parallel with US2 only for its git and dialog files (T031, T033, T035, T036), since both touch `pr_api.rs` and `api.rs`.
- **US4** depends on US2 (notices) and US1.
- **US5** depends on US3 (proposal/open) and US4 (on_merged).
- **Polish** last.

Files shared across stories (sequential edits, never in parallel): `prs.rs`, `pr_api.rs`, `api.rs`, `app/src/api.ts`, `app/src/i18n/en.ts`, `InboxView.tsx`, `PrView.tsx`.

## Parallel Examples

- Setup: T002, T003, T004 together.
- Foundational tests: T005 (`gh.rs`), T006 (`prs.rs`), T007 (`store_prs.rs`) together; then T008, T009, T010 together (same split), then T011.
- US1: T013 alongside T012; T017, T018, T021 together once T015 is in.
- US2: T024 and T025 alongside T022/T023; T029 alongside T026–T028.
- US3: T031 alongside T032; T035 alongside T033/T034.
- US4: T037 and T039 alongside T038.

## Implementation Strategy

1. **MVP = US1 + US2** (both P1): the PR is visible and Fran is called only when CI or a reviewer needs him. Stop and try it in a preview.
2. **US3** (Jira link in every PR), then **US4** (archive on merge) close the loop for repo projects.
3. **US5** extends it to parent projects (the work repos).
4. Each phase ends green (`cargo test`, `tsc`, `vitest`) and is committed on `feature/LIBRE-1`.
