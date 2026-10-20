# Lifecycle & Delete Safety Checklist: Task backlog and deleting tasks

**Purpose**: Unit tests for the requirements on the task lifecycle (backlog → started, any state → deleted), data safety on delete and consistency across sidebar, board, inbox and CLI
**Created**: 2026-10-20
**Feature**: [spec.md](../spec.md)
**Audience / depth**: author, before planning; standard

## Requirement Completeness

- [x] CHK001 - Is the backlog state defined as distinct from a task that is only briefly queued while it starts (so a task mid-start never shows up in the backlog)? [Gap, Spec §Key Entities]
- [x] CHK002 - Are all the transitions out of the backlog listed (only to "started" or "deleted"), and is it stated that a started task can't go back to the backlog? [Completeness, Spec §FR-004]
- [x] CHK003 - Is it specified which task fields can be edited in the backlog (title, notes, project) and whether the key can change (e.g. a free task later linked to a Jira ticket)? [Completeness, Spec §FR-006]
- [x] CHK004 - Is it defined how the notes reach the agent for both workflows (the default workflow and a custom prompt)? [Completeness, Spec §FR-007]
- [x] CHK005 - Does the spec list every trace of a task that a delete removes (history, sessions, pending questions, files) and the ones it keeps (git branch)? [Completeness, Spec §FR-011, §FR-010]
- [x] CHK006 - Are the confirmation texts' contents specified per state (backlog / live agent / unpushed work)? [Gap, Spec §US3]
- [x] CHK007 - Are requirements defined for deleting a task that owns other sessions (e.g. a task converted from a session)? [Gap]

## Requirement Clarity

- [x] CHK008 - Is "unpushed work" defined (commits not on the remote, uncommitted changes, untracked files) and does it match the archive definition? [Clarity, Spec §FR-010]
- [x] CHK009 - Is "behave exactly like a task started from Nueva tarea" made concrete enough to test (same phases, same workspace naming, same branch)? [Clarity, Spec §FR-004, §SC-003]
- [x] CHK010 - Is "oldest first" anchored to the creation time, not to the last edit? [Clarity, Spec §FR-003]
- [x] CHK011 - Is "without a manual reload" bounded with a time (e.g. the same latency as status changes today)? [Measurability, Spec §FR-013]

## Requirement Consistency

- [x] CHK012 - Is FR-002 (project required) consistent with the edge case of a backlog task whose project was removed ("Sin proyecto")? [Consistency, Spec §FR-002, §Edge Cases]
- [x] CHK013 - Is deleting from the checkout (back to the previous branch) consistent with the case where a newer task has already taken that checkout? [Consistency, Spec §US3-4]
- [x] CHK014 - Are the CLI's semantics for "run an existing key" consistent with the Jira view's "Analizar con Claude" for a ticket already in the backlog? [Consistency, Spec §US2-4, §Edge Cases]
- [x] CHK015 - Does archive keep its current meaning, and is it clear to the user how it differs from delete? [Consistency, Spec §Assumptions]
- [x] CHK016 - Is the board's "Backlog" column consistent with the existing columns by spec phase (where does a backlog task with a spec already written go)? [Consistency, Spec §FR-003]

## Scenario & Edge Case Coverage

- [x] CHK017 - Are requirements defined for two starts of the same backlog task at once (two windows, or the CLI and the app)? [Coverage, Gap]
- [x] CHK018 - Is it defined what happens when delete runs while the task is still preparing its workspace? [Coverage, Gap]
- [x] CHK019 - Are the partial-failure outcomes of delete specified (agent stopped but the worktree can't be removed; workspace released but the files can't be removed) and what the user sees? [Exception Flow, Spec §Edge Cases]
- [x] CHK020 - Is the "fully there or fully gone after a restart" rule stated in a way that can be tested? [Measurability, Spec §Edge Cases]
- [x] CHK021 - Is the empty backlog state specified (is the section hidden or shown as "Backlog (0)")? [Coverage, Gap]
- [x] CHK022 - Is it defined whether a deleted task's key can be reused for a free task numbering (LIBRE-n never repeats vs reuses)? [Edge Case, Spec §US3-5]

## UI / CLI Consistency

- [x] CHK023 - Is it specified where "Eliminar" is offered (task view only, or also the sidebar / board context menu)? [Gap, Spec §FR-008]
- [x] CHK024 - Are backlog tasks excluded or included in the sidebar's urgency order and in the inbox counts / Dock badge? [Gap, Spec §FR-003]
- [x] CHK025 - Are the CLI's output and exit codes defined for deleting with unpushed work without the force option? [Gap, Spec §FR-014]
- [x] CHK026 - Is it stated how the CLI list shows backlog tasks (own state name, same as the app)? [Consistency, Spec §FR-014]

## Dependencies & Assumptions

- [x] CHK027 - Is the assumption "the branch is kept on delete" checked against worktree removal (which today may delete the branch)? [Assumption, Spec §FR-010]
- [x] CHK028 - Is it documented that tasks already created with `delilah new` before this feature become backlog tasks? [Assumption, Gap]

## Notes

- All items were resolved in spec.md (FR-006, FR-007, FR-007a, FR-008a, FR-009, FR-011, FR-011a, FR-014, Key Entities, Edge Cases, Assumptions). CHK012: a removed project is the only case where a backlog task has none; "Empezar" asks for one. CHK015: the archive/delete wording is left to the UI copy. CHK027: removing a worktree keeps the branch (checked in `archive_workspace`).
