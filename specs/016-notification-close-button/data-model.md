# Data model: Close button on every notification

No persisted data. One piece of client state.

## Dismissal (in memory, per component)

| Field | Type | Meaning |
|---|---|---|
| id | string | which notice (e.g. `usage:five_hour`, `pause:LIBRE-3`, `more`, `""` for a single notice) |
| sig | string | what the notice reported when it was closed (e.g. `hot`, `full`, `<reason>\|<since>`, `downloading:1.4.0`) |

Rules:

- `hidden(id)` ⇔ an entry exists and `entry.sig === current[id]`.
- `dismiss(id)` stores `current[id]` for `id`.
- After every render where `current` changed, entries with `id ∉ current` or `sig ≠ current[id]` are deleted.
- A notice with signature `null` (`useDismissal`) is not showing: its entry is deleted.

State transitions for one notice:

```text
shown --X--> hidden(sig=A)
hidden(sig=A) --current sig becomes B or notice disappears--> shown (entry deleted)
hidden(sig=A) --re-render with sig A--> hidden
```

## Existing persisted dismissal (unchanged)

Base-stale notices keep `api.dismissNotice(id)` (daemon); the X calls it instead of the old "Descartar" button.
