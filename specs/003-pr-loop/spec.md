# Feature Specification: PR loop

**Feature Branch**: `feature/LIBRE-1`

**Created**: 2026-10-20

**Status**: Draft

**Input**: User description: "PR loop: close the loop from task to merged pull request. Delilah opens a PR for a task's branch (or detects/links the PR the agent opened), shows the PR state, CI checks and review state on the task screen, notifies on CI failure and on new review comments, and when the PR is merged archives the task. The PR description links the Jira ticket. For parent workspaces with submodules: one PR per touched submodule, linked to each other; archive only when all are merged."

## Context

Delilah's goal is that a task moves on its own from a ticket to pull requests.
Today it stops short of that: the agent can push and run `gh pr create`, and
Delilah notices this only to move the task to Revisión. From then on Fran
follows the PR in the browser: whether CI passed, whether someone reviewed it,
whether it was merged. When it is merged, he has to come back to Delilah, mark
the task as done and archive it by hand, or the workspace and the checkout stay
held by a finished task.

This feature makes the PR part of the task: Delilah knows which PR belongs to
each task, shows its state next to the code, tells Fran when something needs
him (CI broke, a reviewer commented) and cleans up when the PR is merged.

## Clarifications

### Session 2026-10-20

- Q: How much of changesets goes in the first version? → A: Tracking only: detect and show one PR per submodule and archive when all are merged; "Abrir PR" works per submodule, without cross-links between the PRs.
- Q: Should CI failures or review requests go to the agent on their own? → A: No; Delilah always waits for the user to choose "Pedir al agente que lo arregle".
- Q: On merge, archive straight away or ask first? → A: Straight away, with no confirmation; the no-lost-work guard (FR-018) still applies.
- Q: Which review comments should trigger a notification? → A: Everyone's except the signed-in account's own, bots included.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - See the task's PR and its state (Priority: P1)

When the agent (or Fran) opens a PR from a task's branch, Delilah finds it and
links it to the task. The task screen gets a "PR" tab next to Cambios showing
the PR's title, number and link, whether it is a draft, open, merged or closed,
the result of each CI check, and the review state (approved, changes requested,
waiting) with the number of comments. The board card and the task list show a
small PR badge with the same state at a glance.

**Why this priority**: it is the base the rest builds on (notifications and
archiving both need to know the PR) and it alone removes the trips to the
browser to check on a task.

**Independent Test**: in a preview with a fixture project whose remote is a test
repository, open a PR from a task's branch outside Delilah; within a minute the
task shows the PR tab with the right state and checks.

**Acceptance Scenarios**:

1. **Given** a task whose branch has no PR, **When** a PR is opened from that
   branch (by the agent, by Fran in a terminal or on the web), **Then** within a
   minute the task shows the PR with its number, title, link and state.
2. **Given** a task with a PR, **When** a CI check starts, passes or fails, or a
   review is submitted, **Then** the PR tab reflects it within a minute, without
   reloading.
3. **Given** a task with a PR, **When** I click the PR's link, **Then** it opens in
   the browser.
4. **Given** a task whose PR could not be found automatically (e.g. a different
   branch name), **When** I paste the PR's URL in the PR tab, **Then** it is linked
   to the task and tracked like a detected one.
5. **Given** a task without a PR, **When** I open the PR tab, **Then** it says there
   is no PR yet and offers to open one (Story 3) or link one by URL.

---

### User Story 2 - Be told when the PR needs me (Priority: P1)

When a CI check fails on the PR, or a reviewer requests changes or leaves new
comments, Delilah sends a macOS notification and adds an item to the Bandeja that
counts on the Dock badge. The item says what happened and offers to send it to
the task's agent ("Pedir al agente que lo arregle"), which sends the agent a
prompt with the failing checks or the review comments, in the current language.
When the PR is approved or merged, Delilah sends a notification but adds no
blocking item.

**Why this priority**: it is what makes the loop work without Fran watching: the
agent works, Delilah watches the PR, and Fran is only pulled in when needed.

