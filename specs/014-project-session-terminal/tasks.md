# Tasks: Project Claude sessions open in the terminal

**Input**: Design documents from `specs/014-project-session-terminal/`
**Prerequisites**: plan.md, spec.md, research.md, data-model.md, contracts/session-view-switch.md, quickstart.md

**Tests**: requested (test-first). Write each test task before its implementation task, and run it to
see it fail first.

**Checks**: `cd app && npx tsc --noEmit && npx vitest related --run src/components/SessionViewSwitch.tsx src/components/TaskScreen.tsx`

## Format: `[ID] [P?] [Story] Description`

- **[P]**: can run in parallel (different files, no dependency on an incomplete task)
- **[Story]**: US1 (opens in the terminal), US2 (view-only switch on projects), US3 (tasks, idea
  conversations and shells unchanged)

---

## Phase 1: Setup

None. No new dependencies, files or configuration: the change sits in existing components.

---

## Phase 2: Foundational: the switch's explicit modes (blocks US2)

**Purpose**: `SessionViewSwitch` gets the discriminated-union props from
`contracts/session-view-switch.md`, so `TaskScreen` can render the view-only mode on projects.

- [ ] T001 Update the runtime-mode tests in app/src/components/SessionViewSwitch.test.tsx: remove the `view` and `chooseView` props from every existing render (L22, 45, 56, 63, 68, 76) and keep their assertions. Also drop the `chooseView` spy and its `not.toHaveBeenCalled` check at L19/L26, since the runtime mode no longer takes it. They must still pass against today's component.
- [ ] T002 Add failing view-only tests in app/src/components/SessionViewSwitch.test.tsx. Render `<SessionViewSwitch mode="view" view="terminal" chooseView={spy} />`:
  (a) Terminal shows as pressed;
  (b) clicking Chat calls `spy("chat")` once, shows no `ConfirmDialog`, and `api.setSessionRuntime` is not called;
  (c) clicking the pressed Terminal does not call `spy`;
  (d) with `view="chat"`, Chat is pressed and clicking Terminal calls `spy("terminal")`;
  (e) the options are never disabled (the view-only props have no `busy`).
- [ ] T003 Implement the props union in app/src/components/SessionViewSwitch.tsx: `{ mode: "view"; view; chooseView } | { mode?: "runtime"; session; busy?; onChanged?; onError? }`. Remove the deprecated `view`/`chooseView` from the runtime arm. Render one shared `Segmented` (same label "Vista de la sesión", same options and beta tag, same `.svs` wrapper). In view mode: `value = view`, and `onChange` calls `chooseView(next)` when `next !== view`, with no dialog and no disabled options. Runtime mode stays exactly as today. Update the doc comments as in the contract. T001 and T002 must pass, and `npx tsc --noEmit` must be clean (the existing task caller in TaskScreen.tsx type-checks against the runtime arm).

**Checkpoint**: the switch supports both modes, and task screens behave as before.

---

## Phase 3: User Story 1: a project session opens in the terminal (Priority: P1) 🎯 MVP

**Goal**: a project's Claude sessions (not idea conversations) show the terminal by default.

**Independent Test**: open a project session. `terminal` is shown and `conversation` is not.

- [X] T004 [US1] In app/src/components/TaskScreen.test.tsx (project block, ~L295), replace "opens a project session in chat even when it is not an idea conversation" with "opens a project session in the terminal": `open({ ...conv, conversation: false })` shows `terminal` and not `conversation`. Add a case for a read-only session (`read_only: true`) and one for a session that is not live (`live: false`): both show `terminal`, and the not-live one also shows the "Reabrir" bar. These tests must fail first.
- [X] T005 [US1] In app/src/components/TaskScreen.tsx `viewOf` (~L197), change the project fallback to `views.get(s.id) ?? (s.conversation ? "chat" : "terminal")`. Comment the `s.runtime === "stream"` branch as defensive (the daemon never runs a project session on a stream, see `run.rs::runtime_kind`). Update the comment above `views` (~L191) that says a project's sessions open in chat. T004 passes, and the existing "shows the conversation view…" test for an idea conversation still passes.

**Checkpoint**: the reported bug is fixed. This slice is shippable on its own.

---

## Phase 4: User Story 2: switch a project session between Terminal and Chat (Priority: P1)

**Goal**: the Chat | Terminal switch shows in the project header and only changes the view.

**Independent Test**: on a project session, Chat, then Terminal, then Chat each flip the view. No
daemon call is made and no dialog appears.

**Depends on**: T003 (view mode) and T005 (default view).

