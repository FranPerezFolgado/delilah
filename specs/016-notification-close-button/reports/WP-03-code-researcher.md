# WP-03 Code Map: UpdateNotice Component (T007–T008)

## Files
- `app/src/components/UpdateNotice.tsx:19–92` — 8 render branches by state.kind (switch); dispatches via `dispatch()` callback; uses `useState` for notes toggle
- `app/src/components/UpdateNotice.test.tsx` — 10 test cases; uses `setup()` helper (line 8) with vi.fn() spy; tests per state kind
- `app/src/updates.ts` — read-only: UpdateState union type (lines 4–13) + UpdateEvent/UpdateIntent types; reduce() function untouched

## State Render Map
Each state renders a `.gn-bar` (or `.gn-err`, `.gn-wait` subclass) div with role="status"|"alert":
- **idle** → `null`
- **checking** → bar with status text (only if `state.manual`)
- **upToDate** → bar + text button "Cerrar" (line 30, dispatches `type:"close"`)
- **available** → bar with "Abrir novedades" toggle, "Actualizar" (dispatch `requestUpdate`), "Más tarde" (dispatch `later`)
- **downloading** → bar + progress element (no buttons)
- **installing** → bar with status text (no buttons)
- **error** → bar with `.gn-err` class + "Reintentar" (dispatch `retry`), "Cerrar" (dispatch `close`)
- **confirmSessions** → bar with 3 buttons: "Actualizar ahora" (`now`), "Cuando terminen" (`whenIdle`), "Cancelar" (`cancel`)
- **waiting** → bar with 2 buttons: "Actualizar ahora" (`now`), "Cancelar" (`cancel`)

## Dispatch Actions
- `close`: upToDate (line 30), error (line 70)
- `later`: available (line 44)
- `cancel`: confirmSessions (line 80), waiting (line 89)
- `requestUpdate`: available (line 43)
- `retry`: error (line 69)
- `now`: confirmSessions (line 78), waiting (line 88)
- `whenIdle`: confirmSessions (line 79)
- `setNotesOpen`: toggle state inside available (line 40)

## Test Patterns
- **Setup**: `vi.fn()` dispatch spy, render component, assert text presence/absence
- **User interaction**: `fireEvent.click(screen.getByText(...))`, then check `dispatch` was called with expected object
- **State coverage**: one test per `.kind` value (8 kinds = 8 core tests + 2 edge cases: singular/plural session count, check failure distinction)
- **Assertions**: `screen.getByText()` for visible content, `screen.queryByText()` for absence, `dispatch.toHaveBeenCalledWith()`

## Watch Out
1. **Plan §3**: spec requires wrapping future states in `<Notice>` (closing state: idle; dismiss-only states: checking, downloading, installing, waiting). Current code has text buttons ("Cerrar", "Más tarde", etc.) that will move to X in a later PR.
2. **Test line 52**: currently finds "Cerrar" by text; plan says "update line 52 to the X by role" (likely `.getByRole("button", { name: "Cerrar" })` after Notice wrapping).
3. **Reducer untouched**: `updates.ts` reduce() function is frozen; all render changes stay in UpdateNotice.tsx component.
4. **No action changes**: dispatch payloads (`type: "close"`, etc.) remain identical; only the trigger (text button → X, or hidden while dismissed) changes.
