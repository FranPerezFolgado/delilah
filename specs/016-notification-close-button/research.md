# Research: Close button on every notification

## R1. Where the dismissal lives

- **Decision**: client-side, in memory, in the component that renders the notice (`useDismissals`); global bars live in always-mounted components (`GlobalNotices`, `UpdateNotice`, `App`), so their closes last the app session.
- **Rationale**: the spec asks to hide "until its state changes" without touching the real state; nothing needs it across reloads, and no daemon change keeps the contract untouched.
- **Alternatives**: persisting in `localStorage` (survives reloads, but stale entries and a reason to reappear are harder to define); a daemon "dismissed" flag (API + migration for a purely visual preference). Ruled out.

## R2. When a closed notice comes back

- **Decision**: each notice has an id and a signature string of what it reports; a dismissal applies only while the signature is equal, and is forgotten (effect) when it differs or the notice disappears.
- **Rationale**: one rule covers every FR-006 case (threshold change, new reason, next disconnect, new version) and the A → B → A edge case.
- **Alternatives**: timers (come back after N minutes) — not asked for; a per-component ad-hoc flag reset in effects — twelve copies of the same logic.

## R3. One close control

- **Decision**: `CloseButton` = kit `IconButton` with `icon="close"`, label `t("Cerrar")`, 28 px; `Notice` wraps a bar and appends it.
- **Rationale**: the brief asks for `IconButton` + `close` from the design system and a shared `Notice` instead of repeating the X; 28 px matches the bars' `size={28}` buttons.
- **Alternatives**: a `size` prop on `IconButton` (touches every icon button's API for one use); styling the X per bar (breaks SC-005).

## R4. X that maps to an existing action

- **Decision**: toasts → `closeToast`; update `upToDate`/`error` → `close`; `available` → `later`; `confirmSessions` → `cancel`; base-stale → `api.dismissNotice`. Text buttons "Cerrar" and "Descartar" are removed (duplicates of the X); "Más tarde" and "Cancelar" stay (named choices).
- **Rationale**: closing those already had a meaning the state machine or the daemon knows; hiding instead would leave them half-closed.
- **Alternatives**: X only hides everywhere — the available update would reappear on the next poll, the base-stale notice on the next refresh.

## R5. Open questions of the brief (decided in autopilot)

- Disconnected banner: included; X hides it, the content stays `inert`; back on the next loss.
- Inbox ask/permission cards: excluded; they need an answer and the agent would be left waiting.
- X during download/install: hides only; there is no cancel today.
- Native macOS notifications: out of scope.

## R6. Focus after closing

- **Decision**: `Notice` moves focus to the next focusable element after the notice if the X had focus.
- **Rationale**: removing the focused node sends focus to `body` in WebKit; the toast host already works around a similar case.
- **Alternatives**: focus the main area (couples the kit to `App`); do nothing (keyboard users lose their place).
