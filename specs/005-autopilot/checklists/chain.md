# Chain, Pause & Preview Requirements Checklist: Autopilot

**Purpose**: Author self-check of the requirements for the automatic chain, its pauses and resume, opening the PR exactly once, automatic answers, and the daemon-owned preview, before planning
**Created**: 2026-10-22
**Feature**: [spec.md](../spec.md)

## Requirement Completeness

- [x] CHK001 - Is the end of every step defined by something Delilah can read (tasks written, all tasks checked, review outcome), not by the agent's prose? [Completeness, Spec §FR-005–FR-008]
- [x] CHK002 - Is each automatic handoff (planning → implementation, implementation → review, review → preview/PR) specified with what replaces today's user confirmation? [Completeness, Spec §FR-006, §FR-009]
- [x] CHK003 - Is every pause cause listed in one place, including the session waiting on something other than an AskUserQuestion (a permission prompt)? [Gap → Spec §FR-013]
- [x] CHK004 - Is the review ending *ready* with uncommitted changes covered? [Gap → Spec §FR-013, Edge Cases]
- [x] CHK005 - Is what each pause leaves behind specified (Bandeja item, notification, history entry, the task's phase and session untouched)? [Completeness, Spec §FR-012–FR-014]
- [x] CHK006 - Are the actions on a paused autopilot defined ("Reanudar autopiloto", "Seguir a mano") and what each does? [Completeness, Spec §FR-014]
- [x] CHK007 - Is the end of autopilot defined (PR linked) and what is explicitly not automated afterwards (PR loop comments, CI)? [Completeness, Spec §FR-011]
- [x] CHK008 - Is the full preview lifecycle specified: start, URL, failure, manual start/stop, stop with the task, daemon restart? [Completeness, Spec §FR-018–FR-020a, Edge Cases]

## Requirement Clarity

- [x] CHK009 - Is "the option the agent marked as recommended" defined precisely enough to decide it mechanically (label marker, both languages, one per question)? [Gap → Spec §FR-013a]
- [x] CHK010 - Is the behaviour for multi-question and multi-select AskUserQuestion calls specified? [Gap → Spec §FR-013a]
- [x] CHK011 - Is "tests failing after N attempts" tied to an observable outcome rather than a count Delilah keeps? [Clarity, Spec §Assumptions, §FR-013]
- [x] CHK012 - Is how Delilah learns a preview's URL, and when a preview counts as failed (timeout), quantified? [Gap → Spec §FR-018a]
- [x] CHK013 - Is it clear that autopilot never approves permission prompts? [Clarity, Spec §Edge Cases]
- [x] CHK014 - Is it clear which tasks can have autopilot (spec-workflow tasks, not project sessions or idea conversations)? [Gap → Spec §FR-001]

## Requirement Consistency

- [x] CHK015 - Are saving mode and autopilot consistent when either is toggled (autopilot implies saving mode; saving mode off turns autopilot off)? [Consistency, Spec §FR-002, US3]
- [x] CHK016 - Is the usage limit consistently excluded from pause causes in US2, FR-013 and FR-017? [Consistency]
- [x] CHK017 - Do the preview requirements agree on ownership (Delilah, not the agent) across US4, FR-018 and the Clarifications? [Consistency]
- [x] CHK018 - Is the draft PR (FR-010) consistent with "the user marks it ready after trying the preview" and the PR loop? [Consistency, Spec §FR-010, §FR-011]

## Scenario Coverage

- [x] CHK019 - Is resuming while the step's session is still working specified? [Gap → Spec §FR-016]
- [x] CHK020 - Is turning autopilot on for a task already past planning, or with a pending offer, covered? [Coverage, Spec §US3.2, Edge Cases]
- [x] CHK021 - Are parent workspaces (one PR per repo, partial failure) covered? [Coverage, Spec §Edge Cases]
- [x] CHK022 - Is a daemon restart in the middle of opening the PR covered so the PR is never opened twice? [Coverage, Spec §Edge Cases, §SC-003]
- [x] CHK023 - Is the user typing into an autopilot session covered? [Coverage, Spec §Edge Cases]

## Acceptance Criteria Quality

- [x] CHK024 - Can SC-002 ("PR or a visible pause, never stalled") be checked from the task without reading sessions? [Measurability, Spec §SC-002]
- [x] CHK025 - Is SC-005's "9 of 10 runs" tied to a defined failure condition for the preview? [Measurability, Spec §SC-005, §FR-018a]

## Dependencies & Assumptions

- [x] CHK026 - Is it stated that the non-interactive behaviour comes from the step instructions, not from editing the user's skills? [Assumption]
- [x] CHK027 - Is the absence of time/token caps recorded as a deliberate choice, with what bounds a run instead? [Assumption]
- [x] CHK028 - Is the relation to decisions #15 and #33 stated (Delilah still does not drive what happens inside a step)? [Assumption, Spec §Why]
- [x] CHK029 - Is the independence from LIBRE-7 justified with how the feature is tested instead? [Assumption]

## Notes

- CHK003, CHK004, CHK009, CHK010, CHK012, CHK014 and CHK019 were gaps; the spec gained FR-013a, FR-018a, the extra pause causes in FR-013, the scope sentence in FR-001, the resume-while-running sentence in FR-016 and two edge cases (permission prompt, uncommitted changes).
