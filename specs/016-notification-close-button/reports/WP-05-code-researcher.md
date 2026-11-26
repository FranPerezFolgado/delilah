# Work Package Map: WP-05 (Tasks T011, T012, T019, T020)

**Feature**: Close button on every notification  
**Phase**: 6 US4 (Banners inside task screen and Jira view)  
**Depends on**: WP-01 (Notice wrapper, CloseButton, useDismissals, useDismissal)

## Files

| File | Lines | What's there and why it matters |
|------|-------|--------------------------------|
| `app/src/components/TaskScreen.tsx` | 83–149 | `BaseStaleBars` component: maps `notices` (InboxItem[]) to base-stale warning bars. Currently has a "Descartar" text button (line 142) that calls `api.dismissNotice`. Must become `<Notice>` with shared X (`.ntc-x`) and preserve "Actualizar rama" button. |
| `app/src/components/TaskScreen.test.tsx` | 602–644 | BaseStaleBars tests: line 607 clicks "Descartar" button; must change to click the shared X by role. Tests must verify: X calls `api.dismissNotice(id)` then `onChanged`; bars without notice id hide locally; "Actualizar rama" stays. |
| `app/src/components/SubtaskList.tsx` | 79–84 | "N subtareas nuevas de Jira" banner: shows when `fresh > 0` (new create/attach rows). Currently no close control. Must become `<Notice>` with local dismissal (sig = `String(fresh)`); "Añadir" button stays. |
| `app/src/components/SubtaskList.test.tsx` | 61–70, 132–137 | Jira import banner tests: verify banner counts (61), "Añadir" opens dialog. Must add: X hides banner; different count shows it again; "Añadir" still works. |
| `app/src/components/ui/index.ts` | — | Must export Notice, CloseButton, useDismissals, useDismissal (from WP-01). |

## Patterns

**Base-stale dismissal** (persistent):
- Bar has `n.notice` id (a pr_notices row); user clicks X → `api.dismissNotice(n.notice)` → `onChanged()`
- If no id: local dismissal via `useDismissal(id=n.text, sig=n.text)` on rerender

**Subtasks banner dismissal** (in-memory):
- `useDismissal(fresh > 0 ? String(fresh) : null)` — hides when `fresh > 0` → returns when count changes
- Skip banner in JSX if `hidden` from hook

**Notice wrapper** (from WP-01):
- `<Notice className="ts-bar base-stale" role="status" onClose={() => …}>`
- Renders children + shared X last (`.ntc-x`, "Cerrar")
- When X had focus, focus moves to next focusable or nothing if none

**Signature change to show again**:
- Base-stale sig: `n.text` (the warning message text)
- Subtasks sig: `String(fresh)` (the count)

## Tests

| Component | Location | Pattern |
|-----------|----------|---------|
| BaseStaleBars | TaskScreen.test.tsx:602–644 | Spies on `api.dismissNotice`, `api.baseStatus`, `api.updateBranch`; mocks via `baseStatus.mockResolvedValueOnce`, etc.; checks toast text; updates existing test at line 607 from `getByText("Descartar")` to `getByRole("button", { name: "Cerrar" })` |
| Subtasks banner | SubtaskList.test.tsx:61–70 | Mocks `api.jiraImportPreview` to return rows with `fresh > 0`; uses `findByText` for banner text and button "Añadir"; adds X test: click, check hidden, rerender with different count, check visible |

## Watch out

- **API call order**: `api.dismissNotice(id)` must complete before `onChanged()` to avoid race; use `.then(onChanged, () => {})` (swallow errors, always call onChanged)
- **Mixed dismissal types**: Base-stale bars can be with-id (persistent) or without (local). Same component, different behavior. Test both.
- **Notice wrapper dependency**: Until WP-01 is done, Notice, CloseButton, useDismissals, useDismissal don't exist. Implement in dependency order.
- **Focus management**: After X closes a bar, focus must not get lost. Notice hook handles this; test it passes from WP-01.
- **Outside allowed files**: WP-05 touches only TaskScreen/SubtaskList and their tests (and ui/index.ts for exports). Other banners (SetupBanner, HandoffActions, TaskView, PrView, JiraView, DisconnectedBanner) are WP-06, WP-07, WP-08.
