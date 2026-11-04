# Implementation Plan: Track every PR a task opens

**Branch**: `015-multiple-prs` | **Date**: 2026-11-03 | **Spec**: [spec.md](spec.md)
**Input**: Feature specification from `specs/015-multiple-prs/spec.md`

## Summary

A task's PR links stop being "one per repo of the workspace" and become "every PR the
task opened": the link is identified by the PR (its URL), not by the repo. Three things
feed it: branch detection in every repo of the workspace (now including a parent's own
repo when it is on the task's branch), the URL printed by the agent's `gh pr create`
(read from the `PostToolUse` hook, any repo, any branch), and the manual paths (Open
PR, Link by URL, now accepting any repo `gh` can read). The merge archive counts every
non-replaced PR, re-detects once before archiving, waits while the agent is mid-turn,
and only verifies/deletes local branches for PRs that came from the task's branch in a
workspace repo. A migration rebuilds `pull_requests` (unique on `(task_key, url)`) and
`pr_notices` (notices keyed by PR, not repo). Autofix, notices and the UI address a PR
by its id instead of its repo.

## Technical Context

**Language/Version**: Rust 2024 (workspace), TypeScript + React (app/, Vite)
**Primary Dependencies**: axum, sqlx (SQLite), tokio, serde; `gh` CLI behind the `Gh` trait (faked in tests, `DELILAH_GH_BIN` in previews)
**Storage**: SQLite, new migration `crates/delilah-daemon/migrations/0027_pr_per_url.sql`
**Testing**: `cargo test -p delilah-daemon`, `cargo test -p delilah-git`, `cd app && npx vitest run`; parent fixture `scripts/make-fixture-parent.sh`
**Target Platform**: macOS daemon + Tauri app
**Project Type**: daemon (HTTP API) + web UI
**Performance Goals**: no extra `gh` calls per poll beyond one `pr list` for the parent's own repo when it is on the task's branch; the pre-archive re-detection is one forced pass only when every PR is merged
**Constraints**: never edit a shipped migration; keep old clients working (fields added, not removed); throttling/backoff of `gh` per dir and per repo unchanged
**Scale/Scope**: a handful of PRs per task

## Constitution Check

`.specify/memory/constitution.md` is the unfilled template: no gates. The project's
ground rules (CLAUDE.md) apply: migration is new, not an edit; contract changes are
additive; Spanish and English strings; previews for trying it.

## Root causes (from the code)

| Gap | Where | Fix |
|---|---|---|
| Parent's own repo never searched | `Workspace::repos()` lists only submodules for a parent (`crates/delilah-git/src/workspace.rs:43`) | new `Workspace::pr_repos(opts)` = `repos()` + `(".", path)` when `parent_has_branch(opts)` |
| Only `--head <ws.branch>` is searched | `pr_loop::run_check`/`detect` (`pr_loop/mod.rs:185-300`) | agent URL capture from the hook (any branch/repo) |
| One row per repo | `UNIQUE (task_key, repo)` (`migrations/0011`), `link_pr` upsert on `(task_key, repo)` (`store_prs.rs:229`) | unique on `(task_key, url)`; `link_pr` upserts by URL |
| Link by URL refuses other repos | `repo_of` → `LinkError::NotFromTask` (`pr_loop/mod.rs:405`) | label outside repos `host/owner/name` instead of refusing |
| Hook only nudges | `phases.rs:172` `gh pr create` → `nudge` | parse `tool_response` URLs and link them |
| Early archive | `merge::all_merged` over the known rows (`pr_loop/merge.rs:26`) | re-detect before archiving; not while the agent is working |

## Design

### 1. Data (`0027_pr_per_url.sql`, `store_prs.rs`)

See [data-model.md](data-model.md). Rebuild `pull_requests` without `UNIQUE (task_key,
repo)`, with `UNIQUE (task_key, url)` and a nullable `replaced_by INTEGER` (id of the
newer PR). Rebuild `pr_notices` with a `pr_url TEXT NOT NULL DEFAULT ''` column (backfilled
from `pull_requests` by `(task_key, repo)`) and `UNIQUE (task_key, repo, pr_url, kind,
cause)`; task-level notices (`archive-blocked`, `archive-partial`, base-stale) keep
`pr_url = ''`. Copy rows keeping ids (autofix, notices and the UI refer to PR ids).

`store_prs.rs`:
- `link_pr` upserts on `(task_key, url)`; it no longer resolves other PRs' notices of the
  same repo (a second PR is not a replacement). Instead, when the new PR has the same
  repo and head branch as a PR whose snapshot is `Closed` (not merged), it sets that one's
  `replaced_by` and resolves its notices (FR-009). Head branch comes from the snapshot
  (`head_branch`, below) or, for detection, from the task branch.
- Every query that selects a PR by `(task_key, repo)` (`store_prs.rs:160, 233, 242, 299,
  343`) moves to the PR id or URL; notice queries filter by `pr_url`.
- `prs_of` returns all rows; a `counted(rows)` helper filters out `replaced_by IS NOT NULL`.

### 2. Snapshot (`pr_types.rs`, `gh/graphql.rs`, `gh/fake.rs`)

Add `head_branch: Option<String>` (`#[serde(default)]`) from `headRefName` to the
GraphQL query and the `pr view` parse. Old snapshots read as `None`; the next check
fills it.

### 3. Where PRs are looked for (`delilah-git/src/workspace.rs`, `pr_loop/mod.rs`)

- `Workspace::pr_repos(&self, opts) -> Vec<(String, PathBuf)>`: `repos()`, plus `"."` for
  a parent whose own repo is on the task's branch. `repos()` itself is unchanged (archive
  checks, setup, release keep their meaning); both doc comments say which callers use
  which: PR detection, Open PR, `repo_of` and the archive's `heads` use `pr_repos`.
