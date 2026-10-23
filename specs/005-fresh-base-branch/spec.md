# Feature Specification: Task branches start from a fresh default branch

**Feature Branch**: `feature/LIBRE-12`
**Created**: 2026-10-23
**Status**: Draft
**Input**: User description: "Crear siempre los worktrees desde main recién actualizado. Que siempre que se cree un worktree lo haga desde main recién actualizado; me pasa que me toca hacer rebase."

## Why

When Delilah starts a task, it creates the task's branch from whatever the project's
folder has checked out at that moment, usually a local `main` that was last
updated whenever the user last pulled. Every PR merged since then is missing from
the new branch, so the agent works on old code and the user has to rebase before
the PR can merge, sometimes resolving conflicts with work that was already done.
Starting every new task branch from the default branch as it is on the remote,
right after asking the remote for it, removes that rebase.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - A new worktree task starts from the remote's latest default branch (Priority: P1)

The user's local `main` is behind the remote (other PRs were merged since their
last pull). They start a task in a worktree. Delilah first brings the project's
view of the remote up to date, then creates the task's branch from the remote's
default branch. The agent sees every merged change, and the PR opens without a
rebase.

**Why this priority**: It is the reported pain, and worktree tasks are the ones
started while other work is in flight, so their base is the most likely to be stale.

**Independent Test**: In a fixture repo project whose local `main` is behind its
remote by one commit, start a worktree task and check that the task's branch
contains that commit and that its starting point is the remote's default branch.

**Acceptance Scenarios**:

1. **Given** a repo project whose local default branch is behind the remote,
   **When** the user starts a task in a worktree, **Then** the task's branch starts
   at the remote's current default branch, including commits the local branch lacks.
2. **Given** the project's folder is checked out on some other branch (not the
   default), **When** a worktree task starts, **Then** the task's branch still starts
   from the remote's default branch, not from that other branch.
3. **Given** a task branch is created this way, **When** the user looks at the
   project's own folder, **Then** its checked-out branch, its local default branch
   and its files are exactly as they were.

---

### User Story 2 - Checkout-mode tasks start from the fresh default branch too (Priority: P1)

Most tasks run in the project's own folder (checkout mode, the default). When such
a task creates its branch, that branch also starts from the remote's latest default
branch instead of from whatever the folder had checked out.

**Why this priority**: Checkout mode is the default, so without it most tasks
would keep the problem.

**Independent Test**: In a fixture repo project whose local `main` is behind its
remote, start a checkout-mode task and check that the folder is now on the task's
branch and that the branch contains the remote's latest commit.

**Acceptance Scenarios**:

1. **Given** a clean project folder on a stale local `main`, **When** a
   checkout-mode task starts, **Then** the folder switches to a new task branch
   that starts at the remote's default branch.
2. **Given** the task's branch already exists in the folder (e.g. a task started
   before, or retried from the backlog), **When** the task starts, **Then** Delilah
   switches to that existing branch untouched: no reset, rebase or merge.
3. **Given** a checkout-mode task is later archived, **When** it goes back to the
   previous branch, **Then** it goes back to the branch the folder was on before,
   as today.

---

### User Story 3 - Parent projects: each chosen submodule starts from its own fresh default branch (Priority: P2)

For a parent project (a repo of submodules), the task's branches and PRs live in
the chosen submodules. Each chosen submodule's task branch starts from that
submodule's remote default branch, freshly fetched, rather than from the commit
the parent happens to record for it.

**Why this priority**: The same rebase problem, in the project type where the
recorded submodule commits are most often behind.

**Independent Test**: With the parent fixture (`scripts/make-fixture-parent.sh`),
add a commit to one submodule's remote that the parent does not record, start a
worktree task with that submodule chosen, and check that the submodule's task
branch contains the commit.

**Acceptance Scenarios**:

1. **Given** a parent project whose recorded commit for a chosen submodule is
   behind that submodule's remote default branch, **When** a task starts (worktree
   or checkout mode), **Then** the submodule's task branch starts at the remote
   default branch of that submodule.
2. **Given** a worktree task of a parent project, **When** the workspace is
   created, **Then** the parent's worktree is based on the parent's remote default
   branch, and submodules that were not chosen stay uninitialised, as today.
3. **Given** two chosen submodules whose default branches have different names,
   **When** the task starts, **Then** each starts from its own default branch.

---

### User Story 4 - Offline: the task still starts, and the user is told (Priority: P2)

The user starts a task with no network (or the remote is unreachable, or the
credentials fail). Delilah cannot update its view of the remote, so it starts the
task from the best base it already has and tells the user that the branch may be
behind.

**Why this priority**: Starting work must never depend on the network; a stale
base is a nuisance, a task that cannot start is a blocker.

**Independent Test**: Point a fixture project's remote at an unreachable location,
start a task, and check that it starts and that a warning naming the project (and
the base it used) is visible on the task.

**Acceptance Scenarios**:

1. **Given** the remote cannot be reached, **When** a task starts, **Then** the
   task starts from the last known state of the remote's default branch (or, if
   there is none, from the local default branch), and a warning visible on the task
   says which repo could not be updated and which base was used.
2. **Given** the remote does not answer, **When** a task starts, **Then** the start
   is held up by at most 30 seconds in total, however many repos are involved,
   before falling back.
3. **Given** a parent project where one chosen submodule fails to update and the
   others succeed, **When** the task starts, **Then** only that submodule falls
   back, and the warning names only it.

---

### Edge Cases

