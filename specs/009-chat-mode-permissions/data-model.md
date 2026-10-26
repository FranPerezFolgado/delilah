# Data model: Chat mode asks for permission as often as terminal mode

No migration: one preference row in the existing `settings` table (created back in
migration 0004, untouched).

## Preference `agent-permissions`

| Field | Value |
|---|---|
| Row key | `pref:agent-permissions` (namespaced by `preferences::setting_key`) |
| Value | JSON string: `"auto"` or `"default"` |
| Unset | treated as `"auto"` |
| Written by | `PUT /api/v1/preferences/agent-permissions`. Any other value is refused with 400 and the row is left as it was. |
| Read by | `AppState::load_agent_permissions()`, at daemon start and after each PUT. It is held in memory as an `AgentPermissions { Auto, Default }` enum, defined in `state.rs` and stored in an `Arc<AtomicU8>` (or `RwLock`; `remote_control`'s `AtomicBool` does not fit two variants). |
| Applies to | the next Claude session that is spawned or relaunched. Sessions already running keep their mode. |

## `claude::Permission` (delilah-agents)

| Variant | Flags | Used for |
|---|---|---|
| `Default` | none | task and project sessions when the preference is `default` |
| `Auto` *(new)* | `--permission-mode auto` | task and project sessions when the preference is `auto` (the default) |
| `Plan` | `--permission-mode plan` | read-only sessions, unchanged |
| `DontAsk` | `--permission-mode dontAsk --setting-sources user --strict-mcp-config` | idea conversations, unchanged |

## Session kind → permission (`run::launch_flags`)

| Session | Preference `auto` | Preference `default` |
|---|---|---|
| Idea conversation | DontAsk | DontAsk |
| Read-only | Plan | Plan |
| Task session (PTY or chat) | **Auto** | Default |
| Project session (PTY) | **Auto** | Default |

## Fake agent scenario (`delilah-e2e`), test-only

`Tool.auto: "allow" | "ask"` (default `"allow"`) is consulted only when the fake was launched
with `--permission-mode auto`:

- `allow`: run the tool, no `can_use_tool`;
- `ask`: send `can_use_tool` as today.

These semantics mirror the spike's table for the recorded CLI version. If the spike shows
that escalations arrive as `permission_denied` and not as an ask, `ask` is replaced by
what was observed.

Launched without that mode, the fake behaves as today and asks for every tool where `ask`
is true.
