# Contract: agent permissions

## HTTP: the existing generic preference route, with validation for one key

`GET /api/v1/preferences/agent-permissions`
- 200 `"auto"` | `"default"` | `null` (unset, which means auto)

`PUT /api/v1/preferences/agent-permissions`, with body `"auto"` or `"default"`
- 200: the stored value, which takes effect for the next session spawned or relaunched.
- 400 `{"error": ...}`: any other body (another string, a number, an object, `null`).

No new route. The 400 is new behaviour for this one key on the generic route. It is
documented on the route in `api.rs` and on `setPreference` in `app/src/api.ts`, the
contract pair. Other keys are unchanged. `app/src/api.ts` already has `preference<T>(key)` and
`setPreference(key, value)`. The UI adds the type `AgentPermissions = "auto" | "default"`.

## CLI arguments passed to `claude`

Task and project sessions, with the preference `auto` or unset:
- PTY: `... --settings <file> --permission-mode auto ...`
- chat: `-p --input-format stream-json --output-format stream-json --verbose
  --include-partial-messages --permission-prompt-tool stdio ... --permission-mode auto ...`

With `default`, no `--permission-mode` (today's argv). Idea conversations (`dontAsk`) and
read-only sessions (`plan`) do not depend on the preference.

## Start failure (FR-008)

If `claude` rejects `--permission-mode auto`, the session fails at start:
- chat: through the existing stream start check;
- PTY: the process exits before any work, and the session is reported failed (the
  existing failed-session path).

One function in `run.rs`, next to the spawn-failure handling, matches the CLI's
rejection text and adds the hint.

The failure message the user sees adds a hint naming the setting: «Ajustes → Permisos de
los agentes: prueba "Por defecto de Claude"». It is shown in the app's language. The spike
records the CLI's exact stderr, so the hint can be matched to it.
