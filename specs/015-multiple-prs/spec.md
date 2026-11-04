# Feature Specification: Track every PR a task opens

**Feature Branch**: `015-multiple-prs`
**Created**: 2026-11-03
**Status**: Draft
**Input**: User description: "When a task create a pr in more than one repo, if the project is a parent, the task only tracks one in the pr tab. It should track any pr created by the task and close only when every pr is merged." (LIBRE-38)

Builds on the PR loop (`specs/003-pr-loop`, User Story 5 "changesets"): a parent task
already tracks one PR per selected submodule, found by the task's branch name, and is
archived when all of them are merged. In practice a task opens PRs that this misses:

- in the parent repository itself (now that the parent is on the task's branch too);
- in a submodule that was not selected when the task started, or in a repository
  outside the workspace;
- from a branch whose name is not the task's branch;
- a second PR in a repository that already has one (it replaces the first).

Each missed PR is invisible in the PR tab and does not hold back the archive, so
the task can close while that PR is still open.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Every PR the agent opens shows up (Priority: P1)

The agent of a task on a parent project opens a PR in a submodule and another in the
parent repository (or in a submodule that was not selected, or from a branch with
another name). Within moments both PRs appear in the task's PR tab, each with its
repository, state, checks and review state, and the PR badge counts both.

**Why this priority**: it is the bug as reported; nothing else in the feature
matters if a PR is not tracked in the first place.

**Independent Test**: with the fixture parent project (`scripts/make-fixture-parent.sh`)
and the fake `gh`, have the task's agent create one PR in a submodule and one in the
parent repository; the PR tab lists both and the badge says 2.

**Acceptance Scenarios**:

1. **Given** a parent task whose agent opens PRs in two repositories, **When** the
   PRs are created, **Then** the PR tab lists both, each with its repository, within
   the time of one check.
2. **Given** a parent task on its branch in the parent repository too, **When** a PR
   is opened from that branch in the parent repository, **Then** it is tracked like a
   submodule's PR.
3. **Given** a task whose agent creates a PR from a branch whose name differs from the
   task's branch, or in a repository outside the workspace, **When** the agent's PR
   command prints the PR's address, **Then** that PR is tracked.
4. **Given** a task with a PR in a repository, **When** a second PR is opened in the
   same repository from another branch, **Then** both are tracked; neither replaces
   the other.
5. **Given** a plain (non-parent) repo project whose agent opens two PRs, **When**
   they are created, **Then** both are tracked too.

---

### User Story 2 - The task closes only when every PR is merged (Priority: P1)

A task with several PRs stays open (in Review) while any of them is open, and is
archived the way it is today only once every tracked PR is merged.

**Why this priority**: closing early is the harmful half of the bug: the workspace is
released and the task disappears from the board while work is still under review.

**Independent Test**: a task with two tracked PRs; merge one and run a check: the task
stays in Review; merge the other: the task is archived.

**Acceptance Scenarios**:

1. **Given** a task with two tracked PRs, **When** one is merged, **Then** the task
   is not archived and the PR tab shows one merged and one open.
2. **Given** a task with two tracked PRs, **When** both are merged, **Then** the task
   is archived once, as today.
3. **Given** a task whose only known PR has just merged, **When** another PR of the
   task exists that has not been linked yet, **Then** the task is not archived: the
   check before archiving looks again in every repository and finds it.
4. **Given** a task whose agent is in the middle of a turn, **When** every known PR
   is merged, **Then** the archive waits until the turn ends (the agent may be about
   to open another PR) and is decided by the check after it.
5. **Given** a task with one merged PR and one closed without merging, **When** the
   check runs, **Then** the task is not archived and the user is told, as today.

---

### User Story 3 - Linking, opening and unlinking any of them by hand (Priority: P2)

From the PR tab the user can still link a PR by its address, open a PR for a
repository of the task that has none, and unlink a PR; these now work for every
repository a task can have a PR in, and act on one PR without touching the others.

**Why this priority**: the manual paths are the fallback when detection misses
something, and today they refuse exactly the PRs this feature is about.

**Independent Test**: on a parent task, link by address a PR of the parent repository
and one of a repository outside the workspace; both are listed. Unlink one; the other
stays.

**Acceptance Scenarios**:

1. **Given** a parent task, **When** I paste the address of a PR of the parent
   repository, of an unselected submodule or of another repository my account can
   read, **Then** it is linked and listed with its repository.
2. **Given** a task with several PRs, **When** I unlink one, **Then** only that one
   goes, and it is not linked again by detection.
3. **Given** a parent task on its branch in the parent repository too, **When** I
   choose "Open PR", **Then** the parent repository is offered alongside the
   submodules that have no open PR.
4. **Given** a PR closed without merging that blocks the archive, **When** I unlink
   it, **Then** the next check archives the task if every remaining PR is merged.

---

### Edge Cases

- The same PR found by more than one route (detection, the agent's command output,
  pasted, opened by Delilah) is tracked once.
- A PR closed without merging and then replaced by a new PR from the same repository
  and branch: the new one is tracked and the closed one is shown as replaced and no
  longer blocks the archive.
- A PR address in the agent's output that `gh` cannot read (no access, other host):
  it is not linked, and the problem is reported the way PR access problems are today.
- PR addresses that appear in the agent's output but were not created by it (it ran
  `gh pr view`, `gh pr list`, or printed someone else's PR): only the output of a PR
  creation command counts.
- A repository outside the workspace has no local branch: archiving deletes nothing
  there, and remote branches are never deleted (FR-037 of the PR loop).
- Tasks that already have PR links when this ships keep them, with no action from
  the user.
- A parent task with subtasks (integration branch, `specs/012-parent-subtasks`): its
  own PRs follow the same rules; how subtask merges roll up is unchanged.

## Requirements *(mandatory)*

### Functional Requirements

**What is tracked**
- **FR-001**: A task MUST track every PR linked to it, with no limit per repository
  and no limit on the number of repositories.
- **FR-002**: Detection by the task's branch MUST look in every repository of the
  workspace: the project's repository for a plain repo; for a parent, each selected
  submodule and the parent repository itself when it is on the task's branch.
- **FR-003**: When the task's agent runs a PR creation command and the command
  prints the address of a PR, Delilah MUST link that PR to the task, whatever its
  repository or head branch, as long as the user's account can read it.
- **FR-004**: Only the output of a PR creation command counts for FR-003; addresses
  printed by other commands or written in text MUST NOT be linked.
- **FR-005**: The same PR MUST never be tracked twice for a task, whichever routes
  found it.
- **FR-006**: A PR the user unlinked MUST NOT be linked again automatically by any
  route (detection or the agent's output); linking it by hand is still allowed.
- **FR-007**: Link by address MUST accept a PR of any repository the user's account
  can read, not only the workspace's repositories.
- **FR-008**: Each tracked PR MUST say which repository it belongs to: the submodule
  path, the parent repository, or the hosting repository's name for a repository
  outside the workspace.

**Replaced PRs**
- **FR-009**: When a PR closed without merging is followed by a new PR from the same
  repository and head branch, the closed one MUST be marked as replaced; a replaced
  PR is shown but does not count for the archive or the badge.

**Closing the task**
- **FR-010**: A task MUST be archived automatically only when it has at least one
  counted PR and every counted PR is merged (FR-017 and FR-039 of the PR loop still
  apply: a PR closed without merging blocks it and notifies).
- **FR-011**: Before archiving, Delilah MUST look once more for PRs in every
  repository of FR-002; if that finds a PR that is not merged, the task MUST NOT be
  archived.
- **FR-012**: Delilah MUST NOT archive a task automatically while its agent is in
  the middle of a turn; the check after the turn ends decides.
- **FR-013**: The archive MUST delete the local task branch of each workspace
  repository whose PR merged from that branch, including the parent repository
  when it had a PR; a parent repository without a PR keeps its branch as today
  (FR-038 of the PR loop). Repositories outside the workspace are not touched.

**Showing them**
- **FR-014**: The PR tab MUST list every tracked PR grouped by repository, each with
  the state, checks, review state and actions it has today.
- **FR-015**: The PR badge on cards and in the sidebar MUST show the worst state among
  the counted PRs and how many there are.
- **FR-016**: "Open PR" MUST offer every workspace repository of FR-002 that has no
  open PR from the task's branch, including the parent repository when it is on the
  task's branch.
- **FR-017**: Inbox items (CI failed, changes requested, new comments) and autofix
  MUST work per PR for every tracked PR, as they do today for the one.
- **FR-018**: All new text MUST exist in Spanish and English.

**Existing data**
- **FR-019**: PR links stored before this change MUST keep working with no action
  from the user, and nothing stored MUST be lost.

### Key Entities

- **Pull request link**: belongs to a task; identified by its PR (host, repository,
  number), not by the repository alone, so a task can have several per repository.
  Has the repository label (FR-008), how it was linked (detected, opened, pasted, or
  created by the agent), whether it has been replaced (FR-009), and the last
  snapshot.
- **Repository label**: where in (or outside) the workspace the PR lives: `.` for a
  plain repo or the parent repository, a submodule path, or the hosting
  repository's name.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: In the fixture parent project, a task whose agent opens PRs in two
  repositories (any mix of submodules, the parent repository and other repositories)
  shows both in the PR tab within one check (30 s) of their creation, in 10 out of 10
  runs.
- **SC-002**: No task with an open or unlinked-but-unmerged PR created by its agent is
  archived automatically, in every scenario of User Story 2.
- **SC-003**: A task with N tracked PRs is archived within one check of the N-th merge,
  and exactly once.
- **SC-004**: Every task with PR links from before the change still shows the same PRs
  with the same states after upgrading.
- **SC-005**: The user never needs "Link by URL" for a PR the task's agent created.

## Assumptions

- Autopilot run: no clarification phase; the choices below are informed guesses.
- "Any PR created by the task" means PRs created by the task's agent (its PR creation
  command output), by Delilah's "Open PR", or found from the task's branch in the
  workspace; PRs other people open from other branches are only tracked if linked by
  hand.
- The PR creation command is the hosting CLI's (`gh pr create`); PRs opened through a
  web browser by the agent are found only by branch detection.
- Linking a PR from outside the workspace by hand is a deliberate user action, so no
  extra confirmation is asked; the PR card shows its repository and it can be unlinked.
- A PR closed without merging still blocks the automatic archive until it is replaced
  (FR-009) or unlinked; the user can always archive by hand.
- Waiting for the agent's turn to end (FR-012) is enough to catch a PR it is about to
  open; no longer grace period is added.
- How subtasks' merges roll up into a parent task (`specs/012-parent-subtasks`) does not
  change.
- Remote branches are still never deleted.
