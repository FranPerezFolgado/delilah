# Contracts: Agent playground

## Hook settings (adapter → Claude Code)

`delilah-agents/src/claude.rs::settings()` changes in three ways:
- `HOOK_EVENTS` gains `PostToolUseFailure`, `SubagentStart` and `SubagentStop`.
- `PreToolUse` gets **two** entries: the existing `AskUserQuestion` one (to
  `/hooks/claude/ask`, which blocks until the user answers), and a new `"*"` one to
  `/hooks/claude`. The new one is non-blocking and uses the same `post_command` as the
  other events, so it never prints a decision.

`classify()` keeps returning `Signal::Other` for `PreToolUse`, `PostToolUseFailure` and
`Subagent*`, so the session state is unchanged. A new pure
`classify_activity(&Value) -> Option<ActivityEvent>` reads them:

```rust
pub enum ActivityEvent {
    ToolStarted { agent: Option<AgentRef>, action: Action },
    ToolEnded   { agent: Option<AgentRef>, id: String },
    HelperStarted { agent: AgentRef },
    HelperEnded   { id: String },
    TurnOver, // Stop, StopFailure, SessionEnd of the main agent
}
pub struct AgentRef { pub id: String, pub role: String }
```

## HTTP (delilahd)

### `GET /api/v1/activity` (new)

Returns the activity of every live Claude session. Shell sessions and sessions with
nothing going on are left out.

```json
{
  "sessions": {
    "c7801c13-…": {
      "action": { "id": "toolu_01…", "kind": "edit", "tool": "Edit", "target": "api.rs", "background": false, "since": "2026-10-20T10:00:00Z" },
      "helpers": [
        { "id": "a84f…", "role": "code-researcher", "since": "…",
          "action": { "id": "toolu_02…", "kind": "read", "tool": "Grep", "target": "LiveState", "background": false, "since": "…" } }
      ]
    }
  }
}
```

- `action` is `null` when there is none, and `helpers` is `[]` when empty.
- `kind` is kebab-case, matching the style of `LiveState`.
- The types go in `app/src/api.ts` as `Activity`, `Action`, `Helper` and `ActionKind`,
  with `api.activity()`.

### WebSocket `/api/v1/events`

- A new topic `activity`, emitted when any session's activity changes.
- `live.ts` adds `"activity"` to `Topic`.
- Existing topics are unchanged. Older UIs ignore the unknown topic.

## UI ↔ shell (separate window)

| From | Mechanism (desktop) | Mechanism (browser preview) | Payload |
|---|---|---|---|
| main → open Playground window | `new WebviewWindow("playground", { url: "index.html#playground" })`, or focus the existing one | `window.open(url + "#playground", "delilah-playground")` | n/a |
| Playground window → main: open a task | `emitTo("main", "delilah:open-task", p)`, then show, unminimise and focus `main` | `BroadcastChannel("delilah").postMessage({ type: "open-task", ...p })` | `{ task: string, session: string }` |

The capability (`app/src-tauri/capabilities/default.json`) changes in two ways:
- `"windows": ["main", "playground"]`;
- new permissions `core:webview:allow-create-webview-window`,
  `core:window:allow-set-focus`, `core:window:allow-show`, `core:window:allow-unminimize`
  and `core:event:default` (if not already in `core:default`).