- `run_check` iterates `pr_repos`. The skip rule becomes "skip repos that already have a
  counted *open* or *merged* PR from the task's branch" (instead of "any row"), so a closed
  PR in a repo does not stop detection of its replacement.
- `detect` links by URL; `existing.any(|r| r.url == url)` is the duplicate check.
  `crate::prs::pick` stays (one PR per head branch per repo).

### 4. The agent's PRs (`phases.rs`, new `pr_loop/capture.rs`)

On the `PostToolUse` hook of a Bash tool whose `tool_input.command` contains
`gh pr create` (the same test as `signal_phase`), take `tool_response.stdout` (and
`stderr`: `gh` prints "already exists" URLs there), extract every token that
`gh::parse_pr_url` accepts, and for each one not yet linked and not in `pr_unlinked`:
read it with `gh.pr_view_url`, resolve the label with `repo_of` (workspace repo when its
remote matches, else `host/owner/name`) and `link_known(..., "agent")`. Runs in a spawned
task so the hook never waits; failures go through the existing `failed()` reporting. The
existing `nudge` stays for the push case. Hooks of a task's subagents count (same
session); hooks of project sessions do not (`on_hook` is per task already). Only the
output of a command that contains `gh pr create` counts (FR-004).

### 5. Link by URL and Open PR (`pr_loop/mod.rs`, `pr_api/mod.rs`, `pr_api/open.rs`)

- `repo_of`: search `pr_repos(opts)`; no match → `Ok(format!("{host}/{owner_name}"))`
  instead of `NotFromTask` (FR-007). The no-workspace branch (`pr_loop/mod.rs:426`) keeps
  `.` for a plain-repo project's checkout and, for a parent project, matches its own
  checkout (`.`) and each initialised submodule via `Workspace::home_of`, before falling
  back to `host/owner/name`. `LinkError::NotFromTask` and its 409 go; the test
  `linking_a_pr_of_another_repo_is_409` becomes "is linked with the host label".
- Proposal `repos` and the open handler use `pr_repos`, and "has an open PR" means an
  open counted PR whose head branch is the task's branch (FR-016).
- Unlink stays by id.

### 6. Closing the task (`pr_loop/merge.rs`, `pr_loop/mod.rs`)

- `all_merged` works over `counted(rows)` (FR-010).
- In `run_check`, when `all_merged` is true: return without archiving if the task's
  primary session is `LiveState::Working` (FR-012). Today only a push / `gh pr create`
  nudges the loop (`phases.rs:172`), so add `pr_loop::nudge` on `Signal::TurnEnded` for
  tasks that have PRs; otherwise the 30 s poll decides. Otherwise run one forced detection pass over `pr_repos` (if not
  already forced in this call), re-read the rows and re-evaluate (FR-011).
- `archive_merged`/`verify`: `heads` is built only from counted PRs whose label is a
  workspace repo and whose `head_branch` is the task's branch (or is `None`, for old
  snapshots of detected PRs); keyed by label. PRs of other branches and outside repos
  are trusted from their remote state and touch nothing local (FR-013). The parent's
  own repo with a merged PR is in `heads`, so its branch is verified and deleted like a
  submodule's; without a PR it is checked as today (FR-038).
- If a label has two counted merged PRs from the task's branch, the one with the highest
  id (linked last) gives the head; tested.

### 7. Every repo-keyed lookup moves to the PR (`pr_loop/notices.rs`, `autofix/`, `inbox.rs`, `pr_api/forward.rs`, `integration.rs`)

A repo can now hold several PRs, so every place that finds "the PR of a repo" or "the
notices of a repo" is wrong. Notices carry `pr_url` end to end; resolve/insert by PR.
Known sites (review of this plan; the implementer greps `\.repo ==` and `repo = ?` in
`crates/delilah-daemon/src` to find the rest):
- `pr_api/forward.rs:268` and `inbox.rs:774`: `find(|p| p.repo == notice.repo)` → match `p.url == notice.pr_url`.
- `inbox.rs:281` (`pr-autofix` inbox item) and `inbox.rs:1821` (task-level `.`).
- `pr_loop/notices.rs:71,89,101`.
- `autofix/mod.rs:381-471, 853, 932, 964`, including `cancel_logged(.., Some(&row.repo))` and
  `resolve_stopped(.., &row.repo)`: by PR, so a sibling PR's notices are untouched.
