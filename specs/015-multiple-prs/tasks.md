# Tasks: Track every PR a task opens

**Input**: Design documents from `specs/015-multiple-prs/`
**Prerequisites**: plan.md, spec.md, research.md, data-model.md, contracts/api.md, quickstart.md

**Tests**: test-first (the project's convention): each phase writes the failing tests, then the code.

**Delivery** (plan.md › Delivery): three PRs, each green on its own.
- **PR 1**: Phase 1 + Phase 2 (data and per-PR plumbing; behaviour unchanged).
- **PR 2**: Phase 3 (US1) + Phase 4 (US3) (more sources of PRs, manual paths).
- **PR 3**: Phase 5 (US2) + Phase 6 (closing the task, docs, patch version bump).

## Format: `[ID] [P?] [Story] Description`

- **[P]**: can run in parallel (different files, no dependency on an unfinished task)
- **[Story]**: US1 every PR shows up · US2 close only when all merged · US3 manual paths

Checks: `cargo test -p delilah-git`, `cargo test -p delilah-daemon`,
`cd app && npx tsc --noEmit && npx vitest related --run <files>`.

---

## Phase 1: Setup

- [X] T001 Record a real Claude Code `PostToolUse` payload for a Bash `gh pr create` (stdout with the PR URL) and one for "a pull request already exists" (URL on stderr), redact paths, and save them as fixtures in `crates/delilah-daemon/tests/fixtures/hooks/post_tool_use_gh_pr_create.json` and `…_gh_pr_create_exists.json`; confirm the `tool_response.stdout` / `tool_response.stderr` shape plan.md §4 relies on, and note any difference in `specs/015-multiple-prs/research.md` (R2)
- [X] T002 [P] Add the parent fixture variant the tests need to the daemon's test helpers in `crates/delilah-daemon/src/test_support.rs` (or the module the PR tests already use for `make-fixture-parent`-style workspaces): a parent workspace with submodules `svc-a`, `svc-b`, the parent repo on the task's branch, and a fake `gh` answering per repo folder

---

## Phase 2: Foundational — data and per-PR plumbing (PR 1)

**Purpose**: a PR link is keyed by its URL and everything addresses a PR by id/URL instead of by repo. Detection still uses `Workspace::repos()` and today's merge rule, so behaviour does not change yet.

### Tests first

- [X] T003 Write a failing migration test in `crates/delilah-daemon/src/store_prs.rs` (tests module): seed `pull_requests` and `pr_notices` with rows shaped like 0011..0026 (with `error_kind`, `autofix` JSON, marks, a `ci-failed` notice of a live PR, a `comments` notice whose PR row is gone, an `archive-blocked` notice), run migrations to 0027, assert ids, every column and the `autofix` JSON survive, `pr_url` is backfilled for PR-kind notices, `''` for task-level ones, and the orphan notice is resolved (data-model.md; SC-004)
- [X] T004 [P] Write failing tests in `crates/delilah-daemon/src/store_prs.rs`: `link_pr` of two different URLs with the same `repo` keeps two rows; linking the same URL twice keeps one row and its marks/snapshot; linking a new PR does not resolve the notices of another PR of the same repo; `unlink_pr` of a PR clears `replaced_by` on rows that pointed at it
- [X] T005 [P] Write failing tests in `crates/delilah-daemon/src/gh/graphql.rs` and `crates/delilah-daemon/src/pr_types.rs`: the snapshot parses `headRefName` into `head_branch`; a stored snapshot JSON without `head_branch` deserialises with `None`
- [X] T006 [P] Write failing tests for the notice → PR lookups with two PRs in one repo: in `crates/delilah-daemon/src/pr_api/forward.rs` ("Ask the agent to fix it" on PR B's notice forwards PR B, not PR A) and in `crates/delilah-daemon/src/inbox.rs` (each item opens its own PR; the `pr-autofix` item points at its PR)
- [X] T007 [P] Write failing tests in `crates/delilah-daemon/src/autofix/mod.rs`: with two PRs in one repo, retry/cancel/stop act on the chosen PR only (`pr: <id>`) and leave the sibling's notices and autofix state alone; `repo` alone selects the first counted open PR of that repo (contracts/api.md)
- [X] T008 [P] Write a failing test in `crates/delilah-daemon/src/integration.rs` (`subtask_facts`): a subtask with a closed PR marked `replaced_by` and a merged replacement counts as merged in the parent-task rollup

### Implementation

- [X] T009 Write `crates/delilah-daemon/migrations/0027_pr_per_url.sql`: rebuild `pull_requests` (all current columns, explicit ids, `replaced_by INTEGER NULL`, `UNIQUE (task_key, url)` instead of `UNIQUE (task_key, repo)`) and `pr_notices` (`pr_url TEXT NOT NULL DEFAULT ''`, backfill from `pull_requests` by `(task_key, repo)` for `ci-failed`/`changes-requested`/`comments`/`pr-opened`, resolve orphans of those kinds, `UNIQUE (task_key, repo, pr_url, kind, cause)`), re-create indexes; no `PRAGMA foreign_keys`; make T003 pass
- [X] T010 Add `head_branch: Option<String>` (`#[serde(default)]`) to `Snapshot` in `crates/delilah-daemon/src/pr_types.rs`, `headRefName` to the query and parse in `crates/delilah-daemon/src/gh/graphql.rs` / `crates/delilah-daemon/src/gh/mod.rs`, and to the fake in `crates/delilah-daemon/src/gh/fake.rs`; make T005 pass
- [X] T011 Rework `crates/delilah-daemon/src/store_prs.rs`: `PrRow` gets `replaced_by`; `link_pr` upserts on `(task_key, url)` and no longer resolves other PRs' notices; every `(task_key, repo)` lookup (around lines 160, 233, 242, 299, 343) moves to id or URL; notices insert/resolve by `pr_url`; `unlink_pr` clears `replaced_by` pointing at the removed PR; add `counted(rows)` (rows with `replaced_by` NULL); make T004 pass
- [X] T012 Move notices to `pr_url` in `crates/delilah-daemon/src/pr_loop/notices.rs` (lines ~71, 89, 101) and `crates/delilah-daemon/src/pr_loop/mod.rs` (`record`, `detect`'s duplicate check becomes `r.url == url`)
- [X] T013 Match notices to PRs by `pr_url` in `crates/delilah-daemon/src/pr_api/forward.rs` (~268) and `crates/delilah-daemon/src/inbox.rs` (~281 `pr-autofix`, ~774); keep task-level `.` notices (~1821) as they are; make T006 pass
- [X] T014 Make autofix per PR in `crates/delilah-daemon/src/autofix/mod.rs` (~381-471, 853, 932, 964; `cancel_logged`, `resolve_stopped` by PR) and accept `pr: <id>` next to `repo` in the `POST /api/v1/tasks/{key}/autofix` body (handler in `autofix/mod.rs`, route in `crates/delilah-daemon/src/api.rs`); make T007 pass
- [X] T015 Filter `replaced_by IS NULL` in `subtask_facts` in `crates/delilah-daemon/src/integration.rs`; make T008 pass
- [X] T016 Grep `crates/delilah-daemon/src` for `\.repo ==`, `repo = ?` and `r.repo` and move every remaining "the PR of a repo" lookup to id/URL; fix the tests and fixtures that insert `(task_key, repo)` (`crates/delilah-daemon/src/store.rs` ~1447, `crates/delilah-daemon/src/jira_import.rs` ~1841)
- [X] T017 Add `replaced_by`, `head_branch` and `linked_by: "agent"` to the PR DTO in `crates/delilah-daemon/src/pr_api/dto.rs`, order `prs` by repo (`.`, submodules, outside repos) then id, and compute `PrSummary` over counted PRs in `crates/delilah-daemon/src/prs.rs` (`summary`); extend their tests
- [X] T018 [P] Mirror the contract in `app/src/api.ts` (`PullRequest.replaced_by`, `head_branch`, `linked_by` with `"agent"`, `setAutofix(key, action, repo?, pr?)`)
- [X] T019 Key PR cards by `pr.id` and call `api.setAutofix(owner, mode, undefined, pr.id)` in `app/src/components/PrView.tsx` (~207, ~298-308); write the test first in `app/src/components/PrView.test.tsx`: two PRs with the same `repo` render two cards, and retry on the second sends its id

**Checkpoint (PR 1)**: `cargo test -p delilah-daemon` and `cd app && npx tsc --noEmit && npx vitest run` green; existing single-PR and per-submodule behaviour unchanged.

---

## Phase 3: User Story 1 — Every PR the agent opens shows up (Priority: P1) (PR 2)

**Goal**: PRs in the parent's own repo, from other branches, in other repos and several per repo are tracked.

**Independent Test**: quickstart.md steps 1, 2 and 4 in a preview with the fixture parent.

### Tests first

- [X] T020 [P] [US1] Write failing tests in `crates/delilah-git/src/workspace.rs` (or `crates/delilah-git/tests/workspace.rs`): `pr_repos` returns the submodules plus `.` for a parent on the task's branch, only the submodules when the parent is not, and `[.]` for a plain repo
- [X] T021 [P] [US1] Write failing tests in `crates/delilah-daemon/src/pr_loop/mod.rs` with the T002 fixture: a PR from the task branch in the parent repo is detected with repo `.`; a second PR in `svc-a` from another branch (linked by hand) does not stop detection or get replaced; a repo whose only PR is closed is still searched and its new PR linked
- [X] T022 [P] [US1] Write failing tests in `crates/delilah-daemon/src/pr_loop/capture.rs` using the T001 fixtures: URLs from `tool_response.stdout` and from stderr ("already exists") are linked with `linked_by = "agent"`; a PR of a repo outside the workspace gets repo `host/owner/name`; a URL already linked is not duplicated; an unlinked URL is not linked again; output of `gh pr view` / `gh pr list` / `echo <url>` is ignored (FR-004); a URL `gh` cannot read is reported once and not linked
- [X] T023 [P] [US1] Write a failing test in `crates/delilah-daemon/src/store_prs.rs` for replacement (FR-009): linking a PR with the same repo and head branch as a closed PR sets the old one's `replaced_by` and resolves its notices; a merged or open PR is never marked replaced

### Implementation

- [X] T024 [US1] Add `Workspace::pr_repos(&self, opts: &GitOptions)` in `crates/delilah-git/src/workspace.rs` (`repos()` + `(".", path)` when `parent_has_branch(opts)`), with doc comments on `repos` and `pr_repos` saying which callers use which; make T020 pass
- [X] T025 [US1] In `crates/delilah-daemon/src/pr_loop/mod.rs` `run_check`, iterate `pr_repos` and skip a repo only when it has a counted `Open`/`Merged` PR whose head branch is the task's branch (data-model.md › Repos with a PR); make T021 pass
- [X] T026 [US1] Implement replacement in `link_pr` in `crates/delilah-daemon/src/store_prs.rs` (same repo + head branch + `Closed` → `replaced_by`, notices resolved); make T023 pass
- [X] T027 [US1] Write `crates/delilah-daemon/src/pr_loop/capture.rs`: `on_bash_output(app, key, hook)` takes commands containing `gh pr create`, extracts URLs with `gh::parse_pr_url` from `tool_response.stdout`/`stderr`, skips linked/unlinked ones, reads each with `gh.pr_view_url`, labels it with `repo_of`, links it with `link_known(.., "agent")`; errors through the existing `failed()` reporting; make T022 pass
- [X] T028 [US1] Call `capture::on_bash_output` from `crates/delilah-daemon/src/phases.rs` `on_hook` on `PostToolUse` Bash hooks of the task (spawned, never awaited in the hook; keep the existing `nudge`)
- [X] T029 [US1] Show the source and outside-repo label in `app/src/components/PrView.tsx` (group heading `host/owner/name` for outside repos, "agent" source); test first in `app/src/components/PrView.test.tsx`; strings in `app/src/i18n/en.ts` and the Spanish source (FR-018)

**Checkpoint**: quickstart steps 1, 2, 4 pass in a preview.

---

## Phase 4: User Story 3 — Linking, opening and unlinking any of them by hand (Priority: P2) (PR 2)

**Goal**: Link by URL accepts any readable repo; Open PR offers the parent's own repo; unlink acts on one PR.

**Independent Test**: quickstart step 3, plus unlinking one of two PRs.

### Tests first

- [X] T030 [P] [US3] Change `linking_a_pr_of_another_repo_is_409` in `crates/delilah-daemon/src/pr_api/mod.rs` into a failing "is linked with the `host/owner/name` label" test; add: a PR of the parent repo links as `.`; with no workspace, a parent project's own checkout and initialised submodules are matched before the host label; unlinking one of two PRs leaves the other and detection does not bring it back
- [X] T031 [P] [US3] Write failing tests in `crates/delilah-daemon/src/pr_api/open.rs`: for a parent task on its branch in the parent repo, the proposal's `repos` includes `.` while it has no open PR from the task's branch, and opening with `repo = "."` pushes and creates from the parent folder; unknown repo still 400
- [X] T032 [P] [US3] Write a failing test in `app/src/components/OpenPrDialog.test.tsx`: the repo select offers `.` (labelled as the parent repository) when the proposal lists it

### Implementation

- [X] T033 [US3] Rework `repo_of` in `crates/delilah-daemon/src/pr_loop/mod.rs`: search `pr_repos`; without a workspace use the project's checkout (`.`) and, for a parent, `Workspace::home_of` per submodule; otherwise `host/owner_name`; remove `LinkError::NotFromTask` and its 409 mapping in `crates/delilah-daemon/src/pr_api/mod.rs`; make T030 pass
- [X] T034 [US3] Use `pr_repos` and the "repos with a PR" rule for the proposal's `repos` and the open handler in `crates/delilah-daemon/src/pr_api/open.rs`; make T031 pass
- [X] T035 [US3] Label `.` as the parent repository in `app/src/components/OpenPrDialog.tsx` for parent tasks; make T032 pass

**Checkpoint (PR 2)**: daemon and app suites green; quickstart steps 1–4 pass.

---

## Phase 5: User Story 2 — The task closes only when every PR is merged (Priority: P1) (PR 3)

**Goal**: archive only when every counted PR is merged, after one more look, and never mid-turn.

**Independent Test**: quickstart steps 5 and 6.

### Tests first

- [X] T036 [P] [US2] Write failing tests in `crates/delilah-daemon/src/pr_loop/merge.rs`: `all_merged` ignores replaced PRs and is false with no counted PR; two PRs, one merged → not archived; both → archived once; merged + closed → not archived, notice raised; merged + closed-but-replaced-by-merged → archived
- [X] T037 [P] [US2] Write failing tests in `crates/delilah-daemon/src/pr_loop/mod.rs`: the only known PR merged while another open PR exists from the task branch in another repo of `pr_repos` → the pre-archive forced detection links it and the task is not archived (FR-011); all merged while the primary session is `Working` → not archived, then archived on the check after the turn ends (FR-012)
- [X] T038 [P] [US2] Write failing tests in `crates/delilah-daemon/src/pr_loop/merge.rs` for the archive heads (FR-013): a merged parent-repo PR verifies and deletes the parent's task branch; a parent without a PR keeps its branch; PRs from another branch or outside the workspace touch nothing local; two merged PRs from the task branch in one label → the highest id's head is used
- [X] T039 [P] [US2] Write a failing test in `crates/delilah-daemon/src/phases.rs`: `TurnEnded` on a task with PRs nudges the PR loop

### Implementation

- [X] T040 [US2] Make `all_merged` work over `counted(rows)` in `crates/delilah-daemon/src/pr_loop/merge.rs`; make T036 pass
- [X] T041 [US2] In `run_check` (`crates/delilah-daemon/src/pr_loop/mod.rs`), before `on_merged`: return if the task's primary session is `LiveState::Working`; else, unless this call was forced, run one forced detection over `pr_repos`, re-read rows and re-evaluate; make T037 pass
- [X] T042 [US2] Build `heads` in `archive_merged`/`verify` (`crates/delilah-daemon/src/pr_loop/merge.rs` ~225-290) from counted merged PRs with a `pr_repos` label and head branch = task branch or `None`, highest id per label, checking labels from `pr_repos`; make T038 pass
- [X] T043 [US2] Nudge the PR loop on `Signal::TurnEnded` for tasks with PRs in `crates/delilah-daemon/src/phases.rs`; make T039 pass
- [X] T044 [US2] Show replaced PRs muted with "replaced" and out of the badge count in `app/src/components/PrView.tsx` (test first in `app/src/components/PrView.test.tsx`); strings in `app/src/i18n/en.ts` and the Spanish source

**Checkpoint**: quickstart steps 5 and 6 pass in a preview.

---

## Phase 6: Polish & cross-cutting (PR 3)

- [X] T045 [P] Update the Pull requests section of `docs/usage.md` (every PR the task opens is tracked: parent repo, other branches, other repos, agent-created; archive only when all merged and not mid-turn; Link by URL for any repo)
- [X] T046 [P] Update `docs/status.md` (built), `docs/architecture.md` (`pull_requests` per URL, `replaced_by`, `pr_notices.pr_url`, `pr_repos`) and add a decision to `docs/decisions.md` ("a task's PR links are per PR, not per repo; the agent's `gh pr create` output is a link source; Link by URL accepts any readable repo")
- [X] T047 Run the full checks (`cargo test --workspace`, `cargo test --manifest-path app/src-tauri/Cargo.toml`, `cd app && npx tsc --noEmit && npx vitest run`) and walk quickstart.md steps 1–7 in a preview (`scripts/preview.sh --fresh`), never against the installed daemon — DONE except: Tauri tests (missing sidecar/dist, environmental) and the quickstart preview walk, left to the user; T048 done WITHOUT --commit (1.4.1 → 1.4.2, uncommitted)
- [X] T048 Bump the patch version, as the user asked for this task, in its own commit `chore: bump version to X.Y.Z`: `scripts/bump-version.sh patch --commit` (writes `Cargo.toml`, `app/src-tauri/Cargo.toml`, `app/package.json`, `app/src-tauri/tauri.conf.json`, the lockfiles and `CHANGELOG.md`; needs `git-cliff`), on the last PR's branch (docs/development.md › Releasing)

---

## Dependencies & Execution Order

- **Phase 1** → **Phase 2** (PR 1) → **Phase 3** and **Phase 4** (PR 2; US3 needs T024 `pr_repos` from US1) → **Phase 5** (PR 3; needs `pr_repos`, `head_branch`, `counted`) → **Phase 6**; T048 is last.
- Within a phase: tests first (they fail), then implementation tasks in order; T011 before T012–T017.

### Parallel opportunities

- Phase 2 tests T004–T008 touch different files: all at once. T018 (app types) alongside the daemon work.
- Phase 3 tests T020–T023; Phase 4 tests T030–T032; Phase 5 tests T036–T039.
- T045 and T046 (docs) together.

## Parallel Example: User Story 1

```text
T020 workspace.rs tests  |  T021 pr_loop/mod.rs tests  |  T022 capture.rs tests  |  T023 store_prs.rs tests
```

## Implementation Strategy

- **MVP**: PR 1 + Phase 3 (US1): every PR shows up. US2 is the other half of the bug and goes in PR 3, right after.
- Each PR is green and usable on its own; PR 1 changes no behaviour.

## Summary

- 48 tasks: Setup 2, Foundational 17, US1 10, US3 6, US2 9, Polish 4.
