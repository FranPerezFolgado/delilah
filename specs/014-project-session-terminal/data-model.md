# Data model: Project Claude sessions open in the terminal

No persisted data changes (no migration, no API field).

## UI state: per-session view (`TaskScreen`)

| Field | Type | Notes |
|---|---|---|
| `views` | `Map<sessionId, "chat" \| "terminal">` | existing; set by the switch or by ConversationView's "open the terminal"; dropped when the screen unmounts |

Resolution of a project session's view (`viewOf`):

1. `kind === "shell"` → `terminal` (no switch)
2. `runtime === "stream"` → `chat` (not expected on projects; kept)
3. `views.get(id)` if set
4. otherwise `conversation ? "chat" : "terminal"` ← **changed** (was always `chat`)

Task sessions: unchanged (the view follows the runtime / effective runtime).