**Independent Test**: with a linked PR in a test repository, make a CI check fail
and add a review comment; check that one notification and one Bandeja item arrive
for each, and that "Pedir al agente" sends the agent a prompt that contains them.

**Acceptance Scenarios**:

1. **Given** a task with a PR whose checks are running, **When** a check fails,
   **Then** I get one notification and one Bandeja item naming the task and the
   failed checks, and the Dock badge goes up by one.
2. **Given** that failure was already notified, **When** Delilah checks the PR
   again and nothing changed, **Then** no new notification or item is created.
3. **Given** a failed check, **When** a new commit is pushed and its checks fail
   again, **Then** it is notified again (once per pushed commit).
4. **Given** a task with a PR, **When** a reviewer requests changes or leaves new
   comments, **Then** I get one notification and one Bandeja item with the
   reviewer and the number of new comments.
5. **Given** a Bandeja item for a failure or a review, **When** I choose "Pedir al
   agente que lo arregle", **Then** the task's agent receives a prompt with the
   failing checks or the comments (resuming the session if it had ended), and the
   item is resolved.
6. **Given** a Bandeja item for a failure, **When** the checks pass on a later
   commit, **Then** the item is resolved on its own.

---

### User Story 3 - Open the PR from Delilah (Priority: P2)

From the task screen, Fran chooses "Abrir PR". Delilah pushes the branch if
needed and opens a PR against the project's default branch, with the task's title
as the title and a description that links the Jira ticket (when the task is a
Jira ticket) and summarises the change. Fran sees the proposed title and
description before it is created and can edit them, and can choose to open it as
a draft.

**Why this priority**: the agent already opens PRs today; this adds a reliable
way that doesn't depend on the agent remembering to do it, and puts the Jira link
in every PR.

**Independent Test**: from a task with commits on its branch and no PR, choose
"Abrir PR", accept the proposal, and check that the PR exists in the test
repository with the ticket link and is linked to the task.

**Acceptance Scenarios**:

1. **Given** a task with commits and no PR, **When** I choose "Abrir PR", **Then** I
   see a proposed title and description that I can edit, and a draft option.
2. **Given** that proposal, **When** I confirm, **Then** the branch is pushed if it
   wasn't, the PR is created, and the task shows it in the PR tab.
3. **Given** a task whose key is a Jira ticket and Jira is configured, **When** the
   proposal is built, **Then** the description contains a link to the ticket.
4. **Given** a free task (LIBRE-n), **When** the proposal is built, **Then** there is
   no ticket link.
5. **Given** the task's branch has no commits beyond the base, **When** I look at the
   task, **Then** "Abrir PR" is not offered (or is disabled, saying why).
6. **Given** the push or the creation fails (no permission, wrong account, no
   remote), **When** I confirm, **Then** I see the error from the hosting service in
   plain words and nothing is linked.

---

### User Story 4 - Clean up when the PR is merged (Priority: P2)

When the task's PR is merged, Delilah marks the task as done, stops its agent
sessions and archives it: the workspace is released (the worktree removed, or the
checkout returned to its previous branch) and the local branch is deleted. Fran
gets a notification saying the task was merged and archived. If the PR is closed
without merging, Delilah notifies him and does nothing else.

**Why this priority**: it removes the last manual step and frees the checkout for
the next task, but the loop is already useful without it.

**Independent Test**: merge a linked PR in the test repository and check that,
within a minute, the task is done and archived, its worktree and local branch are
gone, and a notification arrived.

**Acceptance Scenarios**:

1. **Given** a task with an open PR, **When** the PR is merged, **Then** within a
   minute the task is done and archived, its sessions are stopped, and I get a
   notification.
2. **Given** a merged task in worktree mode, **When** it is archived, **Then** its
   worktree is removed and its local branch deleted.
3. **Given** a merged task in checkout mode, **When** it is archived, **Then** the
   checkout goes back to its previous branch and the task's local branch is
   deleted.
4. **Given** a merged task whose workspace has uncommitted changes or commits that
   are not in the merged PR, **When** Delilah tries to archive it, **Then** it does
   not discard them: the task is marked done, nothing is removed, and a Bandeja
   item explains why and offers to archive it anyway.
