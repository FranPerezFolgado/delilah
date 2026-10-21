# UX Requirements Checklist: Task phase in the task screen header

**Purpose**: Check the spec's UX requirements for the header phase badge are complete and unambiguous before planning (reviewer-facing, light depth)
**Created**: 2026-10-20
**Feature**: [spec.md](../spec.md)

## Requirement Completeness

- [ ] CHK001 - Is the position of the phase relative to the status pill and the file-tree toggle specified (before or after the status)? [Completeness, Spec §FR-001]
- [ ] CHK002 - Does the spec say whether the phase badge carries an icon, text, or both? [Gap, Spec §FR-006]
- [ ] CHK003 - Is the exact format of the progress count defined (separator, "4/12" vs "4 de 12")? [Clarity, Spec §FR-005]
- [ ] CHK004 - Is it stated that the header badge is display-only (no click to change phase)? [Completeness, Spec §Assumptions]

## Requirement Clarity

- [ ] CHK005 - Is "visually distinct from the status" defined with a concrete property (shape, colour, weight)? [Ambiguity, Spec §FR-006]
- [ ] CHK006 - Is "the same rule the board and the spec panel use" named precisely enough to be the single source for the phase? [Clarity, Spec §FR-002]
- [ ] CHK007 - Is "no spec information" defined (no workspace, archived, spec state not loaded yet)? [Clarity, Spec §FR-004]

## Requirement Consistency

- [ ] CHK008 - Are the visibility rules for the header consistent with when the spec panel shows its stepper? [Consistency, Spec §Edge Cases]
- [ ] CHK009 - Is the progress count in the header consistent with the board card's progress (same numbers, same condition)? [Consistency, Spec §FR-005]
- [ ] CHK010 - Do the phase names match the board column labels and their translations? [Consistency, Spec §FR-001, §FR-008]

## Scenario & Edge Case Coverage

- [ ] CHK011 - Are failed and paused-by-limit tasks covered (is the phase shown alongside those statuses)? [Coverage, Gap]
- [ ] CHK012 - Is the behaviour defined while spec information is still loading on first open? [Edge Case, Gap]
- [ ] CHK013 - Is the narrow-window priority order (title truncates first, then what?) fully specified? [Edge Case, Spec §FR-007]

## Acceptance Criteria Quality

- [ ] CHK014 - Is the "minimum window width" in SC-004 tied to a stated value? [Measurability, Spec §SC-004]
- [ ] CHK015 - Can SC-003 ("same time as the board") be verified without a stated refresh interval? [Measurability, Spec §SC-003]

## Non-Functional

- [ ] CHK016 - Are accessibility requirements stated for the badge (accessible name / tooltip readable by screen readers)? [Gap]

## Notes

- Resolve or accept each item during planning; gaps that are deliberate get a line in plan.md.
