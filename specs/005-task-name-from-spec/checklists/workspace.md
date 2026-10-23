# Workspace & Rename Requirements Checklist: Task name and branch from the first analysis

**Purpose**: Unit tests for the requirements on the branch lifecycle, workspaces and the title rename, before planning and during PR review
**Created**: 2026-10-23
**Feature**: [spec.md](../spec.md)
**Depth**: standard · **Audience**: PR reviewer

## Requirement Completeness

- [ ] CHK001 - Is "the project's base branch" defined for each mode (the project's default branch, or whatever the checkout is on when the task starts)? [Gap, Spec §FR-001]
- [ ] CHK002 - Is the state of a worktree before the analysis (no branch of its own) specified clearly enough to rule out a provisional branch? [Clarity, Spec §FR-001]
- [ ] CHK003 - Are the three ways a task gets its branch (the specify hook, Delilah from the spec folder, the title fallback) listed together, with which one wins if two happen? [Completeness, Spec §FR-002, §FR-002a, §FR-007]
- [ ] CHK004 - Is the moment "the first analysis has been written" defined (spec folder created, spec file present, its title heading present)? [Clarity, Spec §FR-002]
- [ ] CHK005 - Is it specified which repo's spec folder counts for a parent project whose analysis lands in a submodule? [Completeness, Spec §Edge Cases]
- [ ] CHK006 - Are the requirements for applying the branch to every repo of a parent task (parent and each chosen submodule) stated, including the order and what "applied" means? [Completeness, Spec §FR-006]
- [ ] CHK007 - Is the slug rule for the title fallback (characters, length, numbering prefix or not) specified? [Gap, Spec §FR-007]
- [ ] CHK008 - Is it stated where the previous title is shown and how "restore" is offered (task screen only, or also the backlog)? [Clarity, Spec §US1, §FR-004]
- [ ] CHK009 - Is editing the title of a running task (not only Queued) required explicitly, on every surface where the title is edited today? [Completeness, Spec §FR-004]

## Requirement Clarity

- [ ] CHK010 - Is "the spec's title exactly as written" defined for specs without the standard heading, or with an empty one? [Edge Case, Spec §Clarifications]
- [ ] CHK011 - Is "the session's first edit of a file outside `specs/`" defined for edits by the user, not the agent, and for edits in submodules? [Clarity, Spec §FR-007]
- [ ] CHK012 - Is "a free variant (the next number)" defined, i.e. does the number or a suffix change, and does the spec folder then disagree with the branch? [Ambiguity, Spec §FR-010]
- [ ] CHK013 - Is "the user has edited the title by hand" scoped (only after the automatic rename, or also before the analysis)? [Clarity, Spec §FR-005, §US1-4]

## Requirement Consistency

- [ ] CHK014 - Do FR-006 ("named like the spec folder") and FR-010 (free variant on collision) agree on what the branch is called when the name is taken? [Conflict, Spec §FR-006, §FR-010]
- [ ] CHK015 - Are FR-014 (existing tasks untouched) and FR-001 consistent for a task created before the upgrade but started after it (backlog)? [Consistency, Gap]
- [ ] CHK016 - Is "Tasks with a Jira key keep today's behaviour" (FR-013) consistent with the title rename for a free task later linked to a ticket, if that exists? [Consistency, Spec §FR-013]
- [ ] CHK017 - Do the edge case "branch can't be applied" and FR-011 describe the same outcome for a partially applied branch (some repos switched, others not)? [Consistency, Spec §FR-011]

## Scenario Coverage

- [ ] CHK018 - Are requirements defined for a checkout-mode task when the checkout isn't on the base branch at start? [Coverage, Gap]
- [ ] CHK019 - Are two worktree-mode free tasks in the same project, running specify at the same time and picking the same `NNN`, covered? [Coverage, Spec §FR-010]
- [ ] CHK020 - Is the backlog → start path (task saved first, started later) covered the same way as a direct start? [Coverage, Spec §FR-001]
- [ ] CHK021 - Is the saving-mode handoff (a new session after `tasks.md`) covered, so the next session starts on the descriptive branch? [Coverage, Gap]
- [ ] CHK022 - Are project sessions turned into free tasks (decision 16 C) covered: where does their branch come from? [Coverage, Gap]
- [ ] CHK023 - Is the task-from-idea flow's title (proposed by Claude) explicitly overwritten by the analysis, as the edge case says, and is that the intended behaviour? [Coverage, Spec §Edge Cases]

## Exception & Recovery Flows

- [ ] CHK024 - Are the contents of the Bandeja item for a failed branch application specified (reason, repos affected, retry action)? [Completeness, Spec §FR-011]
- [ ] CHK025 - Is it defined what "retry" re-attempts and when it gives up? [Clarity, Spec §FR-011]
- [ ] CHK026 - Is recovery after a daemon restart specified for each step (branch created but not recorded, recorded but not applied to submodules, title not updated)? [Recovery, Spec §FR-016]
- [ ] CHK027 - Is the behaviour defined when the analysis is deleted or reverted before Delilah picks it up? [Edge Case, Gap]
- [ ] CHK028 - Is it specified what happens to commits already made on the detached worktree head if the fallback branch is created late? [Edge Case, Spec §FR-007]

## Compatibility (PR, archive, delete, progress)

- [ ] CHK029 - Are PR creation, archive, archive on merge and delete each required to work for a task that never got a branch (no analysis, no edit)? [Coverage, Gap, Spec §FR-015]
- [ ] CHK030 - Is the archive's "go back to previous branches" requirement (checkout mode) defined when the task started without its own branch? [Consistency, Spec §FR-015]
- [ ] CHK031 - Is the spec progress (phase, done/total) required to find the spec folder before the branch exists? [Completeness, Spec §FR-015]
- [ ] CHK032 - Is the branch shown anywhere in the UI, and if so is "no branch yet" a defined state? [Gap]

## Acceptance Criteria Quality

- [ ] CHK033 - Is SC-001's "within 10 seconds of the spec being written" measurable from a defined start event? [Measurability, Spec §SC-001]
- [ ] CHK034 - Can SC-003 ("zero commits on the base branch") be verified in the fixtures for both modes and parent projects? [Measurability, Spec §SC-003]
- [ ] CHK035 - Does each user story's independent test cover the parent-with-submodule fixture, not only the plain repo? [Coverage, Spec §US2]

## Dependencies & Assumptions

- [ ] CHK036 - Is the dependency on Spec Kit's `before_specify` hook behaviour (creates the branch from HEAD in the cwd repo, sequential numbering scanning branches and `specs/`) documented and its version pinned? [Dependency, Spec §Assumptions]
- [ ] CHK037 - Is the assumption that the hook works in a detached worktree validated? [Assumption]
- [ ] CHK038 - Is the `/feature` skill's own branch step (it tells the agent to rely on the hook) listed as something this feature depends on or changes? [Dependency, Gap]

## Notes

- Resolve or accept each item before `/speckit-plan` finalises the design; items answered by the plan can be checked with a pointer to plan.md.
