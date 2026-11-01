# Implementation Plan: Project Claude sessions open in the terminal

**Branch**: `014-project-session-terminal` | **Date**: 2026-11-01 | **Spec**: [spec.md](spec.md)

**Input**: Feature specification from `specs/014-project-session-terminal/spec.md`

## Summary

On a project's screen, Claude sessions show the Terminal view by default (idea conversations
keep Chat), and the existing Chat | Terminal switch is shown there in a **view-only** mode. That
mode changes what the tab shows and does nothing else: no confirmation, no daemon call, no busy
lock. The change is UI only. Two files change: `TaskScreen.tsx` (default view and rendering the
switch on projects) and `SessionViewSwitch.tsx` (a controlled view-only mode). Tests and the
docs are updated to match. The daemon, the HTTP API and the database do not change.

## Technical Context

**Language/Version**: TypeScript 5 / React 18 (`app/`)

**Primary Dependencies**: existing UI kit (`Segmented`, `ConfirmDialog` in `app/src/components/ui`)

**Storage**: N/A. The per-session view stays in `TaskScreen`'s in-memory `views` map, as today.

**Testing**: Vitest + Testing Library (`npx tsc --noEmit && npx vitest related --run <files>`)

**Target Platform**: Delilah desktop app (Tauri webview) and the browser preview

**Project Type**: desktop app, web UI layer

**Performance Goals**: the view flips immediately (local state only)

**Constraints**: task sessions must keep the runtime switch exactly as today; terminals must stay
mounted across view flips so the scrollback is kept (already true: `mounted` set)

**Scale/Scope**: 2 components, 2 test files, 3 docs

## Constitution Check

`.specify/memory/constitution.md` is still the unfilled template, so the gates are the project
rules in `CLAUDE.md`:

| Gate | Status |
|---|---|
| Docs kept true (`docs/status.md`, `docs/usage.md`, decision in `docs/decisions.md`) | ✅ planned (T-docs) |
| HTTP contract `api.rs` ↔ `api.ts` unchanged, or both sides changed together | ✅ untouched |
| No shipped migration edited | ✅ no migration |
| Changes tried in a preview, never on the installed daemon | ✅ quickstart uses `scripts/preview.sh` |
| No version bump on the feature branch | ✅ none |

After the design, the result is the same: no violations, and nothing goes in Complexity Tracking.

## Design

### 1. Default view (`app/src/components/TaskScreen.tsx`, `viewOf` ~L197)

For a project, the fallback changes from `"chat"` to a value based on the kind of session:

```ts
: project
  ? s.runtime === "stream"
    ? "chat"
    : (views.get(s.id) ?? (s.conversation ? "chat" : "terminal"))
```

The comment above `views` (~L191, "a project's sessions open in chat") is updated to match. The
`runtime === "stream"` branch stays, with a comment that marks it as defensive: the daemon never
runs a project session on a stream (`run.rs::runtime_kind`).

### 2. View-only mode of `SessionViewSwitch` (`app/src/components/SessionViewSwitch.tsx`)

The props become a discriminated union, so the mode is explicit. Passing `view` alone can't turn it
on by accident:

```ts
type SessionViewSwitchProps =
  | { mode: "view"; view: SessionView; chooseView: (v: SessionView) => void }
  | { mode?: "runtime"; session: Pick<Session, "id" | "live" | "runtime" | "effective_runtime">;
      busy?: boolean; onChanged?: () => void; onError?: (m: string) => void };
```

- `mode: "view"`: shows `view`. Picking the other value calls `chooseView(next)` and nothing else:
  no `ConfirmDialog`, no `api.setSessionRuntime`, no busy lock (FR-005). Picking the value already
  shown does nothing.
