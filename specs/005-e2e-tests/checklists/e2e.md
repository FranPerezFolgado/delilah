# E2E Test Infrastructure Requirements Checklist: End-to-end tests

**Purpose**: Author self-review of the spec's requirements quality before planning — isolation and safety, fake Claude fidelity, determinism, flow coverage
**Created**: 2026-10-22
**Feature**: [spec.md](../spec.md)

## Isolation & Safety

- [x] CHK001 - Is it specified that tests never use the installed daemon's port or home? [Completeness, Spec §FR-008]
- [x] CHK002 - Is it specified how processes are stopped (own handle, never by name)? [Clarity, Spec §FR-008, US4-4]
- [x] CHK003 - Are requirements defined for two e2e runs at the same time? [Coverage, Spec §FR-008, Edge Cases]
- [x] CHK004 - Is it specified that real `gh`/Jira credentials on the machine are never used? [Completeness, Spec §FR-009, Edge Cases]
- [x] CHK005 - Is "zero real Claude sessions" verifiable, i.e. does the spec say how the suite detects one? [Measurability, Spec §SC-002, §FR-006a]
- [x] CHK006 - Are cleanup requirements defined for failed and interrupted runs, not only passing ones? [Coverage, Spec §US4-4, §SC-006]
- [x] CHK007 - Is keeping temp state for debugging defined as opt-in? [Clarity, Spec §US4-4]
- [x] CHK008 - Is the fake Claude forbidden from reading the user's `~/.claude`? [Completeness, Spec §FR-006]

## Fake Claude Fidelity

- [x] CHK009 - Are the arguments the fake must accept enumerated (new/resume, session id, settings, added folders, prompt)? [Completeness, Spec §FR-002]
- [x] CHK010 - Are the lifecycle signals the fake must emit enumerated? [Completeness, Spec §US1, §FR-003, §FR-004]
- [x] CHK011 - Is the set of scripted steps closed and named, covering the ask confirmation card used by task-from-idea? [Clarity, Spec §FR-004]
- [x] CHK012 - Is the terminal-output fidelity bounded (hooks + text + trust dialog, no replay)? [Clarity, Spec §FR-007, Clarifications]
- [x] CHK013 - Is resume behaviour required of the fake? [Coverage, Spec §US1-4]
- [x] CHK014 - Is the failure path (script fails, path missing, not executable) specified? [Coverage, Spec §US1-5, Edge Cases]
- [x] CHK015 - Is the status-line hook explicitly in or out of scope? [Gap → resolved, Spec §FR-006b]
- [x] CHK016 - Is drift between the fake and the real Claude addressed? [Assumption, Spec §US5]
- [x] CHK017 - Is the no-fake default behaviour (unchanged, launches `claude`) stated? [Consistency, Spec §FR-001, §US1-6]

## Determinism

- [x] CHK018 - Are waits quantified rather than "bounded"? [Clarity, Spec §FR-012]
- [x] CHK019 - Is reliance on fixed sleeps ruled out? [Clarity, Spec §FR-012]
- [x] CHK020 - Is flakiness measurable? [Measurability, Spec §SC-005]
- [x] CHK021 - Is per-test isolation of fixture repos required so tests can't leak state into each other? [Coverage, Spec §FR-009, Edge Cases]
- [x] CHK022 - Is what a failing test must leave behind specified (daemon log; UI screenshot and trace)? [Completeness, Spec §FR-015, Edge Cases]

## Flow Coverage

- [x] CHK023 - Is the first batch of API/CLI flows enumerated and counted? [Completeness, Spec §US2, §FR-010]
- [x] CHK024 - Is the first batch of UI flows enumerated and counted? [Completeness, Spec §US3, §FR-014]
- [x] CHK025 - Are counts consistent between stories, FRs and SC-001 (11 + 6 = 17)? [Consistency, Spec §FR-010, §FR-014, §SC-001]
- [x] CHK026 - Is CLI coverage required, not only HTTP? [Coverage, Spec §FR-011]
- [x] CHK027 - Is on-disk state (worktrees, branches, folders) part of what API flows must check? [Completeness, Spec §FR-010]
- [x] CHK028 - Are exception flows covered (session failure, daemon crash, UI server can't start)? [Coverage, Spec §US1-5, Edge Cases]
- [x] CHK029 - Is recovery covered (state survives a daemon restart)? [Coverage, Spec §US2-11]
- [x] CHK030 - Are out-of-scope areas stated (Tauri native, CI, usage display)? [Scope, Spec §FR-019, §FR-020, §FR-006b]

## Non-Functional & Assumptions

- [x] CHK031 - Are run-time targets quantified per layer? [Measurability, Spec §SC-004]
- [x] CHK032 - Is it stated that e2e tests don't slow down the normal per-crate test commands? [Assumption, Spec §Assumptions]
- [x] CHK033 - Are dependencies on existing fakes and fixtures (fixture script, fake `gh`, Jira sample) documented? [Dependency, Spec §Assumptions]

## Notes

- Self-review 2026-10-22. Gaps found and fixed in the spec in the same pass: CHK005
  (FR-006a), CHK011 (confirmation card in FR-004), CHK003 (parallel runs in FR-008),
  CHK015 (FR-006b), CHK018–CHK019 (30 s cap, no fixed sleeps in FR-012).
