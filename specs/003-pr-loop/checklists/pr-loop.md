# PR Loop Requirements Checklist: hosting integration, notices and archive on merge

**Purpose**: Author self-check of the spec's requirements before planning
**Created**: 2026-10-20
**Feature**: [spec.md](../spec.md)
**Focus**: hosting-service integration, notification de-duplication and state transitions, archive-on-merge safety. Depth: standard.

## Hosting service integration

- [x] CHK001 - Is it specified how the repository and host (github.com vs Enterprise) are determined for a task, including for each submodule of a parent project? [Completeness, Spec §FR-001, §Assumptions]
- [x] CHK002 - Is the behaviour defined when the account `gh` resolves differs from the one git pushes with? [Edge Case, Spec §FR-021, Gap]
- [x] CHK003 - Is "reported at most once until it changes" defined: what counts as the same access problem, and where is it shown after the first report? [Clarity, Spec §FR-022]
- [x] CHK004 - Are rate-limit and network-failure requirements quantified (back-off, when checking resumes, how stale data is marked)? [Clarity, Spec §Edge Cases, Gap]
- [x] CHK005 - Is the check cadence specified for every class of task (in Revisión without a PR, with an open PR, done but not archived, opened on screen)? [Completeness, Spec §FR-002, §FR-007]
- [x] CHK006 - Is it specified whether Delilah deletes the remote branch after a merge? [Gap, Spec §FR-017]

## Linking and phases

- [x] CHK007 - Is it defined what happens after the user unlinks a PR: can automatic detection link the same PR again? [Conflict, Spec §FR-001, §FR-003]
- [x] CHK008 - Are the phase and status changes caused by the PR (linked, merged, closed) defined and consistent with the existing phase signals? [Consistency, Gap]
- [x] CHK009 - Is "the open one wins; if none is open, the most recent" enough to choose among several PRs from one branch, including merged ones? [Clarity, Spec §Edge Cases]

## Notifications and de-duplication

- [x] CHK010 - Is the unit of de-duplication defined for each notice kind (commit SHA for CI, review id or comment watermark for reviews)? [Clarity, Spec §FR-008, §FR-009, §FR-013]
- [x] CHK011 - Is it defined what is notified after a restart when several changes were missed: one notice per missed event, or only the difference with the last saved snapshot? [Ambiguity, Spec §Edge Cases]
- [x] CHK012 - Is "a check fails" defined for checks that are cancelled, skipped, neutral or timed out, and for a re-run that turns red then green? [Clarity, Spec §FR-008]
- [x] CHK013 - Is it specified when review items resolve (new review, merge, close, archive), not only failure items? [Gap, Spec §FR-012]
- [x] CHK014 - Is it consistent with today's rule that macOS notifications are sent only when the app is not in front? [Consistency, Spec §FR-008, §FR-010]
- [x] CHK015 - Is "batch of comments" defined (all new comments found in one check)? [Clarity, Spec §FR-009]
- [x] CHK016 - Is the behaviour of "Pedir al agente" defined when the task is paused by the usage limit or its agent is mid-turn? [Edge Case, Spec §FR-011, Gap]
- [x] CHK017 - Are review comments from third parties treated as untrusted when forwarded to the agent (quoted as data, not instructions)? [Security, Spec §FR-011, Gap]

## Archive on merge

- [x] CHK018 - Is "commits not contained in the merged PR" defined so that squash and rebase merges (where the branch's commits are not in the base) still count as safe? [Clarity, Spec §FR-018]
- [x] CHK019 - Does FR-018 explicitly override today's checkout-mode archive, which never refuses and carries uncommitted files back? [Conflict, Spec §FR-018, §Assumptions]
- [x] CHK020 - Is the order of the archive steps for checkout mode specified (go back to the previous branch before deleting the task's branch)? [Completeness, Spec §FR-017]
- [x] CHK021 - Is it defined what the parent repository's own branch gets in a changeset (no PR; deleted or kept on archive)? [Gap, Spec §User Story 5]
- [x] CHK022 - Is the behaviour defined for a changeset where one PR is merged and another is closed without merging? [Edge Case, Spec §FR-017, §FR-019]
- [x] CHK023 - Is the behaviour defined when the merge happens while the agent is mid-turn or the task is paused? [Edge Case, Spec §FR-017]
- [x] CHK024 - Is it defined what happens if the archive itself fails partway (worktree removed, branch delete fails)? [Recovery, Gap]

## Acceptance and measurability

- [x] CHK025 - Are SC-001 and SC-002 consistent with a 30 s check cadence plus host delays, and measurable in a preview? [Measurability, Spec §SC-001, §SC-002]
- [x] CHK026 - Can SC-005 ("no work is ever lost") be checked with defined cases (uncommitted, unpushed, pushed after the PR's head)? [Measurability, Spec §SC-005]

## Notes

- All items resolved in the spec on 2026-10-20 with defaults: FR-024 to FR-040,
  new edge cases (accounts, same access problem, merge mid-turn) and SC-005 cases.
- CHK025: 30 s cadence plus host delay fits SC-001 (1 min) and SC-002 (2 min).
