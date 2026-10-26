# Contracts: Readable background-task notifications

All changes are additive. `GET /api/v1/sessions/{id}/conversation` keeps its shape; only
the `background-done` marker gains an optional field, and user lines that used to leak
markup now come back as markers.

## Conversation entry: `background-done` marker

Before (main, unreleased):

```json
{ "id": "t:<uuid>:0", "at": "…", "kind": "marker", "marker": "background-done",
  "detail": "Agent \"WP-07 update status.md\" finished" }
```

After:

```json
{ "id": "t:<uuid>:0", "at": "…", "kind": "marker", "marker": "background-done",
  "detail": "Agent \"WP-07 update status.md\" finished",
  "background": {
    "source": "notification",
    "task_id": "ad1c30dd1649f3fe6",
    "kind": "agent",
    "label": "WP-07 update status.md",
    "status": "completed",
    "duration_ms": 89665,
    "tool_uses": 2,
    "tokens": 12317,
    "report": "## WP-07 — done\nT016 ✓. …"
  } }
```

Command:

```json
"background": { "source": "notification", "task_id": "bbepddag3", "kind": "command",
  "label": "Sleep 25 seconds in background", "status": "completed", "exit_code": 0 }
```

Report delivered as a hand-back:

```json
"background": { "source": "notification", "task_id": "a97d…", "kind": "agent",
  "label": "Map code for mobile access", "status": "completed",
  "report_in_handback": true, "duration_ms": 75040, "tool_uses": 28, "tokens": 45339 }
```

Hand-back (new: previously hidden):

```json
{ "id": "t:<uuid>:0", "at": "…", "kind": "marker", "marker": "background-done",
  "detail": "code-researcher",
  "background": { "source": "handback", "task_id": "a97d…", "kind": "agent",
    "label": "code-researcher", "status": "completed", "report": "## Map for … " } }
```

Rules:

- Optional fields absent when unknown; `report_truncated` / `report_in_handback` only when true.
- `status` is passed as read (lowercase); clients must accept any string.
- `report` is plain Markdown text, max 16 KB; clients render it with the same Markdown
  safety rules as Claude's messages.

### TypeScript (`app/src/api.ts`)

```ts
export type BackgroundInfo = {
  source: "notification" | "handback";
  task_id?: string;
  kind: "agent" | "command" | "task";
  label?: string;
  status?: string;
  exit_code?: number;
  duration_ms?: number;
  tool_uses?: number;
  tokens?: number;
  report?: string;
  report_truncated?: boolean;
  report_in_handback?: boolean;
};
// marker entry gains: background?: BackgroundInfo
```

## Compatibility

- **Old UI, new daemon**: shows "Tarea en segundo plano terminada · <detail>" for each
  marker, including the new hand-back markers (one extra line per hand-back). No errors.
- **New UI, old daemon (v1.1.1)**: no `background` field → the new item falls back to the
  generic line built from `detail` (FR-012). Raw XML from v1.1.1 transcripts read by v1.1.1
  is a daemon issue and is fixed by upgrading the daemon.
- `UserPromptSubmit` handling: prompts that contain `<task-notification>` or start with
  the hand-back sentence are agent prompts (not the user's last message). Internal only.
