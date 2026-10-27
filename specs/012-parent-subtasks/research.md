# Research: Parent tasks and subtasks (integration branch)

Decisions R1–R14. Code references are to the tree at the start of the branch.

## R1. Names: `TaskKind::ParentTask`, field `parent_task`

- **Decision**: `TaskKind { Normal, ParentTask, Subtask }` serialised `"normal" | "parent_task" |
  "subtask"`; the subtask's link is `parent_task: Option<TaskKey>`; columns `tasks.kind`,
  `tasks.parent_task`. UI copy: "Parent task" / "Subtask" (Spanish UI: "Tarea madre" /
  "Subtarea").
- **Rationale**: `ProjectKind::Parent` (serialised `"parent"`) already means a repository with
  submodules and appears in 18 files (`project.rs:8`, `run.rs`, `branching.rs`, `pr_api/open.rs`,
  `delilah-git/src/workspace.rs`…). A bare `parent` / `Parent` would be ambiguous in the same
  functions (`run.rs` handles both).
- **Alternatives**: `TaskKind::Parent` + `parent_key` (clashes in grep and in review);
  `Integration` / `integration_task` (accurate but not the user's vocabulary).

## R2. Where the hierarchy rules live

- **Decision**: a pure module `crates/delilah-core/src/hierarchy.rs`: `TaskKind`, the checks
  (`can_add_subtask(parent)`, `can_make_parent_task(task, has_open_pr, has_session, in_checkout)`,
  `can_attach(task, parent)`), `Progress { merged, counted, dropped }` built from a slice of child
  summaries, and `is_live(child)`. The daemon gathers the facts (PRs, workspace, sessions) and
  asks the core.
- **Rationale**: the rules are the testable heart of the feature and need no I/O; the daemon
  already follows this split for task states (`task.rs` transitions).

## R3. "Archived", "live", "merged", "dropped" from existing data

- **Decision**: no new state. A task is **archived** when its status is terminal and it has no
  workspace row (the same test `store.rs:391` `set_title` uses). **Merged** = the subtask has at
  least one PR and all its PRs are `PrState::Merged` (the test `pr_loop::merge::all_merged`
  uses). **Dropped** = archived and not merged. **Live** = not archived (deleted rows are gone).
- **Consequence for the spec**: a deleted subtask disappears from the list (its row is
  deleted); only archived-unmerged ones show as dropped. FR-022 is worded accordingly.

## R4. A parent task's lifecycle without an agent

- **Decision**: `run.rs` starts a parent task like any task up to `WorkspaceReady` (status
  `Running`), then pushes the branch and **launches no primary session**. The task stays
  `Running` until `MarkDone` (merge or by hand); `AgentFinished` never comes. "+ Claude" and
  "+ Terminal" open extra sessions as today; their ending does not move the task. The UI shows a
  running parent task by its progress ("2/5 subtasks merged"), not as "Running".
- **Rationale**: reuses the state machine unchanged (`task.rs:574`, `591`); a new status would
  touch every match on `TaskStatus` across daemon and UI.
- **Alternatives**: a new `Open` status (wide blast radius); starting the agent with no first
  message (`--no-prompt`) and letting it idle (costs a session the user did not ask for).
- Workflow, saving mode, autopilot and CI autofix are stored as "none/off" for a parent task at
  creation (`build_task_with`), so no phase logic ever runs on it.

## R5. Always a worktree for parent tasks and subtasks

- **Decision**: in `run.rs` (`in_checkout` resolution, line ~284) the kind wins: `ParentTask` and
  `Subtask` force `in_checkout = false`, ignoring the request, the task and the project setting.
  The create/start forms hide the workspace choice for them. Converting a task in the user's
  checkout into a parent task is refused (spec FR-008).

## R6. Branch from the parent task's branch: `WorkspaceSpec.base`

- **Decision**: `delilah_git::WorkspaceSpec` gains `base: Option<String>` (a branch name on
  `origin`). `prepare_base` takes it: after the usual bounded fetch it resolves
  `refs/remotes/origin/<base>` only, never the default branch. If that ref is missing, it returns a
  new `GitError::BaseMissing { branch }` and the start is refused ("The parent task's branch is
  not on origin: push it again from the parent task"). If the fetch fails but the ref exists from
  an earlier fetch, the start goes on with the stale warning, as for normal tasks (decision 66).
- For a parent repository, every repo of the subtask (the parent task's repos, R7) uses the same
  branch name, as tasks already share one branch name across repos (`Workspace.branch`).
- **Alternatives**: branching from the parent task's local workspace (would carry unpushed
  work into subtasks and break when the parent task's workspace is on another commit).

## R7. Subtasks inherit the parent task's project and submodules

- **Decision**: at creation the subtask's project is the parent task's, whatever the request
  says (a different explicit project is refused); at start its `submodules` are the parent task's
  workspace submodules.

## R8. One place that says a task's base: `integration::base_for`

- **Decision**: a new daemon module `crates/delilah-daemon/src/integration.rs` with
  `base_for(app, task) -> Base { Default, Branch(String) }` and `own_branch_behind(...)`.
  `Base::Branch` (subtasks) is the parent task's `Workspace.branch`. Callers:
  `run.rs` (WorkspaceSpec.base), `pr_api/open.rs` (`base_of` → `--base`, and the commits of
  the proposal), `base_update.rs` (`inspect` → behind count and "Update branch").
- **Rationale**: today three functions resolve the base in three ways (`delilah-git/base.rs:92`,
  `base_update.rs:67`, `pr_api/open.rs:85`); putting the subtask rule in each would scatter it.
  The default-branch paths are left as they are.

## R9. "Update branch" for a parent task (FR-016, FR-016a)

- **Decision**: `base_update::RepoState` gains the ordered list of refs to merge. For a parent
  task: `origin/<own branch>` (the merged subtasks) then the default branch; for a subtask:
  `origin/<parent task's branch>`; otherwise the default branch. `merge_all` merges them in
  order (always `git merge`, never rebase) and, on conflict in any step, resets every repo to
  its starting commit, as it already undoes a conflicting merge today. The base-status response
  gains `subtask_commits` (commits on `origin/<own branch>` not in `HEAD`) for the parent task's
  banner.

## R10. Merge method per kind (FR-017, FR-018)

- **Decision**: `ProjectSettings.parent_task_merge: MergePreference` (serde default `merge`). A
  daemon function `preferred_method(kind, settings, allowed) -> Option<MergeMethod>`: subtask →
  `Squash`, parent task → the setting, normal → none; if the preferred one is not in
  `allowed`, it falls back to `allowed[0]` (today's choice, `pr_api/merge.rs:110`). The PR DTO
  (`pr_api/dto.rs`) gains `method: Option<MergeMethod>`, the method "Merge" will use; the UI shows
  it instead of `merge_methods[0]` (`PrView.tsx:329`). `merge()` with no method in the request
  uses the same function.
- `MergeMethod` stays in the daemon (`pr_types.rs`); the setting is a core enum
  `MergePreference { Merge, Squash, Rebase }` (same serialisation), converted in the daemon,
  so the core stays free of daemon types.

## R11. Parent task PR: draft, empty commit, ready (FR-014, FR-020, FR-023) — CHK006

- **Decision**:
  - `open_for` forces `draft = true` for a parent task unless every counted subtask is merged
    and there is at least one.
  - `OpenRequest` gains `empty_commit: bool`. When the proposal fails with `"no-commits"` on a
    parent task, the UI offers "Add an empty commit and open"; the daemon then runs
    `git commit --allow-empty -m "chore: start <KEY> <title>"` in that repo's workspace (new
    `delilah_git::commit_empty`), pushes (existing `push_if_needed`) and opens. Refused on
    anything but a parent task.
  - New `gh.pr_ready(url)` (`gh pr ready <url>`) and `POST /tasks/{key}/prs/{id}/ready`, used by
    the suggestion's button. Never called on its own.
- **Rationale**: the commit message follows the repo's Conventional Commits; the empty commit
  is lost in the merge commit of the parent task's PR, so it never reaches `main` as noise of its
  own beyond that.

## R12. Merging a parent task with live subtasks (FR-019)

- **Decision**: `MergeRequest` gains `confirm_live_subtasks: bool`. For a parent task with live
  subtasks and the flag false, `merge` refuses with reason `"live-subtasks"` and the count; the UI
  turns it into a confirmation dialog and retries with the flag.

## R13. Archive, delete and archive-on-merge (FR-026, FR-027)

- **Decision**: one check `integration::live_subtasks(app, key) -> Vec<TaskKey>` called by
  `actions::archive`, `actions::delete` (refuse 409 `"live-subtasks"` naming them; `--force` does
  not override it) and `pr_loop::merge::archive_merged` (after `MarkDone`, skip the release and
  push a notice `KIND_PARENT_TASK_LIVE = "parent-task-live"`). A subtask's own archive and
  delete are unchanged.

## R14. Creating, converting, attaching — CHK014, CHK015, CHK021

- **Create (API)**: `NewTask` gains `kind: Option<TaskKind>` and `parent_task: Option<String>`;
  `parent_task` set implies `Subtask`. Validation in `build_task_with`: the parent task exists,
  is a `ParentTask`, is not archived; a `ParentTask` cannot have `parent_task`. Keys of subtasks
  without a tracker come from `next_free_key()` (`run.rs:1879`), as free tasks.
- **Convert / attach (API)**: `POST /tasks/{key}/make-parent-task` and
  `POST /tasks/{key}/attach` `{ "parent_task": "KEY" }`, each refusing with a reason per
  FR-008. Separate endpoints rather than PATCH, because `BacklogChange` is for backlog edits
  and these have their own preconditions.
- **CLI**: `delilah new --parent-task` creates a parent task; `delilah new --parent <KEY>`
  creates a subtask; `delilah make-parent-task <KEY>`; `delilah attach <KEY> --parent <KEY>`.
  `delilah ls` prints subtasks indented under their parent task and the parent task's
  "N/M"; `delilah show` adds kind, parent task, progress and the subtask list.
- **Starting a subtask** whose parent task is not started (no workspace) is refused in
  `run.rs` with reason `"parent-task-not-started"`.
- **Concurrent merges (CHK021)**: progress is derived on read (R3), never a stored counter,
  so two subtasks merged in the same PR-loop tick cannot race; each sibling's "behind" is
  computed by base-status on read. The PR loop emits `Topic::Tasks` on a subtask merge so the
  parent task's view refreshes (SC-006: within one PR poll, 30 s by default).
- **Branch from the analysis** (spec 005): `branching::starts_branchless` and the adoption of a
  spec name skip parent tasks and subtasks (R6 needs a stable integration branch).

## API view of progress

- **Decision**: `GET /tasks` and `GET /tasks/{key}` return a `TaskView` (`#[serde(flatten)]
  Task` + `subtask_progress: Option<Progress>` for parent tasks); `GET /tasks/{key}/subtasks`
  returns the children with status, archived flag and PR state. Additive: older clients ignore
  the new fields.
- **Alternative**: put progress on `Task` (core type would carry derived, I/O-backed data).
