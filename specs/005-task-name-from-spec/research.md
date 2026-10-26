# Research: Task name and branch from the first analysis

Decisions for [spec.md](spec.md). `CHKnnn` points to the item of
[checklists/workspace.md](checklists/workspace.md) each one answers.

## R1. A workspace without a branch

- **Decision**: `WorkspaceSpec.branch` becomes `Option<String>`; `Workspace.branch`
  stays a `String`, with `""` meaning "no branch yet" (`Workspace::has_branch()`).
  Stored as `''` in `workspaces.branch` (the column is `NOT NULL`; no migration).
  - **Repo, worktree mode**: `git worktree add --detach <path> HEAD`.
  - **Parent, worktree mode**: as today (parent detached), but the chosen submodules
    are initialised and left detached at their recorded commit, without `checkout -b`.
  - **Checkout mode**: `use_checkout` checks the folders are clean and records where
    they are (`previous`), but switches nothing. For a parent it also records the
    parent itself (`"."`), because Spec Kit's hook runs in the parent's root and may
    switch it; archiving then puts it back.
- **Base** (CHK001, CHK018): the same commit today's branch starts from: the project
  folder's current `HEAD`, whatever branch the checkout is on. Nothing is switched
  to the default branch; that would change today's behaviour for no gain.
- **Worktree folder**: `workspaces/<KEY>` as today; never moved (decided with the
  user).
- **Rationale**: an empty string keeps every existing `Workspace` reader compiling and
  tasks started before the feature untouched (FR-014: their branch is non-empty).
- **Alternatives**: a provisional branch renamed later (rejected by the user: Spec Kit
  creates it); `branch` nullable via a table rebuild (a migration for no benefit).

## R2. Who creates the branch, and how Delilah learns its name

One function, `branching::ensure(app, key, why)`, settles the branch of a branchless
free task. It is idempotent and serialised per task (a mutex in `AppState`, like
`HandoffCoordinator`). In order (CHK003):

1. **Spec Kit created it**: the workspace's main repo (the repo, or the parent's
   root) is on a branch it wasn't on at start (worktree: any branch, since it started
   detached; checkout: different from `previous`). Its name is adopted as is: the
   hook numbers it from `specs/` and every branch of the repo, so it may differ from
   the spec folder's number; the branch wins (FR-006).
2. **Delilah creates it from the spec folder** (FR-002a): there is a new spec folder
   (`specs/NNN-name/` holding `spec.md`, not present at start) and no branch: `git
   checkout -b NNN-name` in the main repo; uncommitted files are carried over.
3. **Title fallback** (FR-007): only when called by the guard (R3) and there is no
   spec folder: `slug(title)`.
4. Then, for a parent, `checkout -b <name>` in each chosen submodule (CHK006): parent
   first, then the submodules in the order they were chosen. "Applied" means every
   repo of the task is on that branch.
5. `save_workspace` with the branch; `Topic::Tasks`; clear any `branch_error`.

**Which spec folder** (CHK004, CHK005): a directory under `specs/` with a `spec.md`, in
the main repo or (parent) in a chosen submodule, that did not exist when the task
started. "At start" is recorded as the list of spec folder names
(`workspaces.spec_dirs_at_start`, R6), so a folder of an old feature never counts.

**Slug** (CHK007): lower-case ASCII (accents folded), runs of other characters → `-`,
trimmed, at most 4 words and 40 characters, no number prefix; empty → `task`.

**Name taken** (CHK012, CHK014, CHK019): for names Delilah picks (steps 2–3), a
local or remote-tracking branch with that name → `-2`, `-3`… until free. A branch the
hook made is never renamed; two parallel tasks get different numbers because the hook
scans every branch of the repo, and the hook itself refuses an existing name.

**Detection points** (FR-002, FR-016):
- the PostToolUse hook of any tool of a branchless task's session (the hook path is
  synchronous: Claude waits for it, so the branch exists before the next tool runs);
- the primary's turn end;
- the scheduler tick (30 s) and daemon start, for running branchless tasks: an
  analysis written while the daemon was away is picked up (CHK026);
- `POST /api/v1/tasks/{key}/branch` (retry).

Every step reads the repo again, so a restart in between just redoes what is
missing: branch made but not recorded → adopted (1); recorded but a submodule not
switched → step 4 runs again for the parent's branch; title not updated → R4 runs
on the same triggers.

**Deleted analysis** (CHK027): if the spec folder is gone before Delilah picks it up,
nothing happens; the task stays branchless until the next analysis or the guard.

## R3. The guard: nothing lands without a branch

- **Decision**: sessions launched for a branchless task get one more PreToolUse hook,
  matcher `Edit|Write|MultiEdit|NotebookEdit|Bash`, posting to
  `/hooks/claude/guard?task=…&session=…` with `curl -m 15`. The daemon answers at once
  (empty body = allow) unless the tool is:
  - an edit of a file outside `specs/` and `.specify/` (CHK011: the agent's edit; the
    user's own edits in an editor are outside Delilah, and the commit guard covers
    them), or
  - a Bash command containing `git commit`, `git push`, `gh pr create`, `git merge` or
    `git rebase`,

  in which case it runs `branching::ensure(…, Guard)` first: steps 1 → 2 → 3.
- Once the task has a branch the endpoint returns at once, so later tool calls cost a
  local HTTP round trip. Sessions launched after the branch exists don't get the hook.
- If `ensure` fails, the guard denies the tool with the reason (`permissionDecision:
  "deny"`), so the agent stops instead of committing onto the base branch (SC-003);
  the Bandeja shows the failure (R5).
- **Commits on a detached worktree** (CHK028): the guard runs before any commit, so
  there are none; if there are (e.g. the user committed by hand in the worktree),
  `checkout -b` keeps them on the new branch.