- `mode: "runtime"` (the default, so task callers don't change): today's behaviour, unchanged.
- Both modes render the same `Segmented` (label, options, beta tag, `.svs` wrapper). The deprecated,
  unused `view`/`chooseView` props of the runtime mode are removed.
- The existing runtime tests pass `view`/`chooseView`, which do nothing today
  (`SessionViewSwitch.test.tsx` L22, 45, 56, 63, 68, 76). They drop those props, keep their
  assertions, and type-check against the runtime arm.

This keeps one component and one set of markup and labels instead of a sibling copy (research R1),
and the two behaviours stay explicit (plan review).

### 3. Rendering the switch on projects (`TaskScreen.tsx`, `sessionSwitch` ~L668)

```tsx
const sessionSwitch = !shownClaude ? undefined : project ? (
  <SessionViewSwitch
    mode="view"
    view={viewOf(shownClaude)}
    chooseView={(v) => chooseView(shownClaude, v)}
  />
) : ( /* today's runtime switch, unchanged */ );
```

- `shownClaude` already excludes shell sessions, so FR-009 holds.
- The project header (`.ts-head`, ~L710) already renders `{sessionSwitch}`, so nothing changes
  there.
- `chooseView` (~L633) already calls `showView` and then `rememberView`. `rememberView` returns early
  when there is no `task`, and a project screen has none, so FR-008 ("no task view changed")
  holds. To make that explicit, `chooseView` calls `rememberView` only when `!project`, and a test
asserts that `updateTask` is not called. The comment "(the switch is not offered there)" is updated.
- ConversationView's "open the terminal" buttons already go through `chooseView`, so the switch
  shows Terminal after them (FR-007), because its `view` is `viewOf(shownClaude)`.
- The comment above `sessionSwitch` ("Not for a project's sessions: the daemon refuses…") becomes
  "On a project it only changes the view: the daemon keeps project sessions in a terminal."

### 4. Tests

`app/src/components/SessionViewSwitch.test.tsx`:
- view-only: picking Chat/Terminal calls `chooseView` with the value. It does not call
  `api.setSessionRuntime` and shows no dialog.
- view-only: `busy` does not disable the options.
- view-only: shows `view` as pressed, whatever the session's runtime is.
- Existing runtime tests: drop `view`/`chooseView` from their renders. Their assertions stay.

`app/src/components/TaskScreen.test.tsx` (the project block, ~L250-310):
- Replace "opens a project session in chat even when it is not an idea conversation" with "opens
  a project session in the terminal": `terminal` is present and `conversation` is not.
- Replace "offers no Chat / Terminal switch on a project conversation" with "offers a view-only
  switch on a project session". `.svs` is present. Clicking Chat shows `conversation`, clicking
  Terminal shows `terminal` again, and `setSessionRuntime` and `updateTask` are not called.
- Idea conversation: still opens in chat (the existing first test), and now has a switch.
- Switch usable while the session is `working`.
- Shell session test unchanged (no switch).
- Task block ("shows a task session in the view of its runtime"): unchanged, as a regression guard.

### 5. Docs

- `docs/usage.md` ~L239 and the **Projects** paragraph (~L415): project sessions open in the
  terminal. The Chat | Terminal switch only changes the view there, with no restart. Idea
  conversations open in chat.
- `docs/status.md` (Built): one line for LIBRE-32.
- `docs/decisions.md`: **104. A project's sessions open in the terminal; their switch only changes
  the view (2026-11-01)**. The why: they always run on a PTY, and the chat over the PTY is only a
  reading of the transcript. Ruled out: a separate component (duplicated markup), storing the
  choice (task screens don't either), a daemon-side runtime switch for projects (out of scope:
  project sessions are read-oriented and PTY-only by decisions 16 and 36). Unlike the task's `view`
setting, the project choice is not stored.

## Project Structure

### Documentation (this feature)

```text
specs/014-project-session-terminal/
├── spec.md
├── plan.md              # this file
├── research.md
├── data-model.md
├── quickstart.md
├── contracts/
│   └── session-view-switch.md
├── checklists/requirements.md
└── tasks.md             # /speckit-tasks
```

### Source Code (repository root)

```text
app/src/components/
├── TaskScreen.tsx               # viewOf default, sessionSwitch on projects, comments
├── TaskScreen.test.tsx          # project block tests
├── SessionViewSwitch.tsx        # view-only (controlled) mode
└── SessionViewSwitch.test.tsx   # view-only tests; runtime tests drop view/chooseView
docs/usage.md, docs/status.md, docs/decisions.md
```

**Structure Decision**: web UI layer only (`app/src/components`). No crate changes.

## Complexity Tracking

None.
