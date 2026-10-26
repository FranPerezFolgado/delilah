# Implementation Plan: Session tabs named after their phase

**Branch**: `011-tab-phase-names` | **Date**: 2026-10-26 | **Spec**: [spec.md](spec.md)
**Input**: Feature specification from `specs/011-tab-phase-names/spec.md`

## Summary

The task screen names every primary session «Claude principal» / «Main Claude», so
saving-mode phase sessions lose their phase name. The fix is UI-only. One helper,
`sessionName(session)` in `app/src/chain.ts`, names a session: by its step when it has
one (the existing `stepLabel`, already translated), else «Claude principal» for the
primary, else its stored label. Both places that name sessions on the task screen
use it: the tab bar (`TaskScreen.tsx`) and the Sessions list on the task tab
(`TaskView.tsx`). The daemon, the API and the data stay as they are.

## Technical Context

**Language/Version**: TypeScript 5 / React 18 (`app/`, Vite)
**Primary Dependencies**: none new; the app's `t()` (`app/src/i18n.ts`) and `stepLabel` (`app/src/chain.ts`)
**Storage**: N/A (no change; `Session.step` and `Session.label` already come from the daemon)
**Testing**: vitest + Testing Library (`app/src/chain.test.ts`, `TaskScreen.test.tsx`, `TaskView.test.tsx`)
**Target Platform**: Delilah desktop app (Tauri webview) and the browser UI of a preview
**Project Type**: desktop app, web UI layer only
**Performance Goals**: N/A (a pure function per rendered session)
**Constraints**: the names must match the session chain's step names (FR-005); tooltips keep the stored label (FR-006)
**Scale/Scope**: 2 components, 1 helper, 3 test files, 2 docs

## Constitution Check

`.specify/memory/constitution.md` is the unfilled template, so there are no gates of its
own. The project rules in `CLAUDE.md` apply instead:

- Contracts (`api.rs` ↔ `api.ts`): untouched. **Pass**.
- Migrations: none. **Pass**.
- Tests next to the code, written first (vitest). **Pass** (tasks are test-first).
- Docs kept true: `docs/usage.md` (task screen, sessions) and `docs/status.md`. **Pass**.
- No version bump on the branch. **Pass**.

Re-checked after design: still passes, and nothing goes in Complexity Tracking.

## Project Structure

### Documentation (this feature)

```text
specs/011-tab-phase-names/
├── spec.md
├── plan.md              # this file
├── research.md          # Phase 0
├── data-model.md        # Phase 1
├── quickstart.md        # Phase 1
├── contracts/ui.md      # Phase 1: the naming rule as UI contract
└── tasks.md             # /speckit-tasks
```

### Source Code (repository root)

```text
app/src/chain.ts                     # + sessionName(session): step → stepLabel, primary → «Claude principal», else label
app/src/chain.test.ts                # + unit tests for sessionName (es and en)
app/src/components/TaskScreen.tsx    # tab item label = sessionName(s) (title stays s.label)
app/src/components/TaskScreen.test.tsx  # phase tabs, former step sessions, close button name
app/src/components/TaskView.tsx      # Sessions list row = sessionName(x) (keeps the claude-only «Claude principal» rule)
app/src/components/TaskView.test.tsx # Sessions list shows phase names
docs/usage.md                        # task screen: session tabs are named after their phase in saving mode
docs/status.md                       # note the fix
```

**Structure Decision**: The helper sits next to `stepLabel` in `chain.ts`, which
already owns the step names and imports `Session`; both components import it, so the
tab bar and the Sessions list cannot drift apart (FR-004, SC-003).

### Design notes

- `sessionName(s)`:
  `s.step ? stepLabel(s.step) : s.primary && s.kind === "claude" ? t("Claude principal") : s.label`.
  The `kind === "claude"` check is today's `TaskView` rule. Today the tab bar has no
  such check, so a primary terminal reads «Claude principal» there. No such session
  is ever created (every primary session the daemon makes is a Claude), so the
  helper uses the TaskView rule for both places, and the spec's edge case about
  terminals becomes "unchanged in practice".
- `TaskScreen` tab item: `label: sessionName(s)`, `title: s.label` (unchanged).
  `closable: !s.primary` is unchanged. The close button's accessible name comes from
  the label («Cerrar Implementación»).
- `t()` reads the current language on each call, so the names follow a language
  change on the next render like every other string (SC-004). No memoising.
- Existing tests that expect «Claude principal» for a primary without a step stay as
  they are (FR-002 regression guard).

## Complexity Tracking

None.
