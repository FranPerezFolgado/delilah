# Research: Group toasts

## R1 — Where the waiting group lives

- **Decision**: GlobalNotices owns it and renders it as one keyed toast through the generic store.
- **Rationale**: GlobalNotices already receives `tasks` and `current` and seeds «te espera»;
  the kit stays task-agnostic (same layering as 007-ui-v2's `components/ui`).
- **Alternatives**: a special "group" concept inside the toast store (couples the kit to tasks);
  moving waiting notices out of toasts into a `gn-bar` (would duplicate the Inbox/StatusBar and
  take permanent space — rejected: the user asked to group toasts, not remove them).

## R2 — Merge rule

- **Decision**: merge only when kind and text match exactly and neither toast has an action or key.
- **Rationale**: actions carry closures (each «Deshacer» undoes a different thing); merging them
  would lose one. Exact text keeps the rule predictable and testable.
- **Alternatives**: merge by a caller-provided group id (no caller needs it yet); fuzzy merge
  ignoring numbers (would hide distinct facts like «PR #12 fusionada» / «PR #13 fusionada»).

## R3 — Collapsed stack interaction

- **Decision**: newest in full + «+N» button; expand on hover, focus-within, or click (pinned
  until clicked again or pointer and focus leave); «Cerrar todo» when > 1.
- **Rationale**: the common pattern (stacked notifications in macOS and toast libraries), works
  with keyboard, keeps one item's height on screen.
- **Alternatives**: only lower MAX to 1 (loses toasts); a notification centre panel (overlaps
  the Inbox).

## R4 — Timer pause

- **Decision**: per-toast remaining time in a ref; host passes `paused`; resume with remaining.
- **Rationale**: no store-level clock; fake timers in tests stay simple.
- **Undo toasts**: ProposalCard's pending discard has its own timer; it dismisses its toast when
  that fires, so pausing can't show a stale «Deshacer». TaskView's undo is not time-bound in the
  daemon (it dismisses by id when state changes) — unaffected.

## R5 — Accessibility

- **Decision**: one polite live region on the host; collapsed toasts are not rendered.
- **Rationale**: per-toast `role="status"` re-announces on every re-render of a merged/updated
  toast and announces hidden ones. Tests that query `getByRole("status")` are updated to the
  host region / text queries.

## R6 — Eviction

- **Decision**: over MAX, drop the oldest plain info toast first (no action, no key), else the
  oldest. MAX stays 3 (counting the keyed waiting toast as one).
- **Rationale**: info toasts are about to expire anyway; errors, undo and waiting carry state.
