# UI contract: toasts (replaces the Toast row of 007-ui-v2 contracts/ui.md)

## API (`components/ui`, barrel unchanged)

| Export | Signature | Change |
|---|---|---|
| `pushToast` / `useToast()` | `(input: ToastInput) => number` | + `key`, `onDismiss`; merges identical action-less toasts; keyed update in place |
| `dismissToast` | `(id: number) => void` | unchanged signature; ids of merged repeats resolve |
| `dismissToastByKey` | `(key: string) => void` | **new** |
| `WAITING_TOAST_KEY` | `"waiting"` | **new** constant |
| `clearToasts` | `() => void` | unchanged |
| `ToastHost` | component, mounted once in App | collapsed stack, pause, «Cerrar todo» |

All current call sites compile and behave the same except GlobalNotices («te espera»).

## GlobalNotices props

| Prop | Change |
|---|---|
| `onOpenInbox: () => void` | **new**, required; App passes `() => setView("inbox")` |

## Rendering

| State | Shown |
|---|---|
| 0 toasts | nothing |
| 1 toast | the toast (text, «×N» if count ≥ 2, action, ×) |
| > 1, collapsed | newest toast in full; «+{n}» button (`aria-expanded=false`, label «Mostrar {n} avisos más»); «Cerrar todo» |
| > 1, expanded (hover / focus-within / «+N» clicked) | every toast, oldest first; «Cerrar todo» |

Host: bottom-left of the window as today; a visually hidden `role="status"` announcer
(`aria-live="polite"`) reads only new pushes, merges and keyed updates; the stack itself is not live.

## Texts (es → en)

| es | en |
|---|---|
| «{n} tareas te esperan» (tn with «{n} tarea te espera») | «{n} tasks are waiting for you» / «{n} task is waiting for you» |
| «Ver» | «View» |
| «Cerrar todo» | «Close all» |
| «+{n}» | «+{n}» |
| «Mostrar {n} avisos más» | «Show {n} more notices» |
| «×{n}» | «×{n}» |
| «{key} te espera», «Responder» | existing |
