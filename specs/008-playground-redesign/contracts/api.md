# Contracts: daemon HTTP API ↔ client (playground redesign)

Both changes are additive; old clients keep working.

## `GET /api/v1/activity` (changed)

```json
{
  "sessions": { "<session id>": { "action": null, "helpers": [] } },
  "tokens":   { "<session id>": { "total": 1234567, "input": 1000, "output": 30000,
                                  "cache_write": 1203567, "cache_read": 9000000 } }
}
```

- `tokens` lists live Claude sessions whose transcript has been read; absent → no stack.
- Emitted on topic `activity` when a session's `total` changes (at most once a second
  per session, the watcher's tick).

## `GET /api/v1/activity/history?owner=<owner>&after=<seq>` (new)

- `owner`: task key or `@project` (URL-encoded). Required; unknown owner → `{ "sessions": [] }`.
- `after`: optional u64; only entries with `seq > after`. Absent → all kept entries.
- 400 when `after` is not a number or `owner` is missing.

```json
{
  "started_at": "2026-10-26T09:00:00Z",
  "sessions": [
    {
      "session": "s-1", "live": true, "dropped": false,
      "entries": [
        { "seq": 41, "at": "…", "type": "tool_started", "agent": null,
          "action": { "id": "toolu_1", "kind": "edit", "tool": "Edit",
                      "target": "api.rs", "background": false, "since": "…" } },
        { "seq": 42, "at": "…", "type": "helper_started",
          "agent": { "id": "a1b2", "role": "investigar" } },
        { "seq": 43, "at": "…", "type": "tool_ended", "agent": null,
          "id": "toolu_1", "failed": false },
        { "seq": 44, "at": "…", "type": "helper_ended", "id": "a1b2" },
        { "seq": 45, "at": "…", "type": "turn_over" }
      ]
    }
  ]
}
```

- Sessions are ordered by first entry; entries oldest first. `target` is already cut to
  40 characters (never more than the captions show, FR-023).
- New entries emit topic `activity`; clients refetch with `after` = last seen `seq`.

## Client (`app/src/api.ts`)

- `api.activity(): Promise<{ sessions; tokens }>` (type widened).
- `api.activityHistory(owner, after?)`.
- `TaskTabItem.kind` gains `"office"`; `App.openTask(owner, target?)` where `target` is a
  session id or a tab id (`OFFICE`).
