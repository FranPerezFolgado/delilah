# Requirements Checklist: Conversation view, safety boundary, consistency

**Purpose**: Test the quality of the spec's requirements for the idea conversation's
view, who may create tasks, and how the result fits existing task creation.
**Created**: 2026-10-21
**Feature**: [spec.md](../spec.md)
**Audience**: PR reviewer; resolve or accept each item during planning.

## Requirement Completeness

- [ ] CHK001 - Does the spec say what the conversation view shows before the first message (empty state, a hint of what to write)? [Gap, Spec §FR-002]
- [ ] CHK002 - Is "what Claude is doing while it works" defined: which activity is shown and how much (last action only, or a running list)? [Clarity, Spec §FR-002]
- [ ] CHK003 - Are the states of the conversation listed (working, waiting for the user, waiting on a card, paused by limit, stopped, ended) with what the view shows for each? [Completeness, Gap]
- [ ] CHK004 - Does the spec say how queued messages look and whether they can be edited or removed before Claude reads them? [Gap, Spec §FR-002a]
- [ ] CHK005 - Is it defined whether queued messages are delivered one by one or together after the turn? [Ambiguity, Spec §FR-002a]
- [ ] CHK006 - Is the effect of "Detener" on a pending question card defined (card withdrawn, kept, answered as dismissed)? [Gap, Spec §FR-002a]
- [ ] CHK007 - Does the spec define how the confirmation card differs from an ordinary question card (title and brief prominent, fixed options)? [Clarity, Spec §FR-003, §FR-008]
- [ ] CHK008 - Is the naming of an idea conversation in the sidebar defined (default label, renamed after the first idea or the created task)? [Gap, Spec §FR-016]
- [ ] CHK009 - Are close, archive or delete requirements for idea conversations stated, given they are kept forever? [Gap, Spec §FR-016]
- [ ] CHK010 - Is the link from the conversation to the tasks it created specified (key shown in the reply; is it clickable)? [Completeness, Spec §FR-011]

## Requirement Clarity

- [ ] CHK011 - Is "read the code without editing it" bounded: may Claude run commands (tests, `git log`), or only read and search? [Ambiguity, Spec §FR-005]
- [ ] CHK012 - Is "MUST NOT depend on terminal-only prompts" stated as an outcome that can be checked? [Measurability, Spec §FR-005]
- [ ] CHK013 - Is "too long to pass inline" for the brief quantified, or deliberately left to the plan? [Clarity, Spec §Edge Cases]
- [ ] CHK014 - Is "marked as written by Claude from a conversation" defined (where the mark appears and whether it survives edits)? [Clarity, Spec §US2]
- [ ] CHK015 - Is "longer detail" on question cards defined (Markdown? size limit? collapsed or open)? [Clarity, Spec §FR-003]

## Safety Boundary

- [ ] CHK016 - Is it stated whether a task can be created by anything other than "create" on the card, e.g. the user typing "sí" in the text field? [Ambiguity, Spec §FR-008, SC-004]
- [ ] CHK017 - Does the spec say what happens if the same confirmation is answered twice (double click, answer from the Bandeja and the view)? [Edge Case, Gap]
- [ ] CHK018 - Is the restriction "agents inside tasks MUST NOT create tasks" also stated for extra Claude sessions in a task, and for a project session after "Permitir cambios"? [Coverage, Spec §FR-014]
- [ ] CHK019 - Is it defined whether a project session turned into a task ("Convertir en tarea") loses the ability to create tasks? [Consistency, Spec §FR-014, §FR-017]
- [ ] CHK020 - Is the threat assumption explicit: local-only daemon, no login, so any local process could create tasks the same way as today? [Assumption, Spec §Assumptions]

## Consistency with Existing Task Creation

- [ ] CHK021 - Does "project's default place to work" match the dialog's default (checkout), including parent projects with submodules? [Consistency, Spec §FR-009]
- [ ] CHK022 - Is the task title's validation the same as the dialog's (length, empty), and what if Claude proposes an invalid one? [Consistency, Gap]
- [ ] CHK023 - Is it consistent that a project session can already turn into a task via "Convertir en tarea" and now also create other tasks, and does the spec tell the user the difference? [Consistency, Spec §US3]
- [ ] CHK024 - Is the history entry of FR-015 defined (text, and which conversation it points to)? [Clarity, Spec §FR-015]
- [ ] CHK025 - Do the question cards in the conversation view and the Bandeja stay in sync: does an idea conversation's card also appear in the Bandeja? [Consistency, Gap]

## Acceptance Criteria Quality

- [ ] CHK026 - Can SC-002 ("without seeing a terminal or a form") be verified when the fallback "Ver terminal" exists? [Measurability, Spec §SC-002]
- [ ] CHK027 - Is SC-001's 5 seconds measured from the click or from Claude's reply? [Clarity, Spec §SC-001]

## Edge Cases & Recovery

- [ ] CHK028 - Are requirements defined for the Claude process dying mid-conversation (crash, Claude Code update), beyond the restart case? [Recovery, Gap]
- [ ] CHK029 - Is behaviour defined when the project is removed while its idea conversations exist? [Edge Case, Spec §Edge Cases]
- [ ] CHK030 - Is behaviour defined when the user interacts in "Ver terminal" (types there, answers a question there): does the history in the view reflect it? [Consistency, Spec §FR-004, §FR-016]

## Notes

- Resolve or accept each item during planning; gaps that are deliberate get a line in plan.md.
