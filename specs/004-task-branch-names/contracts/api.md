# API contract changes (additive only)

`crates/delilah-daemon/src/api.rs` ↔ `app/src/api.ts`.

## New: `POST /api/v1/projects/{name}/branch-name`

Proposes a branch for a task about to start. It never creates anything.

Request:

```json
{ "key": "PROJ-12", "title": "Retry failed payments", "notes": "optional", "free": false, "submodules": ["svc-payments"] }
```

- `key`: optional (empty or absent for a new free task).
- `submodules`: `parent` projects; which submodule repos to check for taken names
  (empty: none are checked yet).

Response `200`:

```json
{ "branch": "feature/PROJ-12-retry-failed-payments", "source": "claude" }
```

- `source`: `claude` | `title` | `key`. Claude failing is not an error: the answer
  falls back to `title`/`key`. `branch` is `null` only for a free task with no key whose
  title gives no slug.
- `404`: unknown project. `400`: empty title.
- Takes up to ~10 s (the Claude call); the dialog must not block on it.

## Changed: `POST /api/v1/runs` (`RunRequest.branch`)

- Still optional. Absent: Delilah proposes one (Claude, 5 s limit, then the
  fallbacks). Present: normalised (`feature/` added once, slug rules). `400` with
  "the branch name has nothing usable" when nothing is left.
- In both cases a `-N` suffix is added when the name is taken (local or remote), in
  worktree and checkout mode alike.
- `app/src/api.ts` `RunRequest` gains `branch?: string`.

## Changed: `Task`

Gains `branch?: string | null` (omitted when the task has no workspace). In every
response that returns tasks (`GET /api/v1/tasks`, `GET /api/v1/tasks/{key}`,
`POST /api/v1/runs`, events that carry a task).

## CLI

`delilah run --branch <name>`: same normalisation (no flag changes). Help text:
"Defaults to a name Claude proposes from the title, as `feature/…`".
