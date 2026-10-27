# Feature Specification: Parent tasks and subtasks (integration branch)

**Feature Branch**: `012-parent-subtasks`

**Created**: 2026-10-27

**Status**: Draft

**Input**: User description: "Parent tasks and subtasks — core only (generic, no Jira; the Jira adapter is a later, separate spec). Integration-branch pattern, not stacked PRs: subtasks never chain on each other, all hang from the parent. Three task kinds with fixed behaviour: normal, parent (container: branch + draft PR, no agent at start, sessions on demand), subtask (worktree from the parent's branch, PR against it, squash). Two levels only. Delilah never merges on its own. Progress on the parent. New subtask from the UI, CLI (`delilah new --parent`) and API. A team's workflow is configuration, not code."

## Context

Today one task is one workspace, one branch and one pull request against the
repository's default branch. A large feature is better worked as a **parent task**
with **subtasks**: each subtask opens its PR against the parent task's branch and, once
every subtask is in, the parent task opens one PR to the default branch. This is the
**integration branch** pattern. It is not stacked PRs: subtasks never build on each
other, they all hang from the parent task.

### Terms

- **Parent task**: a task that holds subtasks. Always written in full ("parent task"),
  in the docs and in the UI, never "parent" alone.
- **Parent repository**: the existing, unrelated notion of a repository with submodules.
  This feature does not change it; a parent task can live in a parent repository.
- **Subtask**: a task that belongs to exactly one parent task.
- **Normal task**: every task as it exists today.
- **Integration branch**: the parent task's branch, into which subtasks are merged.
- **Live subtask**: a subtask that is neither archived nor deleted (backlog, running,
  waiting, done but not yet archived, including one merged whose archive was held back
  by leftover changes).
- **Dropped subtask**: a subtask archived or deleted without its PR merged. A subtask
  whose PR was closed without merging but that is still live is not dropped.

### Behaviour per kind

| | Normal task | Parent task | Subtask |
|---|---|---|---|
| Branch starts from | default branch | default branch | the parent task's branch |
| PR against | default branch | default branch, as a draft until done | the parent task's branch |
| "Update branch" merges | default branch | default branch (after its own remote branch) | the parent task's branch |
| Merge method | what the repository allows (as today) | merge commit by default (project setting) | squash |
| Agent when started | yes | no; sessions on demand | yes |
| Archived | when merged | when merged; blocked while it has live subtasks | when merged into the parent task |

## Clarifications

### Session 2026-10-27

- Q: Where does a parent task's workspace live? → A: Always an isolated worktree, like subtasks, so the user's checkout stays free and the finished integration branch sits in its own workspace.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Split a feature into a parent task and subtasks (Priority: P1)

A developer creates a parent task for a large feature and adds subtasks to it, from
the parent task's screen, the CLI or the API. Starting the parent task builds its
workspace and integration branch and publishes the branch, without starting any agent.
Starting a subtask builds its own isolated workspace from the integration branch and
starts the agent as for any task. Several subtasks run at the same time.

**Why this priority**: It is the core of the feature. Without a parent task and
subtasks that branch from it nothing else applies.

**Independent Test**: Create a parent task in a fixture project, add two subtasks,
start the parent task and both subtasks. Check that no agent runs on the parent task,
that each subtask has its own workspace whose branch starts from the integration
branch, and that both run in parallel.

**Acceptance Scenarios**:

