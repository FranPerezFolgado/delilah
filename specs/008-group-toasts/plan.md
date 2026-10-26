# Implementation Plan: Group toasts

**Branch**: `008-group-toasts` | **Date**: 2026-10-26 | **Spec**: [spec.md](spec.md)

**Input**: Feature specification from `specs/008-group-toasts/spec.md`

## Summary

Cut toast noise in the desktop UI without touching the daemon. The toast store in
`app/src/components/ui/Toast.tsx` learns three things: **merge** identical action-less toasts
into one with a repeat count, **keyed** toasts that are updated in place (used by a single
«te espera» toast), and a **user-dismiss callback** so the owner of a keyed toast knows it was
closed. `ToastHost` renders a **collapsed stack** (newest in full, «+N», expand on
hover/focus/click, «Cerrar todo») and **pauses** info timers while it is hovered, focused or
expanded. Eviction prefers the oldest plain info toast. `GlobalNotices` stops pushing one toast
per waiting task and instead keeps a waiting group derived from the task list it already gets,
rendering it as the keyed toast «{key} te espera · Responder» / «{n} tareas te esperan · Ver»;
`App` passes it an `onOpenInbox`. `ProposalCard` dismisses its own «Deshacer» toast when its
pending discard fires, so pausing never shows a dead undo.

## Technical Context

**Language/Version**: TypeScript 5 / React 18 (app/), no Rust change

**Primary Dependencies**: none new (React `useSyncExternalStore` store already in place)

