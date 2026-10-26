# Requirements Quality Checklist: Chat view and chat mode

**Purpose**: Check that the spec's requirements on transcript robustness, the daemon ↔ app contract, the two runtimes and chat-mode permissions are complete and unambiguous before planning.
**Created**: 2026-10-23
**Feature**: [spec.md](../spec.md)
**Depth / audience**: standard, the author, before `/speckit-plan`.

## Transcript robustness

- [x] CHK001 Is it stated that the transcript is used only for display and never for task state? [Clarity, Spec §FR-005]
- [x] CHK002 Are unknown line types, unknown content types and unknown fields each covered as "skip, never fatal"? [Completeness, Spec §FR-005, Edge Cases]
- [x] CHK003 Is the behaviour for a half-written line (Claude still writing) distinguished from a line that never parses? [Clarity, Edge Cases]
- [x] CHK004 Is it specified which transcript content is hidden (thinking, hook output, meta, Delilah's own instructions) and which shows as a marker (compaction)? [Completeness, Edge Cases, Assumptions]
- [x] CHK005 Is the empty state defined both for "no transcript yet" and "transcript gone for good"? [Coverage, Edge Cases]
- [x] CHK006 Is the history requirement explicit about restarts, `--resume` and sessions that predate the feature? [Completeness, Spec §FR-006]
- [x] CHK007 Is there a measurable criterion for robustness against a newer Claude format? [Measurability, Spec §SC-004]
- [x] CHK008 Is the latency of the PTY-mode view quantified? [Measurability, Spec §SC-001]
- [x] CHK009 Is the scale the view must handle quantified? [Measurability, Spec §SC-003, §FR-013]
- [x] CHK010 Is it specified how messages typed directly in the terminal appear in the conversation? [Coverage, Assumptions]

## Daemon ↔ app contract

- [x] CHK011 Is it stated that the idea conversation's existing behaviour (cards, Crear, history) must keep working, i.e. that changes to the conversation contract stay backwards compatible? [Consistency, Spec §FR-011]
- [x] CHK012 Are the user-visible refusals for terminal/attach in chat mode required on all three surfaces (app, CLI, API)? [Completeness, Spec §FR-019]
- [x] CHK013 Is it required that live text is not persisted, so the stored entries are the same in both runtimes? [Consistency, Spec §FR-016]
- [x] CHK014 Is the sync between inline cards and Bandeja items required in both directions, for question cards and for permission cards? [Consistency, Spec §FR-010, §FR-020]
- [x] CHK015 Is it specified whether the per-task view setting and the chat-mode flag are visible and settable from the CLI as well as the app? [Gap → resolved: chat mode in `delilah new`/`run` (FR-015); the view setting is app-only, a display preference]

## Runtimes and fallback

- [x] CHK016 Is the set of task behaviours that must be unchanged in chat mode enumerated (states, phases, Bandeja, limits, saving mode, autopilot, previews, PR)? [Completeness, Spec §FR-017]
- [x] CHK017 Is the switch between runtimes constrained to "between turns", with the mid-turn behaviour defined? [Clarity, Spec §FR-021, US4]
- [x] CHK018 Is recovery defined for a chat-mode process that dies mid-turn, and for a daemon restart? [Coverage, Edge Cases, Spec §FR-022]
- [x] CHK019 Is the behaviour defined when the installed Claude cannot run chat mode? [Coverage, Spec §FR-023]
- [x] CHK020 Is it clear which sessions chat mode applies to (task sessions, all steps) and which it does not (project sessions, idea conversations)? [Clarity, Assumptions]
- [x] CHK021 Is the gate between the spike and chat mode explicit, including what happens if (a)–(c) fail? [Dependencies, Spec §FR-014, US5]
- [x] CHK022 Is "Detener" defined in chat mode as leaving the session usable, with a visible marker? [Clarity, Spec §FR-018, US3 #5]
- [x] CHK023 Is it stated that turning chat mode off everywhere leaves the product as before, apart from the chat view? [Measurability, Spec §SC-008]

## Chat-mode permissions

- [x] CHK024 Are the card's answers enumerated, with the scope of "always" defined (tool, task, across restarts)? [Clarity, Spec §FR-020, Clarifications]
- [x] CHK025 Is the scope of "always" for shell commands defined separately from other tools? [Edge Cases]
- [x] CHK026 Is the fallback (allow list) and the condition that triggers it stated? [Completeness, Spec §FR-020, Clarifications]
- [x] CHK027 Is the behaviour of an unanswered permission card defined (task waits, Bandeja item open, switch refused)? [Coverage, Edge Cases]
- [x] CHK028 Is it specified whether an autopilot task in chat mode answers permission cards by itself, or pauses into the Bandeja? [Gap → resolved below]
- [x] CHK029 Is folder trust in chat mode covered, so that a task never waits on an invisible dialog? [Coverage, Edge Cases]

## Notes

- CHK028 was a gap: autopilot answers question cards with the recommended option, but a
  permission has no "recommended" answer. Resolved in the spec (Edge Cases): an autopilot
  task in chat mode pauses into the Bandeja on a permission card, as it does on a question
  with no recommended option.
