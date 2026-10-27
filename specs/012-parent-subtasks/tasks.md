# Tasks: Parent tasks and subtasks (integration branch)

**Input**: Design documents from `specs/012-parent-subtasks/`
**Prerequisites**: plan.md, spec.md, research.md (R1–R14), data-model.md, contracts/api.md, quickstart.md

**Tests**: included. Implementation is orchestrated test-first; each test task is written
before the code it covers and must fail first.

**Naming**: always "parent task" in code and copy (`TaskKind::ParentTask`, `parent_task`); never
bare "parent", which means a parent repository (`ProjectKind::Parent`) — research R1.

**Checks**: `cargo test -p <crate>`; UI `cd app && npx tsc --noEmit && npx vitest related --run <files>`.
Try changes only with `scripts/preview.sh` (never port 7420).

## Format: `[ID] [P?] [Story] Description`

- **[P]**: can run in parallel (different files, no dependency on an unfinished task)
- **[Story]**: US1–US5 from spec.md

---

## Phase 1: Setup

- [X] T001 Add the empty module `crates/delilah-core/src/hierarchy.rs` and export it from `crates/delilah-core/src/lib.rs`; add the empty modules `crates/delilah-daemon/src/integration.rs` and `crates/delilah-daemon/src/api_hierarchy.rs` and declare them in `crates/delilah-daemon/src/lib.rs` (or `main.rs`, wherever the daemon declares modules)

---

## Phase 2: Foundational (blocks every story)

### Core model

- [X] T002 [P] Write unit tests in `crates/delilah-core/src/hierarchy.rs` for: `progress()` (merged/counted/dropped/ready, zero subtasks ⇒ not ready, all dropped ⇒ counted 0), `is_live`, and every `Refusal` from `check_new_subtask`, `check_make_parent_task`, `check_attach`, `check_start_subtask` (data-model §Invariants, contracts §Refusals)
- [X] T003 Implement `TaskKind { Normal, ParentTask, Subtask }` (serde `"normal"|"parent_task"|"subtask"`, default `Normal`), `ChildFacts { merged, archived }`, `Progress { merged, counted, dropped, ready }`, `progress`, `is_live`, the `Refusal` enum (one variant per contract reason, no strings) and the four `check_*` functions in `crates/delilah-core/src/hierarchy.rs` until T002 passes
- [X] T004 Add `kind: TaskKind` and `parent_task: Option<TaskKey>` to `struct Task` in `crates/delilah-core/src/task.rs` with `#[serde(default)]` / `skip_serializing_if`; test that old JSON without the fields reads as a normal task
- [X] T005 [P] Add `MergePreference { Merge, Squash, Rebase }` (serde lowercase, default `Merge`) and `ProjectSettings.parent_task_merge` with `#[serde(default)]` in `crates/delilah-core/src/project.rs`; test that old settings JSON reads `merge`

### Storage

- [X] T006 Create `crates/delilah-daemon/migrations/0025_task_hierarchy.sql` (data-model §Migration: `kind TEXT NOT NULL DEFAULT 'normal'`, `parent_task TEXT`, index `tasks_parent_task`), with a comment above as in 0023
- [X] T007 Read and write `kind` and `parent_task` in `crates/delilah-daemon/src/store.rs`: `insert_task` columns and binds (~L595), the SELECT lists of `get_task`/`list_tasks` (~L106–124), `task_from_row` (~L681); add `subtasks_of(parent_key) -> Vec<Task>`, `set_kind(key, kind, parent_task)` and `clear_parent_task_links(parent_key)`; tests: round trip, `subtasks_of`, and a pre-upgrade DB (0024 schema with rows) migrates with every task `normal`

### Git

