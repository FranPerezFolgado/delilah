## Spec compliance — 014 project session terminal (07cf05d..HEAD, T001-T012)

Covered:
- FR-001 ✓ TaskScreen.tsx viewOf (`views.get(s.id) ?? (s.conversation ? "chat" : "terminal")`); tests "opens a project session in the terminal", read-only, not-live (with "Reabrir").
- FR-002 ✓ same line; test "an idea conversation opens in chat and offers the view-only switch".
- FR-003 ✓ TaskScreen.tsx sessionSwitch renders `mode="view"` when `project`.
- FR-004 ✓ view mode calls only `chooseView`; chooseView skips rememberView on projects; test asserts no setSessionRuntime/updateTask/confirm.
- FR-005 ✓ view mode has no busy; test "stays enabled while working".
- FR-006 ✓ per-session `views` Map in component state, not persisted; test "each project session keeps its own view".
- FR-007 ✓ onOpenTerminal at TaskScreen.tsx:858 calls chooseView(s,"terminal"); switch reads viewOf; test "open-terminal button leaves Terminal pressed".
- FR-008 ✓ runtime arm unchanged; new task regression test (confirm dialog "Pasar a chat").
- FR-009 ✓ viewOf shell -> terminal; sessionSwitch is for shownClaude only; existing shell test kept.
- FR-010 ✓ docs/usage.md, docs/status.md, docs/decisions.md #104.
- Contract session-view-switch.md: props union matches exactly (SessionViewSwitch.tsx).
- Tasks T001-T012 all match their descriptions. T009 grep of `?? "chat"` finds only NewTaskDialog (task view, unrelated).

Missing: none. (T013 walkthrough out of scope, per brief.)

Extra: none. Only addition is the `onOpenTerminal` hook in the ConversationView test mock, which T006(e) allowed.

Deviates: none.

Untested / notes (LOW):
- US1 scenario 3 (restart in normal mode "allow changes" keeps terminal) has no dedicated test. It follows from viewOf not depending on read_only, and the read-only test covers the default.
- "Turn into a task" edge case follows task rules by construction; no test, none required by tasks.
- I did not run the test suite; verdict is from reading the code and tests.

Findings: CRITICAL 0, HIGH 0, MEDIUM 0, LOW 1 (the untested restart scenario).

VERDICT: ✅ COMPLIANT