- `integration.rs:145` (`subtask_facts`, parent-task rollup): filter `replaced_by IS NULL`,
  so a replaced closed PR of a subtask doesn't count as unmerged (spec: rollup unchanged).
- Tests and fixtures inserting `(task_key, repo)`: `store.rs:1447`, `jira_import.rs:1841`.
 `POST /tasks/{key}/autofix` accepts
`pr: <id>` alongside the existing `repo` (kept for old clients: it means "the counted
open PR of that repo", the first by id). Autofix's per-PR state already lives on the
row (`pull_requests.autofix`).

### 8. API and UI (`pr_api/dto.rs`, `app/src/api.ts`, `PrView.tsx`, `OpenPrDialog.tsx`, badge)

See [contracts/api.md](contracts/api.md). `PullRequest` gains `replaced_by` and
`head_branch` (whether a repo is outside the workspace is read from the label's shape); `linked_by` gains `"agent"`. `PrSummary` is computed over
counted PRs. PrView groups by `repo` (already does), shows replaced PRs muted with "replaced",
keys cards by `pr.id`, labels outside repos, and calls `setAutofix(owner, mode, undefined,
pr.id)`. Strings in `app/src/i18n/en.ts` and the Spanish source.

### 9. Docs

`docs/usage.md` (Pull requests section), `docs/status.md`, `docs/architecture.md`
(`pull_requests` table), and a decision in `docs/decisions.md`: "a task's PR links are
per PR, not per repo; the agent's `gh pr create` output is a link source".

## Project Structure

### Documentation (this feature)

```text
specs/015-multiple-prs/
├── plan.md
├── research.md
├── data-model.md
├── quickstart.md
├── contracts/api.md
└── tasks.md            # /speckit-tasks
```

### Source Code (repository root)

```text
crates/delilah-git/src/workspace.rs           # pr_repos
crates/delilah-daemon/migrations/0027_pr_per_url.sql
crates/delilah-daemon/src/
├── store_prs.rs                               # link by URL, replaced_by, notices by PR
├── pr_types.rs, gh/{graphql.rs,mod.rs,fake.rs} # head_branch
├── phases.rs                                  # PostToolUse capture hook-up, turn-end nudge
├── pr_loop/{mod.rs,capture.rs,merge.rs,notices.rs}
├── pr_api/{mod.rs,open.rs,dto.rs}
├── prs.rs                                     # summary over counted PRs
├── inbox.rs, pr_api/forward.rs                # notice → PR by pr_url
├── integration.rs                             # subtask rollup skips replaced PRs
└── autofix/mod.rs                             # by PR id
app/src/
├── api.ts
├── components/{PrView.tsx,OpenPrDialog.tsx}(+ .test.tsx)
└── i18n/
docs/{usage.md,status.md,architecture.md,decisions.md}
```

**Structure Decision**: existing layout; one new module `pr_loop/capture.rs` for the hook
parsing so `phases.rs` stays a dispatcher.

## Delivery

The task is named "Multiple PRs": split by behaviour so each PR is usable on its own.
(1) **Data + per-PR plumbing**: migration, store by URL, snapshot `head_branch`, every
repo-keyed lookup (section 7), autofix by id (keeping `repo` = first counted open PR),
API fields, UI keyed by id. Behaviour unchanged: still found by branch in `repos()`.
(2) **More sources**: `pr_repos` (parent's own repo), agent capture, Link by URL for any
repo, Open PR on `.`, replaced PRs. (3) **Closing**: counted all-merged, re-detect before
archive, wait while working, turn-end nudge, archive heads; docs; then the **patch version
bump** (`scripts/bump-version.sh patch --commit`, its own commit, requested by the user
for this task).

Contract note: changes are additive in shape, but deliberate behaviour breaks for an old
app: Link by URL no longer answers 409 for another repo, and an old app keying cards or
autofix by `repo` acts on the first PR when a repo has two. The app ships with the
daemon, so this is accepted.

Before building capture, record a real `PostToolUse` payload for a Bash `gh pr create`
(no daemon code reads `tool_response` today) as a test fixture and confirm the
`tool_response.stdout`/`stderr` shape.

## Complexity Tracking

| Choice | Why needed | Simpler alternative rejected because |
|---|---|---|
| Rebuild `pr_notices` | Two PRs in one repo would share `(repo, kind, cause)` for `archive`/comment notices | Encoding the PR in `cause` breaks the existing resolve-by-kind queries and old rows |
| `replaced_by` column | A closed PR followed by its replacement must stop blocking the archive without deleting history | Deleting the closed row (today's replace) loses it when the second PR is in the same repo but a different branch |