5. **Given** a task with an open PR, **When** the PR is closed without merging,
   **Then** I get a notification and the task stays as it is.

---

### User Story 5 - Changesets: one PR per submodule (Priority: P3)

For a task on a parent project with several submodules, each touched submodule
gets its own PR. Delilah tracks all of them on the task: the PR tab lists each
submodule's PR with its state, the badge shows the worst state among them, and
the task is archived only when every PR is merged. In this version, PRs are
opened one submodule at a time and are not cross-linked to each other.

**Why this priority**: it is the original design's "changeset", and the work
projects use parent repos, but it builds on everything above and the first version
tracks changesets but does not open them as a linked set.

**Independent Test**: with the fixture parent project (`scripts/make-fixture-parent.sh`),
a task touching two submodules with a PR in each; merging one leaves the task
open, merging the second archives it.

**Acceptance Scenarios**:

1. **Given** a parent task touching two submodules, each with a PR, **When** I open
   the PR tab, **Then** I see both PRs, each with its submodule, state and checks.
2. **Given** one of the two PRs is merged, **When** Delilah checks again, **Then**
   the task is not archived.
3. **Given** both PRs are merged, **When** Delilah checks again, **Then** the task is
   archived as in Story 4.

4. **Given** a parent task touching two submodules, **When** I choose "Abrir PR",
   **Then** I pick the submodule and a PR is opened for it alone; PRs opened this way
   are not cross-linked to each other in this version.

---

### Edge Cases

- **Several PRs from the same branch** (one closed, one reopened): the open one wins;
  if none is open, the most recent.
- **PR opened against a fork or another repository**: only PRs in the project's own
  remote are detected automatically; others can be linked by URL.
- **The hosting CLI is missing, not signed in, or signed in as an account without
  access**: the PR tab says so in plain words and how to fix it; no notifications
  are sent and nothing else breaks. A failure is reported once, not on every check.
- **Two accounts on the machine** (work and personal): Delilah uses whichever
  account the hosting CLI resolves for that repository and shows which one in the
  PR tab; it never switches accounts on its own. Pushing uses git's own
  credentials, which may be a different account; if the push fails, its error is
  shown as in Story 3.
- **Same access problem**: the same cause (CLI missing, signed out, no access,
  unsupported remote) for the same project is reported once; it stays visible in
  the PR tab until it is fixed.
- **The merge happens while the agent is mid-turn or the task is paused**: the
  sessions are stopped and the task archived anyway (FR-017).
- **Network down or rate-limited**: the last known state stays visible, marked as
  "last checked at …"; checking resumes later without duplicate notifications.
- **The daemon was stopped while a PR changed**: on start it checks again and sends
  only the notifications for changes it hadn't seen (a merge still archives).
- **The task is deleted or archived by hand**: Delilah stops watching its PR.
- **A task with a PR is reopened or resumed after a merge**: not possible; once
  archived, the PR stays linked for reference only.
- **Comments by the user's own account** (including ones the agent posts through
  the same account) are counted in the tab but never notify.
- **A PR with no CI configured**: checks show "no checks"; this is not a failure.
- **Projects without a remote, or with a remote that isn't a supported host**: no PR
  tab actions; the tab explains that PRs are not available for that project.
- **The branch was renamed or deleted on the remote before merging**: the linked PR
  is still tracked by its number.

## Requirements *(mandatory)*

### Functional Requirements

**Linking**
- **FR-001**: Delilah MUST detect a PR whose head is the task's branch in the
  project's remote, whoever opened it, and link it to the task.
- **FR-002**: Delilah MUST start looking for a task's PR when the agent pushes or
  opens a PR (the existing signal), when Fran opens the task, and periodically for
  tasks in Revisión, so a PR opened outside Delilah is found within a minute.
- **FR-003**: Users MUST be able to link a PR to a task by pasting its URL, and to
  unlink it.
- **FR-004**: A task's linked PRs MUST persist across daemon restarts.
- **FR-024**: The repository and host of a PR MUST come from the remote of the
  repository the branch lives in: the project's repo, or each submodule's own repo
  for parent projects.
