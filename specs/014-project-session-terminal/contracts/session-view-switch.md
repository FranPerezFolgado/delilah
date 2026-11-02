# UI contract: `SessionViewSwitch`

```ts
type SessionView = "chat" | "terminal";
type SessionViewSwitchProps =
  /** View-only, for sessions whose runtime can't change (a project's): no confirmation,
   *  no daemon call, never disabled. */
  | { mode: "view"; view: SessionView; chooseView: (v: SessionView) => void }
  /** Runtime (default): changes the session's runtime, after a confirmation. */
  | { mode?: "runtime"; session: Pick<Session, "id" | "live" | "runtime" | "effective_runtime">;
      busy?: boolean; onChanged?: () => void; onError?: (message: string) => void };
```

| Mode | Turned on by | Value shown | On pick |
|---|---|---|---|
| Runtime (task sessions) | `mode` omitted or `"runtime"` | from `live ? runtime : effective_runtime` | confirm → `POST /api/v1/sessions/{id}/runtime` → `onChanged` |
| View-only (project sessions) | `mode: "view"` | `view` | `chooseView(next)` |

The HTTP API is unchanged.
