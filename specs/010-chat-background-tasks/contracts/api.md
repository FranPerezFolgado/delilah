# Contracts: Show background work in chat mode

All changes are additive: older clients ignore the new fields and messages; a newer client
against an older daemon sees `progress` missing and falls back to today's `activity` line.

## `GET /api/v1/sessions/{id}/conversation`

New field, `null` for a session that is not live or not in chat mode:

```jsonc
{
  "entries": [ … ],
  "activity": "Leyendo src/a.rs",        // unchanged (PTY sessions now also set it at tool start)
  "progress": {
    "turn": {                             // null between turns
      "started_at": "2026-10-26T10:00:00.000Z",
      "by": "claude",                     // "user" | "claude"
      "reason": "tarea en segundo plano terminada", // or null
      "step": "Ejecutando cargo test",    // or null
      "last_signal_at": "2026-10-26T10:00:41.120Z"
    },
    "background": [
      { "task_id": "bnksojypv", "tool_use_id": "toolu_01Rb…", "kind": "command",
        "description": "Background sleep for 25 seconds", "state": "running",
        "started_at": "2026-10-26T09:59:30.000Z", "ended_at": null }
    ],
    "subagents": [
      { "tool_use_id": "toolu_01MF…", "description": "List and read files", "state": "running",
        "started_at": "…", "ended_at": null, "steps": 3,
        "recent": [ { "tool": "Read", "summary": "Leyendo b.rs", "at": "…" } ] }
    ]
  }
}
```

`state` values: `running`, `completed`, `failed`, `stopped`, `unknown`. `kind`: `command`,
`agent`, `other`. `recent` holds at most 50 steps; `steps - recent.length` were hidden.

## Conversation entries

New marker kind:

```jsonc
{ "id": "t:<uuid>:0", "at": "…", "kind": "marker", "marker": "background-done",
  "detail": "Background command \"…\" completed (exit code 0)" }   // detail optional
```

`app/src/api.ts`: `marker: "compacted" | "stopped" | "background-done"; detail?: string`.

## `GET /api/v1/sessions/{id}/live` (WebSocket)

New message, sent when the session's progress changes, at most every 250 ms, and once right
after the socket opens:

```jsonc
{ "progress": { /* same object as the conversation's `progress` */ } }
```

Existing messages (`{message, text}`, `{message, done}`, `{reset}`) are unchanged. Text
and done messages are never produced for sub-agent messages (R2 fix).

## Session and task state (no shape change)

- A chat-mode session whose turn ended with background work running stays
  `"state": "working"`; its task stays `running`; no inbox item for that turn end.
- A self-started turn makes the session `working` and the task `running` (history
  `agent-resumed`).
- `POST /api/v1/tasks/{key}/runtime` answers **409** while a session has background work
  running ("Hay trabajo en segundo plano: espera a que termine o pulsa Detener").
- `POST /api/v1/sessions/{id}/messages`: with only background work running (no turn), the
  message is typed now, not queued.
- `POST /api/v1/sessions/{id}/stop`: also asks Claude to stop each running background item
  when the stream supports it (research R8); otherwise unchanged.
