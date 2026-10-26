# Permissions Requirements Quality Checklist: Chat mode asks for permission as often as terminal mode

**Purpose**: Unit tests for the spec's requirements on permission safety, parity, test-first evidence and autopilot pauses, for a reviewer before planning.
**Created**: 2026-10-26
**Feature**: [spec.md](../spec.md)

## Requirement Clarity: what "parity" means

- [x] CHK001 - Is "the same permission behaviour as the terminal" defined precisely enough to tell, for a given action, whether chat mode is in parity? [Clarity, Spec §FR-003]
- [x] CHK002 - Is it stated whose default counts as "terminal behaviour" (the user's Claude Code default at session start, not a value Delilah picks)? [Clarity, Spec §Assumptions]
- [x] CHK003 - Is it stated what happens to parity if the user changes their default after a session has started? [Gap]
- [x] CHK004 - Is "routine workspace action" bounded by a listed set, so SC-001 can be measured the same way each time? [Measurability, Spec §SC-001]

## Permission safety (never more permissive)

- [x] CHK005 - Is "never grant more than terminal mode" stated as a requirement that a test can check, not only as intent? [Measurability, Spec §FR-004]
- [x] CHK006 - Are the excluded mechanisms listed explicitly (bypassing permissions, widening the allow list, the repo's own settings widening it)? [Completeness, Spec §FR-004]
- [x] CHK007 - Do the requirements say idea conversations and read-only sessions keep their modes in both runtimes? [Consistency, Spec §FR-002, §FR-004]
- [x] CHK008 - Does the spec say how a denial by the automatic mode is shown to the user, so silent refusals cannot hide problems? [Clarity, Spec §Edge Cases]
- [x] CHK009 - Is the requirement that a fallback never asks for everything consistent with the requirement that it never allows more? Which wins if the CLI rejects the setting? [Consistency, Spec §FR-008]

## Test-first and spike evidence

- [x] CHK010 - Does the spike requirement name the candidate settings to compare and the minimum set of actions to try? [Completeness, Spec §FR-001]
- [x] CHK011 - Is it stated that the spike's findings gate the fix, so the plan cannot choose a mechanism the spike contradicts? [Gap, Spec §US3]
- [x] CHK012 - Is the environment the spike runs in defined (fixture repo, not the installed app, not work repos)? [Completeness, Spec §Assumptions]
- [x] CHK013 - Is it stated that the new tests must fail on the current code, and how that is recorded? [Measurability, Spec §FR-002, §SC-004]
- [x] CHK014 - Is it stated how the fake agent represents "an action that needs permission" versus "a routine action", so the end-to-end test is not tautological? [Clarity, Spec §FR-007]

## Autopilot pause behaviour

- [x] CHK015 - Is it explicit that decision #43 is unchanged: asks that still arrive pause autopilot and are never answered automatically? [Consistency, Spec §FR-005]
- [x] CHK016 - Is it defined whether a denial by the automatic mode (no ask) counts as a pause, a history event, or nothing? [Gap, Spec §Edge Cases]
- [x] CHK017 - Are requirements defined for switching runtime while autopilot runs (the relaunched session's behaviour, no new pauses)? [Coverage, Spec §US1 AS3]

## Scenario and edge-case coverage

- [x] CHK018 - Are remembered "always in this task" rules covered for chat mode after the change? [Coverage, Spec §FR-006]
- [x] CHK019 - Is the case where the CLI version does not support the chosen setting covered, with a defined user-visible message? [Edge Case, Spec §FR-008]
- [x] CHK020 - Is it stated that project sessions, shells and idea conversations are out of scope? [Coverage, Spec §Edge Cases]

## Dependencies and assumptions

- [x] CHK021 - Is the dependency on undocumented CLI behaviour (the print-mode permission path) acknowledged, together with how a future change would be detected? [Assumption, Spec §US3]
- [x] CHK022 - Is SC-003 (a week of use) tied to data the app already records (history `permission-asked`), so it can be checked? [Measurability, Spec §SC-003]

## Notes

- 2026-10-26: all items pass after the spec's "Clarifications" section was added. It
  covers CHK003, CHK004, CHK010, CHK011, CHK014, CHK016 and CHK021. The other items were
  already covered by the FRs, edge cases and assumptions they cite.
