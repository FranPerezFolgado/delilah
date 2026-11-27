# WP-04 Code Map: Toast Close Button Integration

**Feature**: Replace text × button in Toast with shared `CloseButton` component  
**Tasks**: T009 (test), T010 (implementation)  
**Files**: `app/src/components/ui/Toast.tsx`, `Toast.test.tsx`, `Toast.css`

## Files & Line References

| File | Location | Purpose |
|------|----------|---------|
| `Toast.tsx:193–195` | `ToastView` close button | Current: `<button aria-label={t("Cerrar")}>×</button>`; replace with `<CloseButton onClick={() => closeToast(item.id)} />` |
| `Toast.tsx:78–83` | `closeToast(id)` function | Calls `onDismiss()` then `dismissToast(id)`; already exists, will be called by CloseButton |
| `Toast.test.tsx:85–90` | Existing close test | Line 88: `screen.getByRole("button", { name: /Cerrar\|Close/ })` — will work after replacing with CloseButton (`.ntc-x` class) |
| `Toast.test.tsx:113–126` | `onDismiss` callback test | Verifies X triggers `onDismiss`; must stay green after change |
| `Toast.test.tsx:377–391` | Focus after close test | "Close the two older ones" at line 385–386; relies on button removal triggering focus management |
| `Toast.css:1–3` | Toast row layout | `.toast { display: flex; align-items: center; gap: 8px; }` — 28px X must fit here |

## Patterns & Structure

**Close button anatomy** (from Notice, once WP-01 ships):
- Component: `<CloseButton onClick={fn} />` (from `app/src/components/ui/Notice.tsx`, exported from `index.ts`)
- Renders as: `<IconButton label={t("Cerrar")} icon="close" />` → `<button aria-label="Cerrar"><Icon name="close" /></button>`
- CSS class: `.ntc-x { width: 28px; height: 28px; flex-shrink: 0; margin-left: auto; }`
- No text content, just icon

**Toast close integration**:
- `ToastView` renders action button (optional), then close X button
- `closeToast(item.id)` is a local function that calls `onDismiss` then removes the toast
- Tests rely on `getByRole("button", { name: "Cerrar" })` or by aria-label

**Toast CSS assumptions**:
- Flex row: `.toast { display: flex; align-items: center; gap: 8px; }`
- Text takes `flex: 1`, close button comes last
- If 28px X doesn't fit collapsed view, adjust `.toast` padding or `.toast-host` max-width in Toast.css

## Test Patterns

**RED (T009): New test structure**
```typescript
it("a pushed toast's close is the shared X", () => {
  // 1. Mount and push a toast with onDismiss callback
  // 2. Find the close button by `.ntc-x` class and "Cerrar" label
  // 3. Click it
  // 4. Assert: toast removed AND onDismiss called
  // 5. Assert: "Cerrar todo" still offered if >1 toast
});
```

**Existing tests that must stay green**:
- Line 85–90: manual close (already queries by role, will work)
- Line 113–126: `onDismiss` only on X, not on action or expiry
- Line 377–391: focus moves when button removed (will still work if CloseButton is a button)
- Line 306–318, 320–332: "Cerrar todo" and stack behavior (unchanged)
- Line 428–482: timer pause/resume (unchanged)

## Imports & Dependencies

**From `index.ts` (after WP-01)**:
```typescript
import { CloseButton } from "./Notice"; // or just import in Toast.tsx
```

**Already in `Toast.tsx`**:
- `closeToast(id)` exists and callable from new CloseButton
- `dismissToast(id)` exists
- `onDismiss` callback in `ToastItem` already exists

## Watch Out

1. **Timing**: `CloseButton` doesn't exist until WP-01 (Notice.tsx) is complete. T010 depends on that checkpoint passing.
2. **CSS fit**: The 28px X in a row with text + optional action button. If text wraps in collapsed view, the X may not fit. Test in a narrow window.
3. **Focus trap**: Toast.test.tsx:385–386 closes two buttons in sequence. Ensure focus moves correctly when buttons unmount (Button/IconButton already a `<button>`, so works).
4. **Merged toasts**: Toast merging (line 145–153 in Toast.tsx, test at 166–179) doesn't change. The count badge and X remain distinct.
5. **"Cerrar todo"**: The "Close all" button (line 274–276) is separate and unchanged.
