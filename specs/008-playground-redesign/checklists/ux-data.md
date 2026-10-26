# Requirements Checklist: Playground redesign (UX, activity history, tokens, accessibility)

**Purpose**: Unit tests for the requirements of spec 008 before planning
**Created**: 2026-10-26
**Depth**: Standard · **Audience**: PR reviewer · **Feature**: [spec.md](../spec.md)

## Requirement Completeness

- [x] CHK001 - Are the contents of a task floor (lead desks, helper desks, lounge) specified? [Completeness, Spec §FR-002, Key Entities]
- [x] CHK002 - Is every desk state (kind colour, waiting, paused/idle, none) given a visible signal? [Completeness, Spec §FR-003]
- [x] CHK003 - Are the contents of a building floor (key, title, phase, miniatures, waiting count) specified? [Completeness, Spec §FR-009]
- [x] CHK004 - Are the events kept in the history and their attributes listed? [Completeness, Spec §FR-013, Key Entities]
- [x] CHK005 - Are the token kinds counted (input, output, cache) stated? [Completeness, Spec §FR-025]
- [x] CHK006 - Are the details shown for a selected node/entry listed? [Completeness, Spec §FR-016]
- [ ] CHK007 - Is the arrangement of desks on a floor (pods, how it grows with many helpers) specified beyond "grows with more desk pods"? [Gap, Spec §Edge Cases] — left to the plan (layout is a visual choice)

## Requirement Clarity

- [x] CHK008 - Is the helper's stay in the lounge quantified? [Clarity, Spec §FR-005] — about 5 seconds
- [x] CHK009 - Is the miniature's "first ones" quantified? [Clarity, Spec §Edge Cases] — 6 and "+N"
- [x] CHK010 - Are the paper stack tiers quantified? [Clarity, Spec §FR-026]
- [x] CHK011 - Is token freshness quantified? [Clarity, Spec §FR-025] — within 10 seconds
- [x] CHK012 - Is the retention of ended sessions' history bounded with a number? [Clarity, Spec §Assumptions] — 50 sessions
- [x] CHK013 - Is "idle" (for `zzz`) distinguishable from "waiting for the user" in how sessions end a turn? [Ambiguity, Spec §FR-003, Assumptions] — resolved: no idle state; a finished turn is waiting (`?`), `zzz` only for paused
- [x] CHK014 - Is the history limit stated as a number? [Clarity, Spec §FR-013] — 200 per session

## Requirement Consistency

- [x] CHK015 - Is the Oficina/Flujo choice remembered the same way in US3 and FR-012? [Consistency, Spec §US3-6, FR-012] — fixed: one choice per device
- [x] CHK016 - Do floor-click requirements agree between US2, FR-011 and the clarification? [Consistency, Spec §FR-011]
- [x] CHK017 - Do monitor colours (office) and node colours (Flujo) use one set of kinds? [Consistency, Spec §FR-017]
- [x] CHK018 - Are caption limits the same for captions, nodes and details? [Consistency, Spec §FR-023]

## Acceptance Criteria Quality

- [x] CHK019 - Are latency targets measurable for office, building and Flujo? [Measurability, Spec §SC-003]
- [x] CHK020 - Is SC-006 (look preference) acknowledged as a subjective user check? [Measurability, Spec §SC-006]
- [x] CHK021 - Does each user story have an independent test? [Acceptance Criteria]

## Scenario & Edge Case Coverage

- [x] CHK022 - Are empty states defined for the floor, the building and Flujo? [Coverage, Spec §US1-6, US2-5, FR-018]
- [x] CHK023 - Are daemon restart and connection loss covered for the history? [Recovery, Spec §Edge Cases]
- [x] CHK024 - Are agents without action hooks or readable usage covered? [Exception Flow, Spec §Edge Cases]
- [x] CHK025 - Are parallel tool calls and nested helpers covered in Flujo? [Coverage, Spec §US3-3, Edge Cases]
- [x] CHK026 - Are project-level sessions (no task) covered? [Coverage, Spec §Edge Cases, FR-001]

## Non-Functional (accessibility, motion, privacy)

- [x] CHK027 - Are accessible names required for characters, floors and nodes? [Accessibility, Spec §FR-020]
- [x] CHK028 - Is a keyboard-reachable alternative to the graph specified? [Accessibility, Spec §FR-020]
- [x] CHK029 - Is colour-only signalling ruled out? [Accessibility, Spec §FR-027]
- [x] CHK030 - Is reduce motion defined for walking (appear at destination)? [Accessibility, Spec §FR-021]
- [x] CHK031 - Is exposure of command/file content bounded? [Privacy, Spec §FR-023]
- [ ] CHK032 - Is a bound on graph size (nodes on screen at once) specified for long sessions with helpers? [Gap, Performance] — bounded indirectly by 200 events per session; plan checks it

## Dependencies & Assumptions

- [x] CHK033 - Is the source of token counts (agent's own records) stated, with the fallback when unreadable? [Assumption, Spec §Assumptions, Edge Cases]
- [x] CHK034 - Is the art licence and credits requirement explicit? [Dependency, Spec §FR-019]
- [x] CHK035 - Is the revision of decision #80 recorded? [Assumption, Spec §Assumptions]
