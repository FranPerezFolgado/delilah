# Research: Task branches start from a fresh default branch

## R1. Which ref is "the remote's default branch"

- **Decision**: per repo, after the fetch: `refs/remotes/origin/HEAD` (what `clone`
  recorded, the remote's declared default); if missing and the fetch succeeded, run
  `git remote set-head origin --auto` once (asks the remote, writes only that
  symbolic ref) and read it again; then `origin/main`, `origin/master`. If none
  exists (or the fetch failed and none was ever fetched): local `main`, `master`;
  then `HEAD`.
- **Rationale**: matches `BASE_REFS` in `changes.rs` (same order), so the base the
  task starts from is the base its "changes" view diffs against. `origin/HEAD` is
  present in every clone; `set-head --auto` covers repos created with `git init` +
  `remote add`.
- **Alternatives**: `RepoInfo.default_branch` from `gh` (needs network + GitHub,
  not every project is on GitHub, slower); `git ls-remote --symref` on every start
  (an extra round trip each time); per-project setting (out of scope, spec Assumptions).

## R2. Fetch: what, how bounded, never prompting

- **Decision**: `git fetch --quiet --no-tags origin` (all branches of `origin`, so
  `origin/<default>` is fresh) through a new `git_with_timeout` in `command.rs`:
  spawns the same `command()`, with env `GIT_TERMINAL_PROMPT=0` and
  `GIT_SSH_COMMAND` left alone, stdin null, polls `try_wait` every 50 ms and kills
  the child when the deadline passes (`GitError::Timeout`). A shared `Deadline`
  (an `Instant`) is passed down, so all repos of a start share the 30 s.
- **Rationale**: std only (no new dependency, delilah-git is sync and runs in
  `spawn_blocking`); the daemon runs from launchd with no tty, so with
  `GIT_TERMINAL_PROMPT=0` and null stdin nothing can prompt, and the deadline bounds
  anything that still hangs (e.g. an SSH host that never answers). `--no-tags`:
  tags are not needed for the base and can be many.
- **Alternatives**: `wait-timeout` crate (a dependency for 20 lines); tokio
  `process` with `timeout` (would split git calls between two styles); forcing
  `ssh -o BatchMode=yes` via `GIT_SSH_COMMAND` (would override the user's
  `core.sshCommand`, e.g. 1Password's agent setup).
- **Test of the bound**: a repo whose `origin` is `ssh://x/y` with
  `-c core.sshCommand="sleep 30"` hangs; with a 1 s deadline the fetch must come
  back `Timeout` in < 3 s.

## R3. Branch from a remote-tracking ref without tracking it

- **Decision**: always pass the resolved **commit sha** as the start point
  (`worktree add -b <branch> <path> <sha>`, `checkout -q -b <branch> <sha>`,
  `worktree add --detach <path> <sha>`), never `origin/main` by name.
- **Rationale**: starting from a remote-tracking branch by name sets it as upstream
  (`branch.autoSetupMerge`), so the task branch would track `origin/main`: `git
  status` reports ahead/behind main and a bare `git push` by the agent is refused
  or, with `push.default=upstream`, pushes to main. A sha sets no upstream; Delilah's
  `push -u origin <branch>` sets the right one later. The ref name is still logged
  and shown in the warning.
- **Alternatives**: `--no-track` (equivalent, but a sha also pins exactly what was
  logged against a concurrent fetch moving the ref between resolve and use).

## R4. Parent projects

- **Worktree**: fetch the parent, `worktree add --detach <path> <parent base sha>`,
  then `submodule update --init -- <chosen>` as today (checks out the commit the
  parent records), then per chosen submodule: fetch, resolve, `checkout -q -b
  <branch> <sha>`. A submodule fetch failure falls back per submodule (FR-004a).
- **Checkout**: the parent folder gets no branch and is not fetched (FR-001); each
  chosen submodule is fetched and branched from its own base, or switched to its
  existing branch untouched (FR-005).
- **Known limit, unchanged**: in a parent *worktree*, `submodule update --init`
  clones the submodules from their URLs, so it already needs the network; offline
  it fails the start today and still will (FR-011). The fallback (FR-007) applies to
  the fetches this feature adds.

## R5. Concurrent starts and fetches of the same repo

- **Decision**: a process-wide lock per repo path in `delilah-git`
  (`static FETCHING: Mutex<HashMap<PathBuf, Arc<Mutex<()>>>>`), taken with
  `try_lock` polling against the same deadline; a start that waited for another
  start's fetch does not fetch again if that one finished in the last 5 s.
  Archive-on-merge's `fetch` takes the same lock.
- **Rationale**: two `git fetch` in one repo race on ref locks ("cannot lock ref")
  and one fails (FR-012). The lock lives where the git calls are, so every caller
  gets it.

## R6. Telling the user (FR-007)

- **Decision**: reuse `pr_notices` (task, repo, kind, cause, text; resolved, not
  deleted; resolved on archive and on delete by cascade) with a new kind
  `base-stale`, cause `start`. The Bandeja lists it as a new item kind
  `base-stale` (not blocking, no PR needed), dismissable through the existing
  `POST /api/v1/pr-notices/{id}/dismiss`. The task screen shows the task's open
  `base-stale` items as a banner with the same dismiss.
- **Rationale**: an existing table with the exact semantics (one per task/repo/kind,
  dismissable, cleaned up with the task), no migration; the dismiss route already
  exists. The Bandeja is where Delilah tells the user things.
- **Alternatives**: a new `task_notices` table (a migration for the same shape);
  a task history event (not dismissable, not in the Bandeja); a desktop
  notification (too loud for a non-blocking fact; `alerts.ts` still gets a label
  since `SAYS` is a `Record<InboxKind,…>`, but `base-stale` is not in `BLOCKING`).
- **Text**: `app.lang().pick(es, en)`, e.g. "No se pudo actualizar `svc-orders`
  (timeout); la rama empieza en `origin/main` @ `ab12cd3` y puede ir atrasada."

## R7. Logging (FR-010)

`tracing::info!(task, repo, base = %refname, sha, fetched, "task branch base")`
per repo in `run.rs`, from the `bases` the git calls return.

## R8. Version

New behaviour, backwards compatible, an additive API field and inbox kind →
**minor**: 0.4.0 → 0.5.0.
