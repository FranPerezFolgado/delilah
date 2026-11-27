# Code Research: WP-01 — Close button UI kit (T001–T004)

## Files

| Path | Purpose & Notes |
|------|---|
| `app/src/components/ui/Notice.tsx` | New component (export `CloseButton`, `Notice`, `useDismissals`, `useDismissal` + types). Handles focus move on close, prunes effect keyed on serialised `current`. |
| `app/src/components/ui/Notice.test.tsx` | New test file. Uses @vitest-environment jsdom, imports from vitest and @testing-library/react, afterEach(cleanup). |
| `app/src/components/ui/Notice.css` | New stylesheet. Base: `.ntc { display: flex; align-items: center; }` + `.ntc-x { width: 28px; height: 28px; flex-shrink: 0; margin-left: auto; }` on existing CSS tokens. |
| `app/src/components/ui/index.ts` | Add exports: `CloseButton`, `Notice`, `useDismissals`, `useDismissal` and their prop types (follow existing pattern). |

## Key Findings

### IconButton & icon="close"
- Located: `app/src/components/ui/Button.tsx` (re-exported from `IconButton.tsx`)
- Signature: `IconButton({ label, icon, pressed?, className, ...rest })` → `<button aria-label={label}><Icon name={icon} /></button>`
- Icon name type: `IconName` from `Icon.tsx` (line 50: `type IconName = keyof typeof PATHS | keyof typeof PATHS16`)
- **"close" icon exists** (line 17 in Icon.tsx): `close: "M7 7l10 10M17 7L7 17"` ✓
- Icon size: 16×16 (`.b .i, .ib .i { width: 16px; height: 16px; fill: none; stroke: currentColor; }`)

### UI Kit Structure & Export Pattern
- Each component: `Component.tsx` (the component), `Component.test.tsx` (tests), optional `Component.css`
- All export through `ui/index.ts` as: `export { Component } from "./Component"` + `export type { ComponentProps, … }`
- Example (Button.tsx line 1–2, 26): exports `Button` + `ButtonProps`, `IconButton` + `IconButtonProps`
- Example (index.ts line 1–2): `export { Button, IconButton } from "./Button"` + types
- No component exports its hook utilities separately (each holds what it needs)

### CSS Tokens & Layout
- Tokens used in Button.css: `--primary`, `--primary-tx`, `--primary-hov`, `--card`, `--bd2`, `--tx`, `--tx2`, `--hov`, `--err`, `--err-bg`, `--sel`, `--tx-strong`
- Icon buttons: `.ib { width: 32px; height: 32px; display: inline-flex; align-items: center; justify-content: center; … }`
- Small variant: `.ib.sm` doesn't exist in Button.css, but Button has `.b.sm { height: 28px; … }`
- Flex layout for 28px button: set `width: 28px; height: 28px; flex-shrink: 0`; parent should have `display: flex`

### i18n Import Pattern
- Components in `app/src/components/ui/`: `import { t } from "../../i18n"`
- Higher-level components (e.g., `app/src/components/GlobalNotices.tsx`): `import { t, tn } from "../i18n"`
- All `t()` calls are keyed by string (English in code, translations in locale files)

### Test Setup
- Header: `// @vitest-environment jsdom`
- Imports: `import { describe, expect, it, vi } from "vitest"` + `import { afterEach, cleanup, fireEvent, render, screen } from "@testing-library/react"`
- Teardown: `afterEach(cleanup)` at end of file
- Example (Button.test.tsx:35–41): IconButton test uses `render()`, `screen.getByRole("button", { name: "Close" })`
- Current Toast test does **not** use i18n setup, so probably testing library in jsdom provides mock or default locale

## Patterns in This Codebase

- **Component + Hook**: When a component needs local state, it's internal (`useState`); when it needs to coordinate across siblings/parents, the hook is exported (e.g., `useToast`, `useUpdates`).
- **Props naming**: `onClick`, `onClose` (camelCase, React standard).
- **CSS naming**: short prefix (`.b` for Button, `.ib` for IconButton, `.ntc` for Notice/Notice Container).
- **Tests sit next to code**: `Component.tsx` + `Component.test.tsx` in the same directory.
- **Exports grouped by file**: if a file has multiple components, they share one `export` line in index.ts.

## Watch Out

- **Focus management edge case** (spec): When CloseButton had focus and was clicked, `Notice` must move focus to the next focusable element after the notice. This is a ~10-line helper inside `Notice.tsx`, covered by T001 test. Plan mentions this is per-notice, not a kit-wide util.
- **Effect keyed on serialised `current`**: `useDismissals(current: Record<string, string>)` uses `JSON.stringify(current)` or similar as effect key to handle A → B → A transitions within a single React batch (spec edge case: won't observe a state that only exists during one batch).
- **Toast CSS check**: T003 says "check the 28 px X fits the row in the toast stylesheet next to `Toast.tsx`" — Toast.css line 2–3 shows `.toast` has `gap: 8px`, and buttons in line 11–12 are flexible. The 28px × 28px icon button should fit; adjust `.toast` or button margins in Toast.css if misaligned.
- **Existing tests that rely on old selectors**: UpdateNotice.test.tsx:52 and TaskScreen.test.tsx:607 currently use `getByText("Cerrar")` / `getByText("Descartar")` for the old text buttons—these will break and must be updated to `getByRole("button", { name: "Cerrar" })` etc. (not in WP-01, but mentioned in plan).

## Tests for T001

The test file must cover:
1. `CloseButton`: button with name "Cerrar", class `ntc-x`, close icon, calls `onClick`
2. `Notice`: renders children + exactly one `CloseButton` last, keeps caller's `className` and `role`, calls `onClose`; focus moves on close
3. `useDismissals`: hides only dismissed ids with same sig, forgets on sig change, A → B → A within render shows again, re-render with same sig keeps hidden
4. `useDismissal(null)`: forgets dismissal