- **Repo with no remote** (a local-only project): there is nothing to update; the
  branch starts from the local default branch if there is one, else from the
  current commit, as today. No warning, since nothing failed.
- **The remote does not say which branch is its default**: use the first of
  `main`, then `master` that exists on the remote; if neither does, fall back as
  when offline, with a warning.
- **Task branch already exists**: in checkout mode it is never moved or rewritten
  (see US2-2), judged per repo, submodules included; in worktree mode the start
  fails as it does today.
- **Project folder has uncommitted changes**: worktree mode does not touch the
  folder, so it is unaffected; checkout mode keeps refusing to start, as today.
- **Updating the remote view needs credentials that would prompt**: never wait for
  input; treat it as a failed update (warn and fall back).
- **The update succeeds but the branch cannot be created** (e.g. invalid name,
  workspace folder exists): the task fails to start exactly as today, and a
  backlog task goes back to the backlog.
- **Local default branch is ahead of the remote** (unpushed commits on local
  `main`): the task still starts from the remote's default branch; the local
  commits are not part of the task.
- **Two tasks of the same project start at the same time**: both get a correct,
  fresh base; neither start fails because of the other's update.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: Before creating a new task branch, Delilah MUST update its view of
  the remote (`origin`) of every repo whose state the task starts from: the repo,
  for a repo project; the parent and each chosen submodule, for a parent worktree
  task; each chosen submodule (not the parent folder, which gets no branch), for a
  parent checkout task.
- **FR-002**: A new task branch MUST start from the remote's default branch as just
  updated. The default branch is the one the remote declares; if it declares none,
  `main`, then `master`, whichever exists on the remote.
- **FR-003**: This MUST apply to every way a task gets its branch: worktree mode
  and checkout mode, repo and parent projects, tasks started directly and tasks
  started from the backlog.
- **FR-004**: For a parent worktree task, the parent's worktree MUST be based on
  the parent's remote default branch, and each chosen submodule's branch on that
  submodule's own remote default branch.
- **FR-004a**: A chosen submodule with no remote, or whose update fails, follows
  FR-007 and FR-009 on its own, without affecting the other submodules.
- **FR-005**: In checkout mode, a task branch that already exists in a repo MUST
  be used as it is in that repo; Delilah MUST NOT reset, rebase, merge or otherwise
  move it (repos are judged one by one: a submodule that already has the branch
  keeps it, one that does not gets a fresh one). In worktree mode, an existing
  branch keeps failing the start as it does today (FR-011).
- **FR-006**: Delilah MUST NOT move or create any local branch other than the task's
  own (in particular, the local default branch stays where it is). In worktree mode
  the project folder's checked-out branch and files MUST NOT change; in checkout
  mode the only change is switching to the task's branch, as today.
- **FR-007**: If updating a repo's view of the remote fails or times out, the task
  MUST still start, using the last known remote default branch, else the local
  default branch, else the current commit, and Delilah MUST show a warning naming
  the repo, the reason, and the base used. The warning is an item of the task in
  the Bandeja, also shown on the task's screen, and stays until the user dismisses
  it; it never blocks or pauses the task.
- **FR-008**: Updating MUST NOT wait for interactive input (passwords, host-key
  prompts) and MUST give up after at most 30 seconds for the whole start (all repos
  together; independent repos may be updated at the same time).
- **FR-009**: A repo with no remote MUST start the branch as today, without a warning.
- **FR-010**: The base a task branch started from (ref name and commit) MUST be
  recorded in the daemon's log for each repo, so a wrong base can be diagnosed.
- **FR-011**: Failures unrelated to the update (invalid branch name, existing
  workspace folder, dirty checkout) MUST behave exactly as today.
- **FR-012**: Two tasks of the same project started at the same time MUST both get
  a fresh base; an update running for one MUST NOT make the other's start fail.

### Key Entities

- **Base of a task branch**: per repo where the branch is created, the remote
  default branch it started from (or the fallback used), its commit, and whether
  the update succeeded.
- **Start warning**: a message attached to the task when one or more repos could
  not be updated; says which repos, why, and the base used.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: With the remote reachable, 100 % of new task branches contain every
  commit on the remote's default branch at the moment the task was started.
- **SC-002**: A task started right after a PR is merged on the remote opens its
  own PR with no rebase needed, unless the two touch the same lines.
- **SC-003**: With the remote unreachable, starting a task takes at most 30 seconds
  longer than today in total, whatever the number of repos, and the task always
  starts.
- **SC-004**: With the remote reachable, starting a task takes at most the time of
  one update of the remote per repo involved (typically a few seconds) longer
  than today.
- **SC-005**: In 100 % of offline starts, the user sees a warning on the task
  explaining that the branch may be behind.

## Assumptions

- The remote is called `origin`, as everywhere else in Delilah (push, PR loop,
  archive-on-merge).
- The base is the remote's default branch, detected per repo; per-project
  configuration of the base branch is out of scope (it can be added later if a
  project needs a non-default base).
- The local default branch is not fast-forwarded: the task only needs the remote's
  state, and moving the user's own branches is outside a task's business. The
  user's own `git pull` stays theirs.
- The branch Spec Kit's `/feature` would create is not affected: in Delilah tasks
  the branch already exists when the agent starts, and Spec Kit keeps it.
- Continuing with a warning (rather than aborting) when the update fails is the
  behaviour the user asked for in the idea's notes.
- A slower start (one network round trip per repo) is acceptable in exchange for an
  up-to-date base.
