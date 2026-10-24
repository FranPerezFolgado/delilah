# Workspace Settings Requirements Checklist: Per-project configuration

**Purpose**: Unit tests for the spec's requirements on settings, workspace preparation,
submodule rules, compatibility and the API/CLI contract. For the author before planning
and the reviewer at PR time.
**Created**: 2026-10-20
**Feature**: [spec.md](../spec.md)

## Settings validation and screen UX

- [x] CHK001 - Is every setting's default specified, so an empty project matches today? [Completeness, Spec §US1-1, FR-002]
- [x] CHK002 - Is each invalid value the screen must reject listed (path outside the project, template without `{ticket}`, unknown submodule)? [Clarity, Spec §US1-3, FR-004]
- [x] CHK003 - Is it defined what a template with a placeholder other than `{ticket}` / `{slug}` is? [Gap, Spec §FR-007]
- [x] CHK004 - Is the case of the same submodule in both "always" and "never" defined? [Gap, Spec §FR-012]
- [x] CHK005 - Is it specified what happens to unsaved edits when the user leaves the settings screen? [Gap, Spec §FR-003]
- [x] CHK006 - Is the screen's entry point stated (from Projects) and is the repo-project variant (no submodule rules) defined? [Completeness, Spec §US1-4, FR-003]
- [x] CHK007 - Is "under 1 minute" in SC-001 measurable with a defined set of settings to fill? [Measurability, Spec §SC-001]

## Workspace preparation

- [x] CHK008 - Are the `{slug}` rules (characters, case, length, cut at a word boundary) unambiguous? [Clarity, Spec §FR-007, Edge Cases]
- [x] CHK009 - Is precedence defined between an explicit branch, the template and the default? [Consistency, Spec §US2-3, FR-007]
- [x] CHK010 - Are copy and link defaults defined per entry kind (file vs folder)? [Clarity, Spec §FR-008]
- [x] CHK011 - Is the behaviour defined when a shared entry is also tracked by git (already in the worktree)? [Gap, Spec §FR-008]
- [x] CHK012 - Is a missing shared entry's behaviour defined (start anyway, noted in the log)? [Edge Case, Spec §US3-7]
- [x] CHK013 - Is the order of steps fixed (worktree → shared entries → setup → agent)? [Clarity, Spec §FR-009]
- [x] CHK014 - Is it stated that checkout tasks get neither shared entries nor setup? [Consistency, Spec §Clarifications, Edge Cases]
- [x] CHK015 - Is what the user sees while the setup runs (up to 10 minutes) specified? [Gap, Spec §FR-009]
- [x] CHK016 - Are both recovery actions after a failed setup defined, and what each does? [Completeness, Spec §FR-010, US3-4/5]
- [x] CHK017 - Is it defined whether the recovery actions survive a daemon restart? [Gap, Spec §FR-010]
- [x] CHK018 - Is the cleanup of a worktree kept by a failed setup covered (archive, delete)? [Coverage, Spec §Edge Cases]
- [x] CHK019 - Is "the end of the output" quantified? [Clarity, Spec §FR-010]

## Submodule rules

- [x] CHK020 - Is the merge rule for "always" and "never" with the user's selection and the suggestions explicit? [Clarity, Spec §FR-012]
- [x] CHK021 - Is a rule naming a submodule the parent no longer has handled at start time? [Edge Case, Spec §US4-3]
- [x] CHK022 - Is the new-task form's treatment of "always" (selected, locked) and "never" (hidden) consistent with FR-012? [Consistency, Spec §US4-1, FR-013]

## Backwards compatibility

- [x] CHK023 - Is "no settings → exactly today's behaviour" stated as a requirement and a success criterion? [Consistency, Spec §FR-002, SC-005]
- [x] CHK024 - Is the effect on running tasks and on backlog tasks of a settings change defined? [Coverage, Spec §Edge Cases]
- [x] CHK025 - Is the fate of settings on project rename, path change and removal defined? [Coverage, Spec §Edge Cases]

## API and CLI contract

- [x] CHK026 - Are the CLI commands to show and to change settings named? [Clarity, Spec §FR-005]
- [x] CHK027 - Is it specified that a project with no settings reports the defaults, so clients need no special case? [Gap, Spec §FR-002]
- [x] CHK028 - Is room for PR options stated without committing to their shape? [Completeness, Spec §FR-014]

## Non-functional

- [x] CHK029 - Is the trust assumption for the setup command documented? [Assumption, Spec §Assumptions]
- [x] CHK030 - Is the start-time overhead without a setup quantified? [Measurability, Spec §SC-006]
- [x] CHK031 - Is localisation of the new texts required? [Completeness, Spec §FR-015]

## Notes

- First pass found gaps in CHK003, CHK004, CHK005, CHK011, CHK015, CHK017, CHK019,
  CHK026 and CHK027. All were fixed in the spec the same day (FR-004, FR-005, FR-007,
  FR-008, FR-010, FR-012, FR-016, FR-017, Edge Cases), so every item now passes.
