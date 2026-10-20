# Implementation Plan: PR loop

**Branch**: `feature/LIBRE-1` | **Date**: 2026-10-20 | **Spec**: [spec.md](spec.md)

**Input**: Feature specification from `specs/003-pr-loop/spec.md`

## Summary

The daemon learns about each task's pull requests through the user's own `gh` (behind a
`Gh` trait, faked in tests). A separate 30 s loop finds the PR opened from a task's
branch (per repo: the project's repo, or each submodule of a parent) and, for open
PRs, takes a snapshot with one GraphQL call. A pure `diff` of the old and new snapshots
against stored watermarks decides what is new: a failed head commit, a change request,
new comments by anyone but the user, approved/merged/closed. New items become rows in
`pr_notices`, which the inbox shows as `pr-failed`/`pr-review`/`pr-blocked` (on the Dock
badge), with "Pedir al agente que lo arregle" and "Descartar". When every PR is merged,
the task is marked done and archived after a new `check_merged` (no dirty files, local
branch contained in the PR head), and its local branches are deleted; otherwise a
`pr-blocked` notice offers a forced archive. The app gets a PR tab, a PR badge on cards
and rows, and an "Abrir PR" dialog that pushes and runs `gh pr create` with the Jira
link. See [research.md](research.md).

## Technical Context

**Language/Version**: Rust (edition 2024) + TypeScript/React (Tauri app)

**Primary Dependencies**: axum, sqlx (SQLite), tokio, serde_json; the user's `gh` CLI (external, found on the login shell's PATH)

**Storage**: SQLite (`~/.delilah`): migration `0011_pull_requests.sql` (`pull_requests`, `pr_unlinked`, `pr_notices`); setting `pr_access:<project>`

**Testing**: `cargo test` (pure diff/rollup/pick, store, daemon API and poller with a fake `Gh` and fixture repos with a local bare remote, git `check_merged`/`delete_branch`), `tsc --noEmit`, vitest (PR tab, badge, inbox actions); manual in a preview with `scripts/fake-gh.sh`

**Target Platform**: macOS (daemon + Tauri app)

**Project Type**: desktop-app + local daemon + CLI

**Performance Goals**: a PR change shows within 1 min (30 s loop + host delay); one GraphQL call per open PR per check (~120/h per PR, well under the 5 000/h limit)

**Constraints**: no credentials stored or switched (only `gh`); no work ever lost by the automatic archive; no notice repeated, including across restarts; slow `gh` never delays the usage-limit resume

**Scale/Scope**: single user, a handful of open PRs at a time, up to a few submodules per changeset

## Constitution Check

`.specify/memory/constitution.md` is still the unfilled template; the project's
ground rules (`CLAUDE.md`) serve as gates instead:

| Gate | Status |
|---|---|
| Nothing from the work repos; fixtures only | ✅ fake `gh` + fixture repos with a local bare remote; `make-fixture-parent.sh` for changesets |
| Agent hooks/settings outside the repo, clean env | ✅ unchanged; `gh` runs from the daemon, not in the agent's env |
| Try changes only in a preview, never 7420 / pkill | ✅ quickstart uses `scripts/preview.sh` with `DELILAH_GH_BIN` |
| No remote operations without confirming the account | ✅ the PR tab shows the account `gh` resolves; "Abrir PR" pushes only after the user confirms the dialog; the optional GitHub check in quickstart starts with `gh auth status`; Delilah never deletes remote branches |
| Never edit a shipped migration | ✅ new `0011` |
| API ↔ client contract kept in sync | ✅ [contracts/api.md](contracts/api.md), additive only |

Post-design re-check: no change.

## Project Structure

### Documentation (this feature)

```text
specs/003-pr-loop/
├── spec.md
├── plan.md
├── research.md
├── data-model.md
├── quickstart.md
├── contracts/api.md
└── checklists/{requirements,pr-loop}.md
```

### Source Code

