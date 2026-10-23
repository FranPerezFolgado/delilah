# Contracts: Task branches start from a fresh default branch

All changes are additive; old clients keep working.

## `GET /api/v1/inbox`

`InboxItem` (`crates/delilah-daemon/src/inbox.rs` ↔ `app/src/api.ts`):

- `kind`: new value `"base-stale"` (Rust `ItemKind::BaseStale`, kebab-case like
  the others). An old client that does not know it shows nothing special; the
  type in `api.ts` gains it in `InboxKind`.
- New optional field `notice?: number`: the id of the `pr_notices` row, present on
  `base-stale` items (skipped when `None`). `pr` stays absent on them.

## `POST /api/v1/pr-notices/{id}/dismiss` (unchanged)

Also dismisses `base-stale` notices: 204, emits `inbox`.

## `POST /api/v1/tasks` / backlog start (unchanged)

Request and response are the same. The start may take up to 30 s longer when a
remote does not answer. Errors are as today (422 `workspace: …`); a failed fetch is
never an error.

## CLI

`delilah start …` unchanged. Nothing new to print: the warning is in the Bandeja.