- [ ] T006 [US2] In app/src/components/TaskScreen.test.tsx (project block, ~L289), replace "offers no Chat / Terminal switch on a project conversation" with tests that must fail first:
  (a) a project session (`conversation: false`) shows `.svs` with Terminal pressed; clicking Chat shows `conversation`; clicking Terminal shows `terminal` again;
  (b) through all of it, `api.setSessionRuntime` and `api.updateTask` are never called, and no confirm dialog appears;
  (c) with the session in `state: "working"`, the switch options are enabled and clicking Chat flips the view;
  (d) with two project sessions, picking Chat on the first leaves the second in `terminal` when its tab is selected;
  (e) the chat's own "open the terminal" button (`onOpenTerminal` from ConversationView; use the mocked component's hook or a session state that renders the button) leaves Terminal pressed in the switch.
- [ ] T007 [US2] In app/src/components/TaskScreen.tsx `sessionSwitch` (~L668), render `<SessionViewSwitch mode="view" view={viewOf(shownClaude)} chooseView={(v) => chooseView(shownClaude, v)} />` when `project` is set, and today's runtime switch otherwise. Replace the comment "Not for a project's sessions: the daemon refuses to change their runtime" with one saying that on a project the switch only changes the view. In `chooseView` (~L633), call `rememberView(v)` only when `!project`, and update its comment "(the switch is not offered there)". T006 passes.

**Checkpoint**: US1 and US2 both work. Project sessions open in the terminal and flip freely.

---

## Phase 5: User Story 3: tasks, idea conversations and shells keep their behaviour (Priority: P2)

**Goal**: regression guards for what must not change.

**Independent Test**: the task runtime switch still confirms, an idea conversation opens in chat with a
switch, and a shell has no switch.

- [ ] T008 [US3] In app/src/components/TaskScreen.test.tsx, add "an idea conversation opens in chat and offers the view-only switch": `open(conv)` shows `conversation` and `.svs` with Chat pressed, and clicking Terminal shows `terminal` with no `setSessionRuntime` call. Keep "always shows the terminal for a shell session, without the switch" and "shows a task session in the view of its runtime" unchanged. Add to the task block: on a task Claude session, clicking the other option of the switch opens the confirm dialog (runtime mode), as a regression guard.
- [ ] T009 [US3] Run the full UI suite (`cd app && npx tsc --noEmit && npx vitest run`) and fix any test that assumed project sessions open in chat. Search app/src for "open in chat" or `?? "chat"` and for tests asserting no `.svs` on projects.

---

## Phase 6: Polish & cross-cutting

- [X] T010 [P] Update docs/usage.md. Near L239 ("Project sessions and idea conversations always use the terminal"): project sessions run in a terminal and open in the Terminal view, and their Chat | Terminal switch only changes the view (no restart, no confirmation, kept while the screen is open). Idea conversations open in Chat. In the **Projects** paragraph (~L415), mention that "Ask Claude" opens in the terminal and the header switch shows the chat of the same session.
- [X] T011 [P] Add one line to the "Built" section of docs/status.md: project Claude sessions open in the terminal and have a view-only Chat | Terminal switch (LIBRE-32).
- [X] T012 [P] Add decision **104** to docs/decisions.md (after 103): "A project's sessions open in the terminal; their switch only changes the view (2026-11-01)". Include the why (they always run on a PTY per decisions 16 and 36, and the chat over the PTY is only a reading of the transcript), idea conversations keep Chat, the choice is not stored (unlike the task's `view` setting), and what was ruled out: a sibling component, storing the choice, and a daemon-side runtime switch for projects.
- [ ] T013 Walk through specs/014-project-session-terminal/quickstart.md in a preview (`scripts/preview.sh --fresh`, in the background, never the installed daemon on 7420). Have `ui-verifier` check scenarios 1–7 against the preview URL.

---

## Dependencies & execution order

- **Phase 2 (T001→T002→T003)**: one file pair. T001 and T002 come before T003.
- **US1 (T004→T005)**: independent of Phase 2. It can run in parallel with Phase 2 (TaskScreen
  files and SessionViewSwitch files don't overlap).
- **US2 (T006→T007)**: depends on T003 and T005.
- **US3 (T008→T009)**: after T007.
- **Polish**: T010, T011 and T012 can run in parallel with each other and with any phase (docs only).
  T013 comes last.

### Parallel opportunities

```text
Track A: T001 → T002 → T003   (SessionViewSwitch.tsx / .test.tsx)
Track B: T004 → T005          (TaskScreen.tsx / .test.tsx)
Track C: T010 | T011 | T012   (docs)
then:    T006 → T007 → T008 → T009 → T013
```

## Implementation strategy

- **MVP**: Phase 3 (US1) alone fixes the reported bug: project sessions open in the terminal.
- **Increment 2**: Phase 2 + US2 add the view-only switch, so the user can move between Terminal and Chat.
- **Increment 3**: US3 regression guards, docs, and the preview walkthrough.
- No daemon, API or migration change. No version bump on this branch.