```text
crates/delilah-git/src/workspace.rs       # check_merged(ws, heads), delete_branch(dir, branch), fetch(dir); GitError::NotMerged
crates/delilah-git/tests/workspace.rs     # squash-merged branch passes, extra local commit / dirty file refuses
crates/delilah-daemon/migrations/0011_pull_requests.sql
crates/delilah-daemon/src/gh.rs           # new: Gh trait, CliGh (login PATH, timeout, DELILAH_GH_BIN), FakeGh (tests),
│                                         # GhError → AccessProblem classification, repo_view, user, pr_list, pr_graphql, pr_create
crates/delilah-daemon/src/prs.rs          # new: Snapshot & co, ci_outcome, pick, diff (pure + unit tests); check_task,
│                                         # detect, spawn (30 s loop, back-off), on_merged (R7), notify (FR-033)
crates/delilah-daemon/src/pr_api.rs       # new: HTTP handlers (list, summaries, refresh, link, unlink, proposal, open,
│                                         # forward, dismiss); forward prompt text (es/en)
crates/delilah-daemon/src/store_prs.rs    # new: rows, unlinked, notices (insert-or-ignore, resolve), save_check (one tx)
crates/delilah-daemon/src/actions.rs      # release_workspace pub(crate); archive finishes an archive-partial
crates/delilah-daemon/src/inbox.rs        # ItemKind PrFailed/PrReview/PrBlocked; InboxItem.pr; list adds open notices
crates/delilah-daemon/src/events.rs       # Topic::Prs
crates/delilah-daemon/src/phases.rs       # on gh pr create / git push signal: prs::nudge(key) after 15 s
crates/delilah-daemon/src/state.rs        # AppState.gh, AppState.pr_backoff
crates/delilah-daemon/src/api.rs, main.rs # routes; prs::spawn at startup
scripts/fake-gh.sh                        # new: answers gh from $FAKE_GH_DIR JSON (previews)
app/src/api.ts                            # types + calls of contracts/api.md; InboxKind, BLOCKING
app/src/live.ts                           # "prs" topic → refetch summaries / open task's PRs
app/src/components/PrView.tsx             # new: PR tab (per PR: state, checks, review, comments, account, problem,
│                                         # link by URL, unlink, refresh; empty state with "Abrir PR")
app/src/components/OpenPrDialog.tsx       # new: editable title/body, draft, submodule picker for parents
app/src/components/PrBadge.tsx            # new: compact state + CI
app/src/components/TaskScreen.tsx         # PR tab next to Cambios; refresh on open
app/src/components/Sidebar.tsx, BoardView.tsx   # PrBadge per task
app/src/components/InboxView.tsx          # new kinds: "Pedir al agente que lo arregle", "Descartar", "Archivar igualmente", open PR
app/src/alerts.ts                         # SAYS for new kinds; notify approved/merged/closed from summaries when unfocused
app/src/i18n/en.ts, app/src/styles.css
docs/status.md, docs/architecture.md, docs/decisions.md, README.md
```

**Structure Decision**: the existing layout; three new daemon modules (`gh`, `prs`,
`pr_api`) plus `store_prs`, following the `store_*` split; three new UI components.

## Design (by layer)

### 1. Git (`delilah-git/src/workspace.rs`)

- `fetch(dir, opts)`: `git fetch --quiet origin` (errors returned, caller decides).
- `check_merged(ws: &Workspace, heads: &HashMap<String, String>, opts)`: for each
  `(repo, dir)` of `ws.repos()` with a head: `status --porcelain` empty, else `Dirty`;
  `merge-base --is-ancestor <ws.branch> <head>` (exit 1 → `NotMerged { location }`,
  unknown sha → fetch first, then error). For `Parent`, also `status --porcelain` of
  `ws.path` excluding submodule pointer changes (`--ignore-submodules=all`).
- `delete_branch(dir, branch, opts)`: `branch -D <branch>` if it exists (`show-ref`),
  never on the current branch (error).
- Tests (temp repos + bare remote): squash-merged branch passes; a commit after the PR
  head refuses; an uncommitted file refuses; parent dirty refuses, submodule pointer
  change alone does not; delete_branch on the current branch errors.

### 2. Store (`store_prs.rs`, migration 0011)

- `link_pr(row)` (upsert on `(task_key, repo)`), `prs_of(key)`, `pr(id)`, `unlink_pr(id)`
  (delete + resolve notices + insert `pr_unlinked`), `is_unlinked(key, url)`,
  `open_prs()` (rows whose snapshot state is open/draft or NULL, joined with tasks
  that are not archived: workspace exists or status not terminal).
