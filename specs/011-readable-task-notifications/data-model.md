# Data model: Readable background-task notifications

No database changes. Everything is derived from the transcript on each read.

## BackgroundInfo (new, `delilah-agents::transcript`)

Carried by `Entry::Marker` when `marker == BackgroundDone`, serialized as `background`.

| Field | Type | From | Notes |
|---|---|---|---|
| `source` | `"notification"` \| `"handback"` | line shape | |
| `task_id` | `string \| null` | `<task-id>` / `origin.from` | key for the UI fold |
| `kind` | `"agent"` \| `"command"` \| `"task"` | summary wording (R6); `agent` for hand-backs | |
| `label` | `string \| null` | quoted part of the summary, else its first line; `origin.name` for hand-backs | ≤ 200 chars |
| `status` | `string \| null` | `<status>`, lowercased; `"completed"` for hand-backs | not validated |
| `exit_code` | `int \| null` | `(exit code N)` in the summary | commands only |
| `duration_ms` | `u64 \| null` | `<usage><duration_ms>` | |
| `tool_uses` | `u64 \| null` | `<usage><tool_uses>` | |
| `tokens` | `u64 \| null` | `<usage><subagent_tokens>` | |
| `report` | `string \| null` | `<result>` (entities decoded) or hand-back body (preamble and indent removed) | null for the placeholder |
| `report_truncated` | `bool` | | cut at `MAX_REPORT` = 16 KB |
| `report_in_handback` | `bool` | `<result>` is the "delivered to you as a message" placeholder | |

`tool-use-id`, `output-file` and `note` are not kept (FR-010).

Null / false fields are omitted from JSON (`skip_serializing_if`).

## Entry::Marker (changed)

```text
Marker { id, at, marker, detail: Option<String>, background: Option<BackgroundInfo> }
```

`detail` keeps its current meaning (first summary line) for old clients; for a hand-back
it is `"<agent name>"`.

## Ids

One user line can yield several entries (R3): block `n`, segment `k` →
`t:<uuid>:<n>` for `k == 0`, `t:<uuid>:<n>:<k>` after. Hand-back items use `t:<uuid>:0`.

## UI: folded background item (`app/src/backgroundItems.ts`)

`foldBackground(entries: ConversationEntry[]): ConversationEntry[]`, pure, recomputed on
every render from the raw (merged) entries and never stored, so pages that arrive later
complete a pair without anything to undo:

- a notification with `report_in_handback` takes the report (and `report_truncated`) of
  the nearest later-or-earlier hand-back entry with the same `task_id` that is not yet
  used; that hand-back is removed from the list;
- the merged entry keeps the notification's id, time and metrics;
- unmatched hand-backs and notifications are returned unchanged (the item shows
  "Cargando informe…" while the conversation is not `complete`, "Informe no disponible"
  once it is);
- every other entry passes through in order.
