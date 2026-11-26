# WP-08 Code Researcher Map

## Files

- `app/src/components/DisconnectedBanner.tsx:1–27` — renders banner with status dot, text, and retry button; needs onClose callback and Notice wrapper
- `app/src/components/DisconnectedBanner.test.tsx:1–23` — two existing tests (retries, shows time); will add X button and dismissal tests
- `app/src/App.tsx:402–407` — shell-banner wrapper logic; currently shows when offline, needs to also check !hidden from useDismissal
- `app/src/App.tsx:407` — shell-content inert/aria-disabled/is-offline logic; must stay driven by offline state alone, independent of banner visibility

## Patterns

**Dismissal pattern** (from WP-02 GlobalNotices, WP-04 UpdateNotice):
- `useDismissal(sig: string | null)` returns `[hidden, dismiss]`
- Sig changes → dismissal forgotten, banner reappears
- For disconnected: sig = "offline" when offline, null when online
- When connection drops again after recovery → sig "offline" is new → banner shows again

**Notice wrapper** (from contracts/ui-notice.md):
- Wraps caller's content, adds CloseButton named "Cerrar" (class `.ntc-x`) at end
- Caller's className/role preserved; onClose called when X clicked
- Focus moves to next focusable element after close (Notice handles this)

**Shell-banner structure** (current):
- `.shell-banner` div wrapper (child of `<main>`) shows/hides based on offline state
- Inside it: DisconnectedBanner component
- Must now be: show only when `offline && !hidden` (hidden from useDismissal)
- `.shell-content` stays inert/aria-disabled/is-offline iff `offline`, regardless of banner visibility

## Tests

**DisconnectedBanner.test.tsx** (extend existing two):
- Add: X button is shared `.ntc-x` with accessible name "Cerrar"
- Add: clicking X calls onClose

**App.test.tsx** (extend existing offline test at line 77):
- Offline → banner shows with X
- Click X → banner hides, content still inert/aria-disabled
- Online (conn.status = "up") → banner gone
- Offline again → banner back
- Verify: `is-offline`/`aria-disabled`/`inert` attributes unchanged through close/reopen cycle

## Watch out

- DisconnectedBanner's onClose only hides banner, never changes offline/conn state
- .shell-content must keep attributes driven by `offline` state, not by banner visibility
- Sig must be "offline" (or null) so dismissal resets when connection status changes
- Retry button ("Reintentar ahora") stays visible and functional