- `save_check(id, snapshot, marks, checked_at, notices: &[NewNotice], resolve: &[ResolveRule])`
  in one transaction; `INSERT OR IGNORE` on notices.
- `set_pr_error(id, error)`, `open_notices()`, `notice(id)`, `resolve_notice(id)`,
  `resolve_notices_of(key, repo, kinds)`.
- Tests: upsert replaces; unique notice ignored; cascade on task delete; open_prs
  skips archived tasks.

### 3. `gh.rs`

- `trait Gh: Send + Sync { async fn run(&self, dir: Option<&Path>, args: &[String], stdin: Option<String>) -> Result<String, GhError>; }`
- `CliGh`: `tokio::process::Command` with the login PATH (`delilah_agents::env`),
  `GH_PROMPT_DISABLED=1`, `NO_COLOR=1`, 20 s timeout, binary from `DELILAH_GH_BIN` or `gh`.
- Helpers on top: `repo_view(dir)`, `user(host)` (cached), `pr_list(dir, branch)`,
  `pr_snapshot(host, owner_name, number)` (GraphQL, R2), `pr_create(dir, …)`,
  `pr_view_url(url)`.
- `classify(GhError) -> AccessProblem`: spawn ENOENT → Missing; "gh auth login" /
  "not logged" → SignedOut; HTTP 404/403 / "Could not resolve to a Repository" →
  NoAccess; "none of the git remotes" / non-GitHub → UnsupportedRemote; "rate limit"
  (+ reset header in message) → RateLimited; timeouts / "could not connect" → Network.
- `FakeGh`: map from args prefix to canned output or error; records calls.
- Tests: parsing of `repo view`, `pr list`, GraphQL fixtures; classification table.

### 4. `prs.rs`

- Pure: `ci_outcome`, `pick` (FR-026), `diff` (R4), `summary(rows)` (worst across a
  changeset), `access_notice_due(previous_setting, problem)`.
- `check_task(app, key, force)`: for each workspace repo without a row (and task in
  review or later, or `force`): detect → `pick` → skip unlinked → link (`detected`),
  `phases::advance(Review)`. For each open row: snapshot → `diff` → `save_check` →
  notify (blocking notices via `notifier` if `!events.listening()`; info likewise) →
  emit `Prs` (+ `Inbox` if notices changed). Resolves `ci-failed` when the head
  changed and is green; resolves all notices of a PR on merge/close.
- After a check where every row of the task is merged and none closed unmerged:
  `on_merged(app, key)` (R7): MarkDone if not terminal → fetch + `check_merged` in
  `spawn_blocking` → `archive-blocked` notice, or `actions::release_workspace(force = true)`
  + `delete_workspace` + `delete_branch` per repo (skip the parent) → `archive-partial`
  notice on a later failure → notify "merged and archived".
- `spawn(app)`: 30 s `interval` with `MissedTickBehavior::Delay`; `open_prs()` + tasks
  in review without rows; per-(host, owner_name) back-off map in `AppState`.
- `nudge(app, key)`: `tokio::spawn` sleep 15 s → `check_task(force = true)`.
- Access problem → `set_pr_error`; for Missing/SignedOut/NoAccess/UnsupportedRemote,
  a notification once per project (`pr_access:<project>` setting), cleared on success.
- Tests (fake `Gh`, in-memory store, fixture repo with bare remote): detection links
  and moves to review; unlinked not re-linked; CI failure → one notice, same state →
  none, new failing head → second; green head resolves; comment by `me` ignored, by a
  bot counted; restart (new AppState, same store) sends nothing; merge → done +
  worktree and branch gone; merge with dirty file → `archive-blocked`, nothing removed;
  changeset: one merged keeps it, both archive, one closed blocks auto-archive;
  network error backs off and keeps the snapshot.

### 5. `pr_api.rs` + routes

- Handlers per [contracts/api.md](contracts/api.md).
- Proposal: title = task title; body = Jira link (settings mode cloud/data-center and key
  not `LIBRE-`; site from `jira` settings) + `git log --format=%s <base>..<branch>`;
  `needs_push` = no upstream or ahead. 409 `no-commits` / `has-pr` / `no-workspace`.