- **FR-025**: A PR the user unlinked MUST NOT be linked again automatically; a
  different PR from the same branch can be.
- **FR-026**: Among several PRs from one branch, Delilah MUST pick an open one
  first, then a merged one, then a closed one, the most recent in each group.
- **FR-027**: Linking a PR MUST move the task to Revisión if its phase is below it.
  A merge marks the task done (FR-017); a close changes neither phase nor status.

**Showing**
- **FR-005**: The task screen MUST have a PR tab showing, per PR: number, title,
  link, state (draft, open, merged, closed), base branch, each CI check with its
  result, the review decision, the number of review comments, and when it was last
  checked.
- **FR-006**: Board cards and task lists MUST show a compact PR badge for tasks
  with a PR (state plus CI result).
- **FR-007**: PR state shown in the app MUST update live, within a minute of the
  change on the host, while the task is not archived.
- **FR-028**: Delilah MUST check every 30 seconds the PRs of non-archived tasks that
  have one open, and look for a PR every 30 seconds for tasks in Revisión without
  one; it MUST check at once when a task is opened in the app or the agent pushes
  or opens a PR. Other tasks are not checked.
- **FR-029**: After a failed check (network, host error, rate limit), Delilah MUST
  wait twice as long before the next one, up to 10 minutes, or until the host's
  rate-limit reset; the app MUST keep showing the last known state with the time
  it was checked.

**Notifying**
- **FR-008**: Delilah MUST notify (macOS notification and Bandeja item counting on
  the Dock badge) when a CI check fails on the PR's latest commit, once per commit.
- **FR-009**: Delilah MUST notify (notification and Bandeja item) when a reviewer
  requests changes or new review comments appear, once per new review or batch of
  comments. Comments and reviews by anyone count, bots included, except those by
  the account Delilah uses on the host (FR-021).
- **FR-010**: Delilah MUST send a non-blocking notification when a PR is approved,
  merged or closed without merging.
- **FR-011**: Delilah MUST NOT send failures or reviews to the agent on its own. Failure and review items MUST offer to send the details to the
  task's agent as a prompt in the current language, resuming its session if
  needed; doing so resolves the item.
- **FR-012**: A failure item MUST resolve on its own when a later commit's checks
  pass.
- **FR-013**: Notifications MUST NOT repeat for a change already notified,
  including after a daemon restart. The unit is the head commit for CI, the review
  for reviews, and the newest comment seen for comments; a "batch" is every new
  comment found in one check.
- **FR-030**: After a restart or a gap in checking, Delilah MUST notify only the
  difference between the last saved state and the current one, not every missed
  event.
- **FR-031**: A commit's CI counts as failed when any check ends failed, timed out
  or needing action; cancelled, skipped and neutral checks are not failures. It is
  notified when the head commit first becomes failed.
- **FR-032**: Every PR item in the Bandeja (failure or review) MUST resolve when the
  PR is merged or closed, when the task is archived or deleted, or when the user
  forwards or dismisses it.
- **FR-033**: macOS notifications MUST follow today's rule (sent only when the app
  is not in front); the Bandeja item is always added.