1. **Given** a project, **When** the user creates a parent task and starts it, **Then**
   it gets an isolated workspace (never the user's checkout) and a branch from the
   freshly fetched default branch, the branch is pushed, and no agent session is started.
2. **Given** a started parent task, **When** the user opens a Claude or terminal session
   on it, **Then** the session opens in the parent task's workspace, as for any task.
3. **Given** a parent task, **When** the user creates a subtask from its screen, with
   `delilah new --parent <KEY>`, or through the API, **Then** the subtask appears in the
   backlog under its parent task, with no workspace and no agent.
4. **Given** a backlog subtask whose parent task is started, **When** the user starts
   it, **Then** it gets its own isolated workspace (never the user's checkout) whose
   branch starts from the integration branch as just fetched, and its agent starts.
5. **Given** two backlog subtasks of a started parent task, **When** both are started,
   **Then** they run at the same time, each in its own workspace.
6. **Given** a subtask whose parent task has not been started, **When** the user starts
   it, **Then** it is refused with "Start the parent task first".
7. **Given** a subtask, **When** the user tries to create a subtask of it, **Then** it is
   refused: subtasks cannot have subtasks.

---

### User Story 2 - Subtask PRs go to the integration branch (Priority: P1)

A finished subtask opens its PR against the integration branch, keeps up with it
through "Update branch", and is merged by squash. When its PR is merged, the subtask
is archived, and every other live subtask is told that the integration branch moved.

**Why this priority**: Without this, subtask PRs would target the default branch and
the pattern breaks.

**Independent Test**: With a started parent task and a subtask with commits, open the
subtask's PR, merge it from Delilah, and check its target, its merge method, the
archiving of the subtask, and the "Update branch" offer on a sibling subtask.

**Acceptance Scenarios**:

1. **Given** a subtask with commits, **When** the user opens its PR (by hand or through
   the project's automation), **Then** the PR targets the integration branch, never the
   default branch.
2. **Given** a subtask whose integration branch received new commits, **When** the user
   looks at the subtask, **Then** it shows that the parent task's branch moved N commits
   ahead, and "Update branch" merges the integration branch (never the default branch)
   into the subtask's branch.
3. **Given** a subtask PR that is open, not a draft and passing, **When** the user
   presses "Merge", **Then** it is merged by squash, and the PR tab says so beforehand.
4. **Given** a repository that does not allow squash, **When** the user merges a subtask
   PR, **Then** Delilah uses a method the repository allows and the PR tab shows which.
5. **Given** a subtask PR merged (from Delilah or on GitHub), **When** Delilah sees it,
   **Then** the subtask is marked done and archived as normal tasks are today.

---

### User Story 3 - Parent task progress and its own PR (Priority: P2)

The parent task shows how many of its subtasks are merged and lists them with their
state. Its PR to the default branch is a draft until the user decides it is ready;
when every subtask is merged, Delilah suggests marking it ready. It is merged with a
merge commit by default so the default branch keeps one commit per subtask.

**Why this priority**: It closes the loop and gives the overview, but subtasks already
deliver value without it.

**Independent Test**: With a parent task with three subtasks, merge two and check the
progress "2/3 subtasks merged"; merge the third and check the ready suggestion; mark
the PR ready and merge it, and check the merge method and the parent task's archiving.

**Acceptance Scenarios**:

1. **Given** a parent task with subtasks, **When** the user looks at it (task screen,
   sidebar, CLI), **Then** it shows "N/M subtasks merged" and lists each subtask with its
   state and PR state.
2. **Given** a parent task, **When** the user opens its PR, **Then** it targets the
   default branch and is a draft; if the integration branch has no commits yet, Delilah
   offers to add an empty commit so the PR can be opened.
3. **Given** a parent task whose subtasks are all merged, **When** the user looks at it,
   **Then** Delilah suggests marking its PR ready for review, which it does only when the
   user accepts.
4. **Given** a parent task PR that is ready and passing, **When** the user presses
   "Merge", **Then** it is merged with the project's parent task merge method (merge
   commit by default), or a method the repository allows if that one is not allowed,
   and the PR tab shows which.
5. **Given** a parent task with live subtasks, **When** the user tries to merge its PR
   from Delilah, **Then** Delilah warns how many subtasks are still live and merges only
   after the user confirms.
6. **Given** integration branch commits that are not in the parent task's workspace
   (merged subtasks), **When** the user presses "Update branch" on the parent task,
   **Then** they are brought in first, then the default branch is merged as today.

---

### User Story 4 - Archive and delete rules keep the hierarchy consistent (Priority: P2)

A parent task cannot disappear while it has live subtasks, whether by archiving on
merge, by hand or by deleting it.

**Why this priority**: Prevents orphaned subtasks whose PRs target a branch that is gone.

**Independent Test**: With a parent task with one live subtask, try to archive and
delete the parent task (and merge its PR on GitHub): all are held back with a reason.
Archive the subtask and retry: it works.

**Acceptance Scenarios**:

1. **Given** a parent task with live subtasks, **When** the user archives or deletes it,
   **Then** it is refused, naming the live subtasks.
2. **Given** a parent task with live subtasks whose PR gets merged, **When** Delilah sees
   the merge, **Then** it marks the parent task done but does not archive it, and the
   Inbox says why.
3. **Given** a parent task whose subtasks are all archived or deleted, **When** its PR is
   merged, **Then** it is archived as normal tasks are today.
4. **Given** a subtask, **When** it is archived or deleted without being merged, **Then**
   the parent task's progress no longer counts it, and it shows as dropped in the list.

---

### User Story 5 - Turn an existing task into a parent task or a subtask (Priority: P3)

A task created as normal turns out to be big: the user turns it into a parent task.
A backlog task that belongs to a parent task is attached to it as a subtask. These are
the same operations a tracker adapter will use later.

**Why this priority**: Convenience; the main flows work by creating parent tasks and
subtasks from scratch.

**Independent Test**: Turn a normal task without an open PR into a parent task and add
a subtask; attach a backlog normal task to a parent task; check the refusals.

**Acceptance Scenarios**:

1. **Given** a normal task with no open PR and no running agent session, **When** the
   user turns it into a parent task, **Then** it keeps its key, branch and workspace and
   becomes a parent task.
2. **Given** a normal task with an open PR, **When** the user tries to turn it into a
   parent task, **Then** it is refused with the reason.
3. **Given** a normal task running in the user's checkout, **When** the user tries to
   turn it into a parent task, **Then** it is refused: parent tasks only live in an
   isolated workspace.
4. **Given** a normal task in the backlog (never started) in the same project as a
   parent task, **When** the user attaches it to that parent task, **Then** it becomes
   its subtask.
5. **Given** a subtask, or a parent task with subtasks, **When** the user tries to turn
   it into another kind, **Then** it is refused.

---

### Edge Cases

- **Empty parent task PR**: GitHub does not open a PR with no commits of difference.
  The parent task's PR can only be opened once the integration branch has commits; if
  it has none, "Open PR" offers to add an empty commit first.
- **Integration branch not on the remote** (deleted by hand, push failed): starting a
  subtask and opening a subtask PR are refused with the reason; the parent task offers to
  push its branch again.
- **Parent task PR merged on GitHub while subtasks are live**: the parent task is not
  archived (User Story 4); open subtask PRs whose target branch disappears are reported
  as the PR tab shows any closed PR today.
- **Parent task in a parent repository**: subtasks work in the same repositories (the
  same submodules) as their parent task; each repository's integration branch is the
  parent task's branch in that repository.
- **Checkout mode**: neither parent tasks nor subtasks ever use the user's checkout,
  whatever the project's or the task's workspace mode says; both always get an isolated
  workspace, so a long-lived feature never holds the user's folder.
- **Integration branch fetch fails when starting a subtask** (offline): the subtask
  starts from the last known state and warns "Branch may be behind", as today.
- **Conflicts on "Update branch"** in a subtask: refused with the reason, branch left as
  it was, as today.
- **Subtask in a different project**: not possible; a subtask always belongs to its
  parent task's project.
- **Free (branchless) start**: parent tasks and subtasks always start with a branch;
  the branchless start of free tasks does not apply to them.
- **Reopening**: a done parent task or subtask reopens as a task does today and keeps
  its kind and its parent task.
- **Phased workflow, saving mode, autopilot, CI autofix** apply to subtasks as to any
  task (autopilot's PR targets the integration branch); none apply to a parent task's
  start, since it starts no agent.

## Requirements *(mandatory)*

### Functional Requirements

**Kinds and hierarchy**

- **FR-001**: Every task MUST have one kind: normal, parent task or subtask. Existing
  tasks MUST become normal tasks with no change in behaviour.
- **FR-002**: A subtask MUST belong to exactly one parent task, in the same project.
- **FR-003**: Only two levels MUST exist: a subtask cannot be a parent task, and a parent
  task cannot be a subtask.
- **FR-004**: The hierarchy MUST be stored on the Delilah task and work without any
  tracker. A subtask MAY carry its own tracker ticket key, as any task can today.

**Creating**

- **FR-005**: Users MUST be able to create a parent task, and a subtask of a given parent
  task, from the UI (a "New subtask" action on the parent task), the CLI
  (`delilah new --parent <KEY>`, and a way to create a parent task) and the API.
- **FR-006**: Creating a subtask MUST be refused when the named parent task does not
  exist, is archived, or is not a parent task.
- **FR-007**: New subtasks MUST wait in the backlog without workspace or agent until
  started, as backlog tasks do today.
- **FR-008**: Users MUST be able to turn a normal task with no open PR, no running
  agent session and not in the user's checkout into a parent task, and attach a never-started normal task of the same
  project to a parent task as a subtask, from the UI, the CLI and the API. No other kind
  change is allowed: a subtask cannot be detached, and a parent task cannot go back to
  normal.

**Starting**

- **FR-009**: Starting a parent task MUST create an isolated workspace (never the user's
  checkout, whatever the workspace mode) and its branch from the
  freshly fetched default branch, push the branch, and start no agent session; the user
  opens Claude or terminal sessions on it on demand.
- **FR-010**: Starting a subtask MUST create an isolated workspace (never the user's
  checkout) whose branch starts from the parent task's branch as just fetched from the
  remote, and start the agent as for a normal task.
- **FR-011**: Starting a subtask MUST be refused while its parent task has not been
  started or its branch is not on the remote.
- **FR-012**: Several subtasks of one parent task MUST be able to run at the same time.

**PRs, updating and merging**

- **FR-013**: A subtask's PR MUST target the parent task's branch, however it is opened
  (by hand, by autopilot, by the CLI or the API). A subtask PR found targeting another
  branch (opened outside Delilah) MUST be shown with a warning naming its target;
  Delilah does not retarget it.
- **FR-014**: A parent task's PR MUST target the default branch and MUST be opened as a
  draft, unless every counted subtask is already merged (then the user chooses, as for
  any task). When the integration branch has no commits, opening it MUST offer to add an
  empty commit first.
- **FR-015**: For a subtask, the "behind" indicator and "Update branch" MUST use the
  parent task's branch instead of the default branch.
- **FR-016**: For a parent task, "Update branch" MUST first bring in the integration
  branch's remote commits (merged subtasks), then merge the default branch as today. As
  today, it MUST refuse with a reason, and leave the branch as it was, when the workspace
  has tracked changes or either step conflicts.
- **FR-016a**: A parent task MUST show, apart from "main moved N commits ahead", how many
  merged subtask commits are not yet in its workspace, so the user knows to update it
  before working on it.
- **FR-017**: "Merge" MUST use squash for a subtask PR, and the project's parent task
  merge method for a parent task PR. If the repository does not allow that method, it
  MUST fall back to the method Delilah picks for that repository today. The PR tab MUST
  show the method before merging.
- **FR-018**: The project MUST have a "parent task merge method" setting, default merge
  commit.
- **FR-019**: Merging a parent task PR from Delilah while it has live subtasks MUST ask
  for confirmation, naming how many are live.
- **FR-020**: Delilah MUST never merge any PR on its own, and MUST never mark a parent
  task PR ready for review without the user's action.

**Progress**

- **FR-021**: A parent task MUST show "N/M subtasks merged", where M counts its subtasks
  except those archived or deleted without being merged, in the task screen, the sidebar
  and the CLI.
- **FR-022**: A parent task MUST list its subtasks with their state and PR state, and
  subtasks archived or deleted unmerged as dropped.
- **FR-023**: When every counted subtask is merged (and there is at least one), the
  parent task MUST suggest marking its PR ready for review, on its screen and as one
  Inbox notice. A parent task with no counted subtasks shows "0 subtasks" and no
  suggestion. Each subtask merge notifies as a task merge does today.
- **FR-024**: A subtask MUST show which parent task it belongs to, with a link to it.
- **FR-025**: The sidebar MUST show subtasks grouped under their parent task.

**Archiving and deleting**

- **FR-026**: Archiving or deleting a parent task MUST be refused while it has live
  subtasks, naming them.
- **FR-027**: When a parent task's PR is merged while it has live subtasks, Delilah MUST
  mark it done but not archive it, and say why in the Inbox; archiving it later follows
  FR-026.
- **FR-028**: A merged subtask MUST be archived as normal tasks are on merge today.

**Contract and docs**

- **FR-029**: The task's kind, its parent task and (for a parent task) its progress
  MUST be available through the API and the CLI (task list and task detail), as
  additive changes that older clients ignore, so the UI and future tracker adapters
  (Jira, GitHub sub-issues, Linear) can build on them without special cases.
- **FR-030**: The user docs MUST describe the feature as the "integration branch"
  pattern and say plainly that stacked PRs are not supported.

### Key Entities

- **Task** (existing): gains a **kind** (normal, parent task, subtask) and, for a
  subtask, a reference to its **parent task**.
- **Parent task progress** (derived): counted subtasks, merged subtasks, dropped
  subtasks.
- **Project settings** (existing): gains the **parent task merge method** (default merge
  commit).

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: A user can go from an empty project to a started parent task with two
  running subtasks in under 2 minutes, without typing any branch name.
- **SC-002**: 100% of subtask PRs opened through Delilah target the integration branch;
  0 target the default branch.
- **SC-003**: After a feature with N subtasks is merged with the default settings, the
  default branch history shows exactly N subtask commits plus one merge commit (when the
  repository allows merge commits).
- **SC-004**: No parent task can be archived or deleted while it has a live subtask:
  0 orphaned subtasks in any scenario of this spec.
- **SC-005**: Every existing task and flow (normal tasks) behaves exactly as before:
  the existing test suite passes unchanged.
- **SC-006**: The parent task's progress is right after every merge, archive or delete
  of a subtask, no later than Delilah notices a merged PR today, with no manual refresh.

## Assumptions

- The parent task's merge method is the only new project setting; subtasks always
  squash (subject to what the repository allows). More settings can come with the Jira
  adapter.
- Subtasks inherit the parent task's project and repositories (submodules); they cannot
  pick others.
- The parent task's branch name follows the project's branch rules like any task; a
  subtask's branch too. Neither starts branchless.
- Starting a parent task does not open its PR on its own; the user opens it (as a draft)
  when they want, once the integration branch has commits or with the offered empty
  commit.
- Keys of subtasks created without a tracker are generated as for free tasks today.
- Out of scope: detaching a subtask, turning a parent task back into a normal task,
  stacked PRs, dependencies between subtasks, slicing a task into
  subtasks from a `/feature` plan, the Jira adapter (next spec), other trackers.