- Open: `git push -u origin <branch>` (git's credentials) if needed → `gh pr create
  --head --base --title --body-file - [--draft]` → `pr_view_url` → link (`opened`) →
  advance(Review) → emit.
- Forward: build prompt from `notice.detail` (lang): CI → names + URLs + `gh run view
  --log-failed` hint; reviews → quoted blocks with author, path:line, framed as data
  (FR-035). Paused → 409 `resume_at`. Primary session live → `type_prompt`; ended →
  reopen it (same path as `session_api::reopen`), then type. Resolve, emit `Inbox`.
- Tests: proposal with/without Jira link and with no commits; open with a fake gh and
  bare remote pushes and links; push failure links nothing; forward to a live session
  types the prompt (as `inbox::reply` tests do); forward while paused → 409.

### 6. Inbox, events, phases

- `ItemKind`: `PrFailed`, `PrReview` after `Error`; `PrBlocked` after `SuggestReview`.
- `InboxItem.pr: Option<PrRef { notice, url, repo }>` (skip if none).
- `list` appends unresolved notices of tasks that still exist.
- `Topic::Prs` (`"prs"`).
- `phases::on_hook`: when `signal_phase` matched a `gh pr create`/`git push` command,
  `prs::nudge`.

### 7. App

- `api.ts`: types and calls; `InboxKind` and `BLOCKING` extended.
- `live.ts`: `prs` → refetch `prSummaries` and, when a task screen is open, its `taskPrs`.
- `TaskScreen`: tab `PR` (constant like `CHANGES`), always available for tasks with a
  workspace or linked PRs; calls `refreshPrs` when the task is opened.
- `PrView`: per PR a card: `#n title` (link opens in the browser via `platform`), state
  chip, base, checks list (icon per outcome, link), review decision, comments count,
  "comprobado hace …", repo label for changesets; header with the account and, if any,
  the problem explained with its fix ("Instala gh: brew install gh", "Ejecuta gh auth
  login", …); actions "Actualizar", "Desvincular"; empty state: "Aún no hay PR",
  "Abrir PR" (disabled with the 409 reason) and "Vincular por URL".
- `OpenPrDialog`: loads the proposal; editable title/body; "Borrador" checkbox; for
  parents a submodule select; errors shown inline.
- `PrBadge` in `Sidebar` rows and `BoardView` cards: state icon (draft/open/merged/
  closed) + CI dot (pending/success/failure); tooltip with review decision.
- `InboxView`: `pr-failed`/`pr-review`: text, "Pedir al agente que lo arregle"
  (disabled with the paused message), "Descartar", "Ver PR"; `pr-blocked`: text,
  "Archivar igualmente" (→ archive force), "Descartar".
- `alerts.ts`: `SAYS` entries; a summaries effect notifying `aprobada`, `mergeada y
  archivada`, `PR cerrada` when unfocused (as "reanudada").
- i18n: every new string in `i18n/en.ts`.
- Tests: PrView renders states and the problem text; badge worst-state; inbox buttons
  call the right API.

## Testing strategy

Tests first, bottom-up: git (`check_merged`, `delete_branch`) → pure `prs` functions
(`diff` table tests are the core of de-duplication) → `gh` parsing and classification →
store → poller and merge archive with the fake `Gh` → API handlers → app components
(vitest) → manual run in a preview with `scripts/fake-gh.sh` ([quickstart.md](quickstart.md)),
then optionally one real PR on a personal throwaway repo.

## Risks / limitations

- `gh` output formats: GraphQL fields are stable, but error texts used by `classify`
  can change between versions; unknown errors fall back to `Network` (back-off, no
  notification), and the message is shown in the PR tab.
- Repos where the user's `gh` has another default account than git's push credentials:
  reading works with one account, pushing may fail with the other; the error is shown,
  nothing is switched (by design).
- A merge that GitHub reports late (merge queues) archives late, never early.
- Submodule branches in a parent worktree live in the worktree's module git dirs and go
  away with the worktree; `delete_branch` skips repos where the branch no longer exists.
- Polling cost grows with open PRs; at ~120 calls/h per PR the limit is reached only
  past ~40 open PRs, far beyond the single-user scope.
