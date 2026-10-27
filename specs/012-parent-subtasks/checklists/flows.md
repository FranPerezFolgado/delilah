# Flows Checklist: Parent tasks and subtasks (integration branch)

**Purpose**: Release-gate check of the requirements' quality for the git/PR flows, the hierarchy rules and the API/CLI contract, by the author before planning
**Created**: 2026-10-27
**Feature**: [spec.md](../spec.md)

## Git and PR flows: completeness

- [x] CHK001 Is the base of a subtask's branch defined for every repository a parent task works in (parent repository with submodules)? [Completeness, Spec §Edge Cases]
- [x] CHK002 Is it defined which method is used, and in what order, when the repository allows neither the preferred method nor only one alternative? [Clarity, Spec §FR-017]
- [x] CHK003 Does the spec say how the "behind" indicator of a parent task counts commits: default branch only, or also merged subtasks not yet in its workspace? [Gap, Spec §FR-015, §FR-016]
- [x] CHK004 Is the behaviour of a parent task's "Update branch" defined when bringing in its remote commits conflicts with local, unpushed work? [Edge Case, Spec §FR-016]
- [x] CHK005 Is it defined what happens to a subtask PR opened outside Delilah against the default branch? [Gap, Spec §FR-013]
- [x] CHK006 Is it defined whether the empty commit is pushed immediately and with what message? [Clarity, Spec §FR-014]
- [x] CHK007 Is the requirement that the parent task's PR be a draft consistent with a user who wants to open it ready (all subtasks already merged)? [Consistency, Spec §FR-014, §FR-023]

## Hierarchy rules: consistency

- [x] CHK008 Is "live subtask" consistent with a subtask that is merged but whose archive was held back (leftover changes): does it block the parent task's archive? [Ambiguity, Spec §Terms, §FR-026, §FR-028]
- [x] CHK009 Is "dropped" consistent between archived-unmerged and deleted subtasks, and with a subtask whose PR was closed without merging? [Consistency, Spec §FR-021, §FR-022]
- [x] CHK010 Is detaching a subtask from its parent task (or turning a parent task back into a normal task) explicitly excluded or specified? [Gap, Spec §FR-008]
- [x] CHK011 Is reopening a done parent task or subtask covered? [Gap]
- [x] CHK012 Are the refusals (start before the parent task, nesting, convert, archive, delete) each given a user-visible reason? [Completeness, Spec §FR-006, §FR-011, §FR-026]
- [x] CHK013 Is it defined whether a parent task with zero subtasks behaves as a parent task (progress, ready suggestion)? [Edge Case, Spec §FR-023]

## API and CLI contract

- [x] CHK014 Is the CLI way to create a parent task named, not only "a way to"? [Clarity, Spec §FR-005]
- [x] CHK015 Are the CLI and API operations for turning a task into a parent task and attaching a subtask specified? [Completeness, Spec §FR-008]
- [x] CHK016 Is it stated that the API changes are additive, so older clients keep working? [Gap, Spec §FR-029]
- [x] CHK017 Is the CLI listing of a parent task's subtasks and progress specified (which command)? [Clarity, Spec §FR-021]

## Acceptance criteria and coverage

- [x] CHK018 Can SC-006 "with no manual refresh" be verified objectively (a time bound)? [Measurability, Spec §SC-006]
- [x] CHK019 Does SC-003 account for repositories that allow only squash (as Delilah's own)? [Consistency, Spec §SC-003, §FR-017]
- [x] CHK020 Are notifications defined for the parent task when a subtask is merged or dropped? [Gap]
- [x] CHK021 Are concurrent merges of two subtasks addressed (progress and "behind" of the others)? [Coverage, Spec §SC-006]

## Notes

- Author pass 2026-10-27: spec updated (Terms: live/dropped; FR-013, FR-014, FR-016,
  FR-016a, FR-017, FR-008, FR-023, FR-029, Reopening, SC-006, out of scope).
- CHK014: the exact CLI flag for creating a parent task is left to the plan (FR-005 asks
  for one; the plan names it).
- Open for the plan (research.md): CHK006 (empty commit message and push), CHK015 (names
  of the convert/attach operations), CHK021 (concurrent subtask merges).
- Settled in research.md: CHK006 (R11: `chore: start <KEY> <title>`, pushed before the
  PR opens), CHK014/CHK015 (R14: `new --parent-task`, `make-parent-task`, `attach`),
  CHK021 (R14: progress derived on read, never stored).
