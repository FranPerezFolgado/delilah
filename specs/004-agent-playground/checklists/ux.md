# Live State & UX Requirements Checklist: Agent playground

**Purpose**: Unit tests for the spec's requirements on live state, lifecycle, accessibility, the separate window, caption privacy and asset licensing, before planning
**Created**: 2026-10-20
**Feature**: [spec.md](../spec.md)
**Audience**: reviewer, before `/speckit-plan`

## Requirement Completeness

- [x] CHK001 - Are the states shown for an agent enumerated, each with its own look? [Completeness, Spec §FR-004]
- [x] CHK002 - Are all action kinds enumerated, including a fallback for unknown actions? [Completeness, Spec §FR-008, US3-4]
- [x] CHK003 - Is the "between actions" (no current action) look defined? [Completeness, Spec §FR-009]
- [x] CHK004 - Is the empty state (no live agent) defined? [Completeness, US1-4]
- [x] CHK005 - Is behaviour defined for agents started before the feature existed, which report no actions or helpers? [Gap → Edge Cases]
- [x] CHK006 - Does the spec define what the separate window shows when there are no agents and while the connection is lost? [Gap → FR-019 "same live content"]
- [x] CHK007 - Does the spec define what a click in the separate window does when the main window is closed or minimised? [Gap → FR-020]

## Requirement Clarity

- [x] CHK008 - Is "live" quantified with a latency bound? [Clarity, Spec §FR-005, SC-003]
- [x] CHK009 - Is the caption's "short prefix" quantified with a length? [Ambiguity → FR-009]
- [x] CHK010 - Is "highlighted" for waiting agents described with more than colour alone? [Clarity, Spec §FR-006, US2]
- [x] CHK011 - Is how the user is told the picture "may be out of date" described (what changes on screen)? [Ambiguity → FR-014]
- [x] CHK012 - Is "supervising" distinguished from "waiting for the user" so the two cannot be confused? [Clarity, US4-5]
- [x] CHK013 - Is "no noticeable lag" in SC-004 measurable? [Measurability → SC-004]

## Requirement Consistency

- [x] CHK014 - Are the phase names the same as the rest of the app? [Consistency, Spec §FR-003]
- [x] CHK015 - Do the clear rules for actions (FR-010) and helpers (FR-018) list the same agent transitions? [Consistency]
- [x] CHK016 - Is the "delegating" wording in US3 consistent with "supervising" in US4? [Consistency, US3, US4]
- [x] CHK017 - Is project grouping (FR-021) consistent with the 1–8 agents layout scenario and the >8 scroll edge case? [Consistency, US5, Edge Cases]
- [x] CHK018 - Is the waiting counter defined to count agents across all projects and offscreen desks? [Consistency, Edge Cases]

## Lifecycle & Scenario Coverage

- [x] CHK019 - Are appear/disappear rules defined for desks, actions and helpers on start, end, fail, wait and pause? [Coverage, Spec §FR-002, FR-010, FR-018]
- [x] CHK020 - Is recovery after a lost connection specified (refresh on reconnect)? [Recovery, Spec §FR-014]
- [x] CHK021 - Are missing end signals (crash mid-action, helper that never reports done) covered? [Exception Flow, Edge Cases]
- [x] CHK022 - Are nested helpers (a helper launching helpers) covered? [Edge Case]
- [x] CHK023 - Is a task with several agent sessions covered? [Edge Case]

## Non-Functional: Accessibility, Motion, Privacy, Licensing

- [x] CHK024 - Are keyboard access requirements defined for every character? [Accessibility, Spec §FR-007]
- [x] CHK025 - Is an accessible name required for each character (task, state, action) for screen readers? [Gap → FR-022]
- [x] CHK026 - Does the reduce-motion requirement also cover helper fades and the waiting highlight? [Gap → FR-011]
- [x] CHK027 - Is pausing animation while hidden specified, including in the separate window? [Spec §FR-012]
- [x] CHK028 - Is the caption content bounded so it never shows more than the terminal already shows? [Privacy, Edge Cases, FR-009]
- [x] CHK029 - Is it explicit that actions and helpers are not stored? [Privacy, Assumptions, Key Entities]
- [x] CHK030 - Is the place where asset sources and licences are recorded defined? [Gap → FR-013]

## Notes

- Gaps found on the first pass (CHK005, 006, 007, 009, 011, 013, 025, 026, 030) were
  fixed in the spec in the same change; all items pass now.