**Storage**: N/A (in-memory, toasts don't survive reload — unchanged)

**Testing**: Vitest + Testing Library (jsdom, fake timers), `i18n/coverage.test.ts`

**Target Platform**: Delilah desktop app web UI (Tauri webview) and the browser preview

**Project Type**: desktop-app (frontend part only)

**Performance Goals**: no measurable cost; toasts ≤ 3 items

**Constraints**: public toast API stays source-compatible for every current call site;
Spanish texts through `t()` with English entries (coverage test is strict both ways)

**Scale/Scope**: 1 kit component + CSS, GlobalNotices, a one-line App prop, ProposalCard,
i18n, tests, README/docs

## Constitution Check

`.specify/memory/constitution.md` is the unfilled template: no gates. Project rules from
`CLAUDE.md` apply instead and pass:

- Frontend only, no contract change in `api.rs` ↔ `api.ts`; no migration.
- Checks: `cd app && npx tsc --noEmit && npx vitest related --run <files>`, then `npx vitest run`.
- Tried in a preview (`scripts/preview.sh`), never against the installed daemon.
- Docs: README (toasts are user-facing) and `docs/status.md`; decision recorded in
  `docs/decisions.md` (grouped waiting toast replaces per-task toasts).
- Version bump: **minor** (user-facing behaviour change, backwards compatible) at the end.

Post-design re-check: still passes (no new module boundaries; the kit stays generic — it knows
nothing of tasks, GlobalNotices owns the waiting group).

## Design

### Toast store (`components/ui/Toast.tsx`)

`ToastInput` gains optional fields, all backwards compatible:

| Field | Meaning |
|---|---|
| `key?: string` | Identity for in-place updates: pushing with a key that is on screen replaces its text/action/kind and keeps its position and id; no merge counting. |
| `onDismiss?: () => void` | Called when the **user** removes the toast (× or «Cerrar todo»), not on auto-expiry, action click or programmatic `dismissToast`. |

`ToastItem` gains `count` (≥ 1), `aliases` (ids of merged repeats), `deadline` (epoch ms,
info only) and `remaining` (ms left while paused).

**Timers live in the store, not in the view** (hidden toasts are not rendered while
collapsed, and must still expire): one store-level timeout is scheduled for the earliest
`deadline`; when it fires, expired items are removed and the next one is scheduled.
`setToastsPaused(true)` converts every `deadline` into `remaining` and clears the timeout;
`setToastsPaused(false)` turns `remaining` back into `deadline = now + remaining`. A merge
resets the item to the full `ttl`. Errors, `wait` and keyed toasts have no deadline.

- `pushToast(input)`:
  1. `key` set and an item with that key exists → update it in place, return its id.
  2. No `key`, no `action`, and an item with same `kind` (default info) and `text` exists →
     `count++`, `bump++`, add the new id to `aliases`, move it to the end, return the new id.
  3. Otherwise append; if over `MAX` (3), evict the oldest item that is `info` with no action
     and no key; if none, the oldest item **without a key** (keyed toasts are never evicted,
     so the waiting toast can't vanish while GlobalNotices still holds a group).
- `dismissToast(id: number)`: signature unchanged; removes the item whose id or alias matches
  (programmatic: no `onDismiss`).
- `dismissToastByKey(key: string)`: new, removes the keyed item (programmatic).
- `WAITING_TOAST_KEY = "waiting"` exported for GlobalNotices (no magic string).
- `clearToasts()` unchanged (programmatic, used by tests).
- Internal `closeToast(id)` / `closeAll()` used by the host: call `onDismiss` then remove.

### Host (`ToastHost`)

- `expanded` state = hovered || focus-within || pinned. `pinned` is toggled by «+N» and
  reset when the pointer has left **and** focus is outside the stack (FR-010), so a clicked
  stack can't stay expanded/paused forever. The host calls `setToastsPaused(expanded)`.
- Collapsed with >1 item: render the newest `ToastView` in full plus a stub row
  `<button class="toast-more" aria-expanded="false" aria-label="Mostrar {n} avisos más">+{n}</button>`
  and «Cerrar todo». Older toasts are not rendered while collapsed (no hidden live regions →
  no duplicate announcements).
- Expanded: every toast, oldest at top, newest at the bottom; «Cerrar todo» at the bottom.
- `ToastView` is purely presentational (no timers).
- Count badge «×N» when `count ≥ 2`, «×99+» above 99.
- Announcements: a separate visually hidden `role="status"` (`aria-live="polite"`) announcer
  holds the text of the last push, merge («… ×N») or keyed update only; the visible stack is
  not a live region, so expanding it never re-announces older toasts (FR-019).

### Waiting group (`components/GlobalNotices.tsx`)

- Keep `seen` (seeding as today) and add `group: string[]` in a ref, ordered by arrival.
- On each `tasks` change: add keys that newly started waiting and are not `current`; drop keys
  that no longer wait, are no longer in the list, or equal `current` (also re-checked when
  `current` changes).
- The group is recomputed in an effect over `[tasks, current]` (today's effect depends on
  `tasks` only; seeding stays in the `tasks` effect, the group logic gets its own).
- Render to the store: empty → `dismissToastByKey(WAITING_TOAST_KEY)`; 1 → `pushToast({key: WAITING_TOAST_KEY,
  kind: "wait", text: t("{key} te espera"), action: {label: t("Responder"), run: open(key)},
  onDismiss: clearGroup})`; N → `text: tn(n, …)` → «{n} tareas te esperan», action
  `{label: t("Ver"), run: onOpenInbox}`.
- `onDismiss` empties the group (keys stay in `seen`, so they don't come back until they stop
  and start waiting again).
- New prop `onOpenInbox: () => void`; `App` passes `() => setView("inbox")`.
- On unmount: `dismissToastByKey(WAITING_TOAST_KEY)`.

### ProposalCard

Keep the id returned by `pushToast` for «Tarea descartada» and `dismissToast(id)` when the
discard timer fires, so a paused toast can't outlive the action it undoes (FR-016).
TaskView already dismisses its undo toast by id; with aliases, this keeps working.

### Styles (`Toast.css`)

`.toast-host` keeps its place; add `.toast-more` (compact pill under/behind the newest toast,
drawn with the stacked-card offset), `.toast-count` badge, `.toast-clear` row. Same tokens as
today (FR-012).

## Project Structure

### Documentation (this feature)

```text
specs/008-group-toasts/
├── plan.md
├── research.md
├── data-model.md
├── quickstart.md
├── contracts/ui.md      # toast API + UI contract
└── tasks.md             # /speckit-tasks
```

### Source Code (repository root)

```text
app/src/
├── components/ui/Toast.tsx         # store: merge, key, onDismiss, eviction; host: collapse, pause
├── components/ui/Toast.css
├── components/ui/Toast.test.tsx
├── components/ui/index.ts          # export unchanged names (no new exports needed)
├── components/GlobalNotices.tsx    # waiting group → one keyed toast; onOpenInbox prop
├── components/GlobalNotices.test.tsx
├── components/ProposalCard.tsx     # dismiss own undo toast when discard fires
├── components/ProposalCard.test.tsx
├── App.tsx                         # pass onOpenInbox
└── i18n/en/Toast.ts (new; picked up by the `en/*.ts` glob), i18n/en/GlobalNotices.ts
README.md, docs/status.md, docs/decisions.md, docs/architecture.md (Kit line unchanged; toast note)
```

**Structure Decision**: frontend-only change inside the existing kit component and its one
consumer that owns waiting tasks; no new module.

## Plan review

Architecture review (no CRITICAL). Applied: HIGH — timers moved from `ToastView` to the
store so collapsed (unrendered) toasts still expire and pause; MEDIUM — `pinned` reset rule,
keyed toasts never evicted, separate non-visible announcer instead of a live stack; LOW —
`dismissToast` keeps its numeric signature, new `dismissToastByKey` + `WAITING_TOAST_KEY`,
new `i18n/en/Toast.ts` (coverage test is strict both ways: «+{n}», «×{n}» need literal `t()`).

## Complexity Tracking

None.