- **Rationale**: the hook path is the only place Delilah sees a tool *before* it runs;
  `asks.rs` already uses a blocking PreToolUse hook.
- **Alternatives**: PostToolUse on the first edit only (the commit could slip in the
  same turn); a git `pre-commit` hook in the repo (it would live in the worktree or
  the user's checkout: ruled out by the ground rules).

## R4. The title

- **Source**: the first line of the new spec folder's `spec.md` matching
  `# Feature Specification: <name>`; `<name>` trimmed. A missing heading, an empty
  name or the template's placeholder (`[FEATURE NAME]`, any `[…]`-only name) means
  "not yet" (CHK010).
- **When**: on the same triggers as R2, independently of the branch (FR-011), for free
  tasks whose `title_source` is `initial`.
- **Once** (FR-005, CHK013): `tasks.title_source`: `initial` → `spec` (automatic) or
  `manual` (the user changed it, before or after). Only `initial` is ever renamed
  automatically. `tasks.previous_title` keeps the title replaced by the automatic
  rename.
- **Restore / edit** (FR-004, CHK008, CHK009): `PATCH /api/v1/tasks/{key}` accepts
  `title` for any task that isn't archived (not only backlog); a title change through
  the API sets `title_source = manual` and clears `previous_title`. Restoring is a
  PATCH with the previous title. The task screen header shows "Antes: <previous>" with
  "Restaurar" while `previous_title` is set; the title in the header is editable in
  place (backlog view keeps its form).
- **Task from an idea** (CHK023): its title is Claude's proposal, but it is still
  `initial`, so the analysis renames it: the analysis knows more than the idea did.

## R5. Failures

- `tasks.branch_error TEXT` (NULL when fine): the reason, with the repo label when a
  submodule failed (CHK017: a partly applied branch stays recorded on the main repo,
  the error names the submodules left behind; the retry finishes them).
- Bandeja item kind `branch` ("No se pudo crear la rama: <reason>") with "Reintentar"
  → `POST /api/v1/tasks/{key}/branch`, which runs `ensure` once more (CHK024, CHK025).
  The tick does not retry a failed step on its own: the failure stays until the user
  retries or the agent's next tool call (guard) retries it.

## R6. Data

Migration `0015_task_branch_from_spec.sql`:
- `tasks.title_source TEXT NOT NULL DEFAULT 'manual'` — existing tasks are `manual`
  (never renamed, FR-014); new free tasks are created `initial`; Jira tasks `manual`.
- `tasks.previous_title TEXT`, `tasks.branch_error TEXT`.
- `workspaces.spec_dirs_at_start TEXT` (JSON array; NULL for workspaces that started
  with a branch).

## R7. Compatibility (FR-015)

- **PR** (`pr_api/open.rs`, CHK029): a branchless task is refused with 422 "la tarea
  aún no tiene rama"; the PR loop (`pr_loop::detect`) skips it.
- **Archive / delete** (CHK029, CHK030): checkout mode goes back to `previous` (which
  now may include the parent); nothing to delete. Worktree mode: `archive_workspace`
  checks uncommitted changes as today; with no branch there are no unpushed commits to
  check (R3), and `worktree remove` works on a detached worktree.
- **Archive on merge**: needs a PR, hence a branch: unchanged.
- **Spec progress** (`specs.rs`, CHK031): `find_spec_dir` skips the branch match and
  `changed_paths` when there is no branch; the branch is created as soon as the spec
  folder appears, so the gap is one tool call.
- **Branch in the UI** (CHK032): not shown today; `Task` gains no branch field.
- **Saving-mode handoff** (CHK021): the next session launches in the same workspace;
  by `tasks.md` the branch exists; if not, it gets the guard like any session launched
  for a branchless task.
- **Project session turned into a task** (CHK022): `start_run` relaunches it with new
  settings; it gets the guard and the branchless workspace like any free task.
- **Backlog tasks** (CHK015, CHK020): free tasks still in the backlog when this ships
  start branchless (their `title_source` is `manual` from the migration, so their
  title is kept: the user wrote it before the feature existed). New backlog tasks are
  `initial`.
- **Jira tasks** (FR-013): `feature/<KEY>` at start, `title_source = manual`. A task is
  free when its key has the `LIBRE-` prefix (the existing convention of
  `next_free_key`).

## R8. Spec Kit and the `/feature` skill (CHK036–CHK038)

- Spec Kit 0.8.12 (`.specify/init-options.json`), git extension: `before_specify` runs
  `create-new-feature.sh`, which does `git checkout -q -b NNN-name` in the cwd; this
  works in a detached worktree (it is a plain `checkout -b`). Quickstart checks it on
  the fixtures.
- Projects without the hook are covered by R2 step 2.
- The first prompt of a branchless task adds one sentence: "La tarea aún no tiene
  rama: deja que Spec Kit la cree al especificar" / "The task has no branch yet: let
  Spec Kit create it when specifying", so an agent following `/feature` doesn't skip
  the hook as it would on a Delilah-owned branch.

## R9. Tests

- `delilah-git`: detached worktree / no-switch checkout; `checkout -b` carrying
  uncommitted files; submodules applied after the fact (fixtures from
  `scripts/make-fixture-parent.sh`-style temp repos).
- `delilah-daemon`: `branching` (adopt hook branch, create from spec folder, title
  fallback, collisions, partial parent, restart idempotence), guard endpoint (allow /
  ensure / deny), title rename once / manual wins / restore, PR refusal when
  branchless, migration defaults.
- `app`: task screen header (previous title, restore, inline edit), Bandeja `branch`
  item.