- **FR-034**: "Pedir al agente" MUST behave like any other reply from the Bandeja
  (delivered after the agent's current turn); while the task is paused by the
  usage limit it MUST be disabled, saying when it will be available.
- **FR-035**: Comments forwarded to the agent MUST be quoted as the reviewers'
  content, with author and location, framed as data to consider and not as
  instructions from the user.

**Opening**
- **FR-014**: Users MUST be able to open a PR from the task screen when the task's
  branch has commits beyond its base and no open PR.
- **FR-015**: Before creating it, Delilah MUST show an editable title and
  description and a draft option; the title defaults to the task's title and the
  description includes a link to the Jira ticket when the task is one and Jira is
  configured.
- **FR-016**: Opening MUST push the branch first when it isn't on the remote, and
  MUST report failures in plain words without linking anything.

**Cleaning up**
- **FR-017**: When all of a task's PRs are merged, Delilah MUST, without asking, mark the task done,
  stop its sessions, release its workspace and delete its local branch, then notify.
- **FR-018**: That clean-up MUST NOT discard work: if the workspace has
  uncommitted changes, or the local branch has commits that are not in the PR's
  final head commit (its head equals it or is an ancestor of it, so squash and
  rebase merges count as safe), Delilah MUST mark the task done but leave the
  workspace and branch, and add a Bandeja item offering a forced archive. This
  applies to checkout mode too, overriding today's rule that a checkout archive
  never refuses.
- **FR-036**: In checkout mode the archive MUST first return the checkout to its
  previous branch and only then delete the task's branch.
- **FR-037**: Delilah MUST NOT delete remote branches.
- **FR-038**: In a changeset, only submodules get PRs; the merge archive deletes
  the submodules' merged branches and leaves the parent repository's branch as is.
  Uncommitted changes in the parent still block it (FR-018).
- **FR-039**: When one of a changeset's PRs is closed without merging, the task is
  not archived automatically even if the others are merged; Delilah notifies and
  the user can archive it by hand.
- **FR-040**: If the archive fails partway (e.g. the workspace is released but a
  branch can't be deleted), the task MUST stay done and a Bandeja item MUST say
  what was left; archiving by hand retries the rest.
- **FR-019**: A PR closed without merging MUST only notify; the task stays as is.
- **FR-020**: Delilah MUST stop watching a task's PRs when the task is archived or
  deleted.

**Access and language**
- **FR-021**: Delilah MUST use the account the hosting CLI already has for the
  repository, MUST show which account in the PR tab, and MUST NOT store or switch
  credentials.
- **FR-022**: When PR access isn't possible (CLI missing, signed out, no access,
  unsupported remote), the PR tab MUST explain why and how to fix it, and the
  problem MUST be reported at most once until it changes.
- **FR-023**: All new text in the app, notifications and prompts to the agent MUST
  exist in Spanish and English.

### Key Entities

- **Pull request link**: belongs to a task (and, for parent projects, to one
  submodule); has the host's PR number and URL, how it was linked (detected,
  opened by Delilah, pasted), and the last known snapshot.
- **PR snapshot**: state, draft flag, head commit, base branch, CI checks (name,
  result), review decision, comment count, time checked. Compared with the
  previous snapshot to decide what to notify.
- **PR notice**: a Bandeja item (CI failed, changes requested, new comments,
  couldn't archive) tied to a task, a PR and the commit or review that caused it,
  so it is created once and resolved when it no longer applies.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: A PR opened from a task's branch, by anyone, appears on the task
  within 1 minute in 95 % of cases.
- **SC-002**: A CI failure or a new review is notified within 2 minutes, and never
  more than once for the same commit or review.
- **SC-003**: After a PR is merged, the task is done and its workspace released
  within 2 minutes without Fran doing anything, unless there is unsaved work.
- **SC-004**: Following a task from "agent finished" to "merged and archived"
  needs no visit to the browser except to review the code itself.
- **SC-005**: No work is ever lost by an automatic archive, in at least these
  cases: uncommitted changes, local commits not pushed, and commits pushed after
  the PR was merged.
- **SC-006**: When PR access fails, Fran understands the cause from the PR tab
  alone, without reading logs.

## Assumptions

- The hosting service is GitHub (github.com or GitHub Enterprise), reached through
  the `gh` CLI the user has already signed in; other hosts are out of scope.
- The PR's base is the remote's default branch; choosing another base is out of
  scope for this version.
- Checking every 30 seconds for tasks in Revisión (and on demand when a task is
  opened) is enough; webhooks are out of scope because the daemon only listens on
  loopback.
- Saving what was learned to QMD after a merge (in the original design) is out of
  scope.
- Reviewing the code and replying to review comments still happen on the host;
  Delilah only shows counts and forwards comments to the agent.
- Archiving on merge reuses today's archive rules; the only new guard is FR-018.
- The Jira link uses the site configured in Ajustes; with sample data or no Jira,
  no link is added.
