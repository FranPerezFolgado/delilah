# Data model: Group toasts

In-memory only (front-end store); nothing is persisted.

## ToastInput (public, extended)

| Field | Type | Notes |
|---|---|---|
| text | string | required, already translated |
| action | `{label, run}`? | toasts with an action never merge |
| ttl | number? | ms, info only, default 6000 |
| kind | `"info" \| "error" \| "wait"`? | default info; error and wait never auto-dismiss |
| key | string? | **new**: in-place identity; never merged |
| onDismiss | `() => void`? | **new**: user closed it (× / «Cerrar todo») |

## ToastItem (internal)

`ToastInput` + `id: number`, `count: number` (≥ 1), `aliases: number[]`, `deadline?: number`, `remaining?: number` (store-level timers, paused as a whole).

Rules:
- `id` and every alias resolve to the item for `dismissToast`.
- `count` = 1 + number of merged repeats; shown as «×N» from 2, «×99+» above 99.
- A merge resets an info toast to its full `ttl`; keyed toasts are never evicted.
- Order = arrival order of the last push (merge moves to the end; keyed update keeps place).
- At most `MAX = 3` items; eviction per research R6.

## WaitingGroup (GlobalNotices)

`keys: string[]` in arrival order.

| Event | Transition |
|---|---|
| task newly waiting (not on first load), not `current` | append key |
| task no longer waiting / not in list / becomes `current` | remove key |
| user closes the waiting toast | clear all (keys stay in `seen`) |
| keys empty | `dismissToastByKey(WAITING_TOAST_KEY)` |
| keys = 1 / > 1 | push keyed toast «{key} te espera · Responder» / «{n} tareas te esperan · Ver» |