- [X] T008 [P] Write tests in `crates/delilah-git/src/base.rs` (fixture repos with a bare origin) for a base override: `prepare_base` with `Some("feat/x")` resolves `origin/feat/x` after fetch, never the default branch; missing ref ⇒ `GitError::BaseMissing { branch }`; fetch failure with an existing ref ⇒ stale warning as today
- [X] T009 Add `WorkspaceSpec.base: Option<String>` in `crates/delilah-git/src/workspace.rs` (~L19) and pass it through `create_workspace` (~L218, L251) to `prepare_base`; make the base ref a parameter of one shared resolver in `crates/delilah-git/src/base.rs` (no second resolver); add `GitError::BaseMissing`; update every `WorkspaceSpec { .. }` literal in the workspace (grep) with `base: None`; T008 passes
- [X] T010 [P] Add `commit_empty(dir, message)` (`git commit --allow-empty -m`) and `merge_refs(dir, &[String])` (merge each ref in order, on conflict `git merge --abort` and reset to the starting commit) in `crates/delilah-git/src/workspace.rs`, with tests on fixture repos

### Daemon foundation

- [X] T011 Write tests for `crates/delilah-daemon/src/integration.rs`: `base_for` (normal/parent task ⇒ default; subtask ⇒ parent task's `Workspace.branch`; subtask whose parent task has no workspace ⇒ error), `subtask_facts`/`progress` (merged = ≥1 PR and all merged; archived = terminal + no workspace row; deleted children absent), `live_subtasks`, `preferred_method` (subtask ⇒ squash; parent task ⇒ setting; normal ⇒ `allowed[0]`; preferred not allowed ⇒ `allowed[0]`)
- [X] T012 Implement `crates/delilah-daemon/src/integration.rs` until T011 passes: `Base { Default, Branch(String) }`, `base_for`, `subtask_facts` (one `subtasks_of` query + one PR query, no per-child round trips), `progress`, `live_subtasks`, `preferred_method(kind, &ProjectSettings, &[MergeMethod])` converting `MergePreference` ↔ `pr_types::MergeMethod`. Rule: no `gh` or git calls in this module, only store queries
- [X] T013 Map every `hierarchy::Refusal` to a 409 body `{ error, message, ...detail }` with Spanish/English messages in `crates/delilah-daemon/src/lang.rs` (reason strings from contracts §Refusals live only here)
- [X] T014 [P] Add notice kinds `parent-task-live` and `parent-task-ready` to `crates/delilah-daemon/src/inbox.rs` (KIND_* constants ~L28 and `ItemKind` ~L131), following the `NewNotice` pattern in `store_prs.rs:290`
- [X] T015 [P] Mirror the contract types in `app/src/api.ts`: `TaskKind`, `Task.kind` (absent ⇒ `"normal"` on read), `Task.parent_task`, `Task.subtask_progress`, `Progress`, `SubtaskSummary`, `ProjectSettings.parent_task_merge` and `DEFAULT_PROJECT_SETTINGS.parent_task_merge = "merge"` (~L168); add `api.subtasks`, `api.makeParentTask`, `api.attach`, `api.pushBranch`, `api.markReady`; PR type gains `method`, `base_branch`, `expected_base`; base-status gains `subtask_commits`, `base`; open-PR body `empty_commit`; merge body `confirm_live_subtasks`

**Checkpoint**: core, storage, git and `integration.rs` pass their tests; stories can start.

---

## Phase 3: User Story 1 — Split a feature into a parent task and subtasks (P1) 🎯 MVP

**Goal**: create a parent task and subtasks (UI, CLI, API), start the parent task (worktree, pushed branch, no agent) and its subtasks in parallel, each branched from the integration branch.

**Independent Test**: spec US1 / quickstart steps 1–2: subtasks' worktrees contain `origin/<parent task branch>`; starting a subtask before its parent task is refused.

### Tests

- [X] T016 [P] [US1] API tests in `crates/delilah-daemon/src/api_hierarchy.rs` (or `crates/delilah-daemon/tests/`) for create: `kind: parent_task` stores workflow none and saving mode/autopilot/CI autofix off; `parent_task` set ⇒ subtask in the parent task's project; refusals `not-a-parent-task`, `parent-task-archived`, `nesting`, `project-mismatch`; free keys from `next_free_key`
- [X] T017 [P] [US1] Daemon e2e test in `crates/delilah-daemon/tests/parent_tasks.rs` (fake Claude, fixture repo with bare origin): starting a parent task makes a worktree (even with project workspace mode = checkout), pushes its branch, launches no session, status `Running`; restarting the daemon leaves it `Running` (startup recovery, idle/stale detection); an extra "+ Claude" session ending does not move it; a done parent task or subtask reopened keeps its kind and parent task; starting a subtask before the parent task ⇒ `parent-task-not-started`; two subtasks start in parallel from `origin/<parent task branch>` with the parent task's submodules; missing remote branch ⇒ `parent-task-branch-missing`

### Implementation

- [X] T018 [US1] Extend `NewTask` (`crates/delilah-daemon/src/api.rs` ~L318) with `kind`, `parent_task`; validate in `build_task_with` (~L481) through `hierarchy::check_new_subtask`; force the subtask's project and the parent task's agent fields per data-model §Invariants
- [X] T019 [US1] Return `TaskDto` (`#[serde(flatten)] Task` + `subtask_progress: Option<Progress>` for parent tasks, from `integration::progress`) from GET `/api/v1/tasks` and `/api/v1/tasks/{key}` in `crates/delilah-daemon/src/api.rs`; keep the list to one progress query per parent task
- [X] T020 [US1] In `crates/delilah-daemon/src/integration.rs` add `start_rules(app, &Task) -> StartPlan { in_checkout: false, branchless: false, base: Option<String>, submodules: Option<Vec<String>>, launch_agent: bool }` (subtask: `check_start_subtask`, base and submodules from the parent task's workspace; parent task: `launch_agent = false`)
- [X] T021 [US1] Call `integration::start_rules` from `crates/delilah-daemon/src/run.rs`: override `in_checkout` (~L284), branchless (~L395–416), `WorkspaceSpec.base`/`submodules` (~L509–513 and the other `create_workspace` ~L2005); for a parent task, after `WorkspaceReady` push the branch (`push_branch`) and skip the primary session; map `GitError::BaseMissing` to `parent-task-branch-missing`. Verify no recovery or idle path sends `AgentFinished`/moves a parent task; fix there if one does. T017 passes
- [X] T022 [P] [US1] In `crates/delilah-daemon/src/branching.rs` make `starts_branchless` (~L143) false for parent tasks and subtasks, and skip them in the spec-name branch adoption (`branching::pending`)
- [X] T023 [US1] Add `POST /api/v1/tasks/{key}/push-branch` (parent task only; `push_branch`) and `GET /api/v1/tasks/{key}/subtasks` (`SubtaskSummary[]`, 404 for other kinds) in `crates/delilah-daemon/src/api_hierarchy.rs`, routed from `api.rs`
- [X] T024 [P] [US1] CLI in `crates/delilah-cli/src/main.rs`: `new --parent-task` and `new --parent <KEY>` (mutually exclusive) (~L816); `ls` prints subtasks indented under their parent task with `N/M` (~L798); `show` adds kind, parent task, progress and the subtask list (~L963); refusals print the message and exit 1; tests for argument parsing and output formatting
- [X] T025 [P] [US1] `app/src/components/NewTaskDialog.tsx`: kind choice (normal / parent task); opened from a parent task, creates a subtask of it; workspace choice hidden for both kinds; agent options (workflow, first message, saving mode, autopilot, model) hidden for a parent task; vitest
- [X] T026 [P] [US1] `app/src/components/Sidebar.tsx`: subtasks nested and collapsible under their parent task (grouping ~L79), "N/M" badge on the parent task; a subtask whose parent task is not in the list shows at top level; vitest
- [X] T027 [US1] New `app/src/components/SubtaskList.tsx` (key, title, state, PR state, dropped; "New subtask" button) used in `app/src/components/TaskScreen.tsx` for parent tasks; `app/src/components/TaskHeader.tsx` shows progress instead of "Running" for a parent task, a link to the parent task for a subtask, and "Push branch again" when the parent task's branch is missing; Spanish strings in place, English in `app/src/i18n/en/`; vitest

**Checkpoint**: US1 works end to end in a preview (quickstart 1–2).

---

## Phase 4: User Story 2 — Subtask PRs go to the integration branch (P1)

**Goal**: subtask PRs target the parent task's branch; "behind" and Update branch use it; Merge uses squash.

**Independent Test**: spec US2 / quickstart step 3 (fake `gh` records `--base`).

### Tests

- [X] T028 [P] [US2] Tests in `crates/delilah-daemon/src/pr_api/open.rs` (fake `gh`): for a subtask, proposal commits and `pr_create --base` use the parent task's branch on every path that opens a PR (`open_for`, `create_and_link`, the literal `--base` ~L467, autopilot/forward/hand-off callers); a test fails if any path passes the default branch for a subtask
- [X] T029 [P] [US2] Tests in `crates/delilah-daemon/src/base_update.rs` (~L355+ test module): subtask behind count and Update branch against `origin/<parent task branch>`; conflict leaves every repo at its starting commit

### Implementation

- [X] T030 [US2] Replace `base_of` (`crates/delilah-daemon/src/pr_api/open.rs` ~L85, used ~L102, ~L236) and the literal base (~L467) with `integration::base_for`; opening a subtask PR when `origin/<parent task branch>` is missing is refused with `parent-task-branch-missing` (spec edge case; test in T028); T028 passes
- [X] T031 [US2] In `crates/delilah-daemon/src/base_update.rs`, take the refs to compare and merge from `integration::base_for` (subtask: `origin/<parent task branch>`) in `inspect` (~L113), `update` (~L152) and `merge_all` (~L179) using `delilah_git::merge_refs`; base-status gains `base`; T029 passes
- [X] T032 [US2] `crates/delilah-daemon/src/pr_api/merge.rs` (~L97–112): without a method in the request use `integration::preferred_method`; `crates/delilah-daemon/src/pr_api/dto.rs` gains `method`, `base_branch`, `expected_base`; tests: subtask ⇒ squash, squash not allowed ⇒ `allowed[0]`
- [X] T033 [US2] `app/src/components/PrView.tsx`: show `pr.method` instead of `merge_methods[0]` (~L329, L448, L456); warning naming the target branch when `base_branch ≠ expected_base` for a subtask (no retarget action); vitest

**Checkpoint**: US1 + US2 = the MVP: integration branch fed by subtasks.

---

## Phase 5: User Story 3 — Parent task progress and its own PR (P2)

**Goal**: progress everywhere; parent task PR draft (empty commit when needed), Update branch brings in merged subtasks first, merge method from the setting with confirmation, ready suggestion.

**Independent Test**: spec US3 / quickstart steps 4–5.

### Tests

- [ ] T034 [P] [US3] Tests in `crates/delilah-daemon/src/pr_api/` (fake `gh`): parent task PR opened draft unless progress is ready; `no-commits` then `empty_commit: true` commits `chore: start <KEY> <title>`, pushes and opens; `empty_commit` on another kind ⇒ `empty-commit-not-allowed`; `ready` endpoint calls `gh pr ready`; merge with live subtasks and no confirmation ⇒ `live-subtasks`; with confirmation ⇒ merges with the setting's method
- [ ] T035 [P] [US3] Tests in `crates/delilah-daemon/src/base_update.rs`: parent task Update branch merges `origin/<own branch>` then the default branch; `subtask_commits` counts commits on `origin/<own branch>` not in `HEAD`; tracked changes or a conflict ⇒ refused/undone
- [ ] T036 [P] [US3] Tests in `crates/delilah-daemon/src/pr_loop/` : a subtask PR merge emits `Topic::Tasks` for the parent task; `parent-task-ready` notice pushed once when progress first becomes ready with a draft PR, not again on later ticks; two subtasks merged in one tick give the right progress

### Implementation

- [ ] T037 [US3] `crates/delilah-daemon/src/pr_api/open.rs`: force `draft` for a parent task unless `progress.ready`; `OpenRequest.empty_commit` path with `delilah_git::commit_empty` + push; T034 (open part) passes
- [ ] T038 [US3] `crates/delilah-daemon/src/gh/mod.rs`: `pr_ready(url)`; `POST /api/v1/tasks/{key}/prs/{id}/ready` handler in `crates/delilah-daemon/src/pr_api/` routed from `api.rs`; never called by the daemon on its own
- [ ] T039 [US3] `crates/delilah-daemon/src/pr_api/merge.rs`: `MergeRequest.confirm_live_subtasks`; refuse with `live-subtasks` and the keys when false and `integration::live_subtasks` is not empty
- [ ] T040 [US3] `crates/delilah-daemon/src/base_update.rs`: parent task refs = `origin/<own branch>` then default; `subtask_commits` in base-status; T035 passes
- [ ] T041 [US3] `crates/delilah-daemon/src/pr_loop/mod.rs` / `pr_loop/merge.rs`: on a subtask merge emit `Topic::Tasks` for its parent task; push `parent-task-ready` once (dedupe by parent task key + PR); T036 passes
- [ ] T042 [P] [US3] `app/src/components/ProjectSettings.tsx`: "Parent task merge method" (merge / squash / rebase), noting it falls back to what the repository allows; vitest
- [ ] T043 [US3] `app/src/components/PrView.tsx` for a parent task: draft badge; "Mark ready" suggestion when `subtask_progress.ready` and the PR is a draft; on `no-commits` offer "Add an empty commit and open"; on `live-subtasks` a confirmation dialog naming them, retrying with `confirm_live_subtasks`; base banner with "N merged subtask commits not in your workspace" (`subtask_commits`); i18n; vitest

**Checkpoint**: parent task PR flow complete (quickstart 4–5).

---

## Phase 6: User Story 4 — Archive and delete rules (P2)

**Goal**: no parent task archived or deleted while it has live subtasks; merged parent task with live subtasks is done, not archived, with a notice.

**Independent Test**: spec US4 / quickstart step 6.

- [ ] T044 [P] [US4] Tests: `actions::archive` and `actions::delete` on a parent task with a live subtask ⇒ 409 `live-subtasks` naming it (`--force` does not override); with only archived subtasks ⇒ allowed, and delete clears their `parent_task`; `archive_merged` on a parent task with live subtasks ⇒ `Done`, workspace kept, `parent-task-live` notice; a subtask's own archive/delete unchanged (`crates/delilah-daemon/src/actions.rs`, `crates/delilah-daemon/src/pr_loop/merge.rs` test modules)
- [ ] T045 [US4] `crates/delilah-daemon/src/actions.rs` (~L103–170): `integration::live_subtasks` check in archive and delete; delete of a parent task calls `store.clear_parent_task_links`; archiving or deleting a subtask emits `Topic::Tasks` for its parent task so progress refreshes (SC-006)
- [ ] T046 [US4] `crates/delilah-daemon/src/pr_loop/merge.rs::archive_merged` (~L51–116): after `MarkDone`, if the task is a parent task with live subtasks skip release/archive and push `parent-task-live`; T044 passes
- [ ] T047 [P] [US4] CLI `archive`/`rm` and UI archive/delete actions (`app/src/components/TaskHeader.tsx`, `crates/delilah-cli/src/main.rs`) show the refusal naming the live subtasks; vitest

---

## Phase 7: User Story 5 — Turn an existing task into a parent task or a subtask (P3)

**Goal**: convert a normal task into a parent task; attach a never-started task as a subtask.

**Independent Test**: spec US5 / quickstart step 7.

- [ ] T048 [P] [US5] API tests for `POST /api/v1/tasks/{key}/make-parent-task` (refusals `has-open-pr`, `has-session`, `in-checkout`, `nesting`; a converted running task keeps its workspace, its agent fields are turned off and it starts no new agent) and `POST /api/v1/tasks/{key}/attach` (`already-started`, `not-a-parent-task`, `parent-task-archived`, `nesting`, project change to the parent task's)
- [ ] T049 [US5] Implement both handlers in `crates/delilah-daemon/src/api_hierarchy.rs` with `hierarchy::check_make_parent_task` / `check_attach` and `store.set_kind`; emit `Topic::Tasks`; T048 passes
- [ ] T050 [P] [US5] CLI `make-parent-task <KEY>` and `attach <KEY> --parent <KEY>` in `crates/delilah-cli/src/main.rs`
- [ ] T051 [P] [US5] `app/src/components/BacklogTaskView.tsx` (and the task menu in `TaskHeader.tsx` for started normal tasks): "Make parent task" and "Attach to parent task…" (picker of the project's non-archived parent tasks), showing refusal reasons; vitest

---

## Phase 8: Polish & cross-cutting

- [ ] T052 [P] `docs/decisions.md`: new decision — task kinds, the integration branch pattern, stacked PRs not supported, progress derived on read, naming "parent task" vs parent repository (FR-030)
- [ ] T053 [P] `docs/usage.md`: parent tasks and subtasks as the user sees them (create, start, PRs, Update branch, merge methods, Mark ready, archive rules, CLI); called the "integration branch" pattern
- [ ] T054 [P] `docs/status.md` (built; Jira adapter pending) and `docs/architecture.md` (task kinds, `hierarchy.rs`, `integration.rs`, migration 0025)
- [ ] T055 Full checks: `cargo test --workspace`, `cargo test --manifest-path app/src-tauri/Cargo.toml`, `cd app && npx tsc --noEmit && npx vitest run`
- [ ] T056 Run quickstart.md in a preview (`scripts/preview.sh --fresh`, `DELILAH_PR_POLL_SECS=5`); UI checks through `ui-verifier`

---

## Dependencies & execution order

- Phase 1 → Phase 2. In Phase 2: T002–T005 (core) first; then storage (T006–T007) and git (T008–T010) in parallel; T011–T012 need core + storage; T013–T015 in parallel once core exists.
- US1 (Phase 3) needs all of Phase 2. US2 needs US1 (started subtasks). US3 needs US2 (PR paths through `base_for`). US4 needs Phase 2 + T018 (creating subtasks); it can run in parallel with US2/US3 (different files: `actions.rs`, `pr_loop/merge.rs` archive part). US5 needs Phase 2 + T023 (`api_hierarchy.rs` exists) and can run in parallel with US2–US4.
- File conflicts to serialise: `pr_api/open.rs` (T030 → T037), `pr_api/merge.rs` (T032 → T039), `base_update.rs` (T031 → T040), `PrView.tsx` (T033 → T043), `pr_loop/merge.rs` (T041 → T046), `TaskHeader.tsx` (T027 → T047 → T051), `main.rs` CLI (T024 → T047 → T050), `api_hierarchy.rs` (T023 → T049).
- Phase 8 last.

### Parallel examples

- Phase 2: T005 ∥ T008 ∥ T010 ∥ T014 ∥ T015 once T003 is in.
- US1: T016 ∥ T017 (tests); then T022 ∥ T024 ∥ T025 ∥ T026 alongside T018–T021.
- After US1: US2 (daemon PR paths) ∥ US4 (archive rules) ∥ US5 (convert/attach), then US3.

## Implementation strategy

- **MVP**: Phases 1–4 (US1 + US2): parent tasks, subtasks in parallel, PRs into the integration branch with squash. Usable without the progress UI.
- Then US3 (progress, parent task PR), US4 (safety rules), US5 (conversion), docs.
- Commit per phase; no version bump; the PR title is a Conventional Commit (`feat: parent tasks and subtasks (integration branch)`).
