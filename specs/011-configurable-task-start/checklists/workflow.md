# Workflow Commands Requirements Quality Checklist: Configurable workflow commands per project

**Purpose**: Unit tests for the spec's requirements on migration, saving-mode/autopilot gating, template validation, skill discovery and start precedence
**Created**: 2026-10-26
**Feature**: [spec.md](../spec.md)
**Depth**: Standard · **Audience**: author, before planning

## Migration & Backwards Compatibility

- [x] CHK001 - Is it specified which existing projects get the "Spec Kit (`/feature`)" preset and which new ones get the built-in prompt? [Completeness, Spec §FR-002]
- [x] CHK002 - Are requirements defined for existing tasks (backlog and running) created before the feature, not only projects? [Gap → added FR-015]
- [x] CHK003 - Is the behaviour of legacy backlog tasks with workflow `prompt` and no stored message defined? [Edge Case, Spec §Edge Cases]
- [x] CHK004 - Is "exactly as before" measurable — are today's commands pinned so the preset can be compared against them? [Measurability, Spec §FR-011, §SC-002]
- [x] CHK005 - Is it stated that the migration needs no manual step and that old clients keep working? [Completeness, Spec §FR-002, §FR-013]

## Saving Mode & Autopilot Gating

- [x] CHK006 - Is "follows a phased workflow" defined for a task, including a task overridden away from its project's phased workflow? [Clarity, Spec §FR-009, US4-AS2]
- [x] CHK007 - Is the effect of the global saving-mode preference on non-phased tasks explicit? [Clarity, Spec §FR-009]
- [x] CHK008 - Are requirements defined for a running phased task whose project switches to single start before its next hand-off? [Gap → added to Edge Cases]
- [x] CHK009 - Is the refusal of autopilot on non-phased tasks consistent with decision 46 and its error code? [Consistency, Spec §FR-009, US3-AS3]
- [x] CHK010 - Do the phased requirements state the dependency on Spec Kit's files for hand-offs? [Assumption, Spec §Assumptions]

## Template Validation

- [x] CHK011 - Is the allowed placeholder set specified per field (`{spec_dir}` only in implementation)? [Clarity, Spec §FR-004]
- [x] CHK012 - Is the behaviour of `{spec_dir}` in plan/review commands defined (refused)? [Consistency, Spec §FR-004]
- [x] CHK013 - Are blank-field rules defined for each workflow kind (custom start, each phased command)? [Completeness, Spec §FR-004, US3-AS4]
- [x] CHK014 - Is it defined how a literal brace (e.g. JSON in a message) is written without being read as a placeholder? [Gap → added to Edge Cases]
- [x] CHK015 - Are the same validation rules required for per-task overrides as for project settings? [Consistency → added to FR-012]

## Skill Discovery

- [x] CHK016 - Are the discovery sources listed (user, project, plugins; skills and commands)? [Completeness, Spec §FR-005, §Assumptions]
- [x] CHK017 - Is the invocable name for plugin skills and duplicate names across sources defined? [Gap → added to FR-005]
- [x] CHK018 - Is the failure behaviour of discovery (unreadable folders) defined so it never blocks? [Edge Case, Spec §Edge Cases]
- [x] CHK019 - Is the "not found" warning's effect on saving and starting unambiguous? [Clarity, Spec §FR-006]
- [x] CHK020 - Is it stated that discovered skills are read-only and never stored or executed by the listing? [Non-Functional, Spec §Key Entities]

## Start Precedence

- [x] CHK021 - Is the full precedence chain specified, including the no-project case? [Completeness, Spec §FR-007, §Edge Cases]
- [x] CHK022 - Can a start request express each per-task choice ("follow the project", a message, nothing), not only a message? [Gap → added to FR-007]
- [x] CHK023 - Is it defined when the notes pointer is appended (every sent message, never with "nothing")? [Clarity, Spec §FR-007, §FR-008]
- [x] CHK024 - Is it specified that only the first session receives the start and hand-offs receive phase commands? [Coverage, Spec §Edge Cases]
- [x] CHK025 - Is it stated whether a backlog task's stored start survives project setting changes? [Consistency, Spec §FR-012, US4]

## Notes

- Gaps found in CHK002, CHK008, CHK014, CHK015, CHK017 and CHK022 were fixed in spec.md in the same pass.
- Exact preset commands (the "Spec Kit (oficial)" review command, the built-in prompt wording) are pinned in the plan.
