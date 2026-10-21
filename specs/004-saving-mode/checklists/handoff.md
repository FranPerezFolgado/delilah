# Handoff & Chain Requirements Checklist: Saving mode

**Purpose**: Author self-check of the requirements for session handoff robustness and the clarity of the session chain, before planning
**Created**: 2026-10-21
**Feature**: [spec.md](../spec.md)

## Requirement Completeness

- [x] CHK001 - Are both handoff boundaries defined with the exact conditions that trigger them (files, task checks, turn end)? [Completeness, Spec §FR-004]
- [x] CHK002 - Is it specified which session drives status, questions and limits before, during and after a handoff? [Completeness, Spec §FR-006, §FR-010]
- [x] CHK003 - Are requirements defined for when starting the new session fails (the old one keeps the main role, the user is told)? [Gap → Spec §FR-014]
- [x] CHK004 - Is the user's choice to decline the offer and keep working in the same session specified? [Gap → Spec §FR-005]
- [x] CHK005 - Is changing saving mode on a task already under way covered by a requirement, not only by an edge case? [Completeness, Spec §FR-001, Edge Cases]
- [x] CHK006 - Is the content of each history entry for a handoff defined? [Completeness, Spec §FR-012]

## Requirement Clarity

- [x] CHK007 - Is "main session" defined in terms the user sees (whose turns drive status, questions, limits)? [Clarity, Spec §FR-006]
- [x] CHK008 - Is it clear that confirming the planning → implementation handoff also approves the tasks? [Clarity, Spec §FR-005]
- [x] CHK009 - Are the three chain steps named, ordered and tied to the phase where each starts? [Clarity, Spec §Key Entities]
- [x] CHK010 - Is "the review session commits" bounded (no push, no PR)? [Clarity, Spec §US-4]

## Requirement Consistency

- [x] CHK011 - Do the forward-only phase rules (decision 17) and the handoff triggers agree when the user moves the phase by hand? [Consistency, Edge Cases]
- [x] CHK012 - Is the automatic review handoff consistent with "the user decides" — is the user told it happened? [Gap → Spec §FR-015]
- [x] CHK013 - Is FR-003 (first session told to stop after tasks) consistent with the edge case where it implements anyway? [Consistency, Edge Cases]

## Scenario & Edge Case Coverage

- [x] CHK014 - Is a usage-limit pause during the save-notes turn or the new session's start addressed, including what resumes after the reset? [Coverage, Edge Cases]
- [x] CHK015 - Is a daemon restart mid-handoff addressed with a defined end state? [Recovery, Edge Cases, Spec §FR-011]
- [x] CHK016 - Is the invariant "exactly one main session per running task" stated? [Coverage, Spec §FR-011]
- [x] CHK017 - Is reopening a finished session defined so it cannot take back the main role? [Coverage, Edge Cases]
- [x] CHK018 - Is crossing two boundaries before the user acts addressed? [Edge Case, Edge Cases]
- [x] CHK019 - Is the save-notes step's failure or question addressed? [Exception Flow, Spec §US-3]

## UX of the Chain

- [x] CHK020 - Are the states of each chain step (pending, active, done) and their meaning specified? [Clarity, Spec §FR-009]
- [x] CHK021 - Is it specified which terminal is shown by default after a handoff? [Completeness, Spec §US-2]
- [x] CHK022 - Is a waiting handoff (offer pending, save-notes in progress) visible in the chain, not only in the Bandeja? [Gap → Spec §FR-009]
- [x] CHK023 - Is SC-003 ("can say which session is working and what comes next") verifiable without implementation knowledge? [Measurability, Spec §SC-003]

## Dependencies & Assumptions

- [x] CHK024 - Is the dependency on the spec folder being on the branch (committed or at least in the workspace) stated? [Assumption]
- [x] CHK025 - Is the relaxation of decision 15 recorded as an assumption that needs a decision entry? [Assumption]

## Notes

- CHK003, CHK004, CHK012 and CHK022 were gaps; the spec gained FR-014, FR-015, the decline option in FR-005 and the waiting state in FR-009.
