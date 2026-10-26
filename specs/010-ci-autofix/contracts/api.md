# Contracts: CI autofix

Additions only; existing shapes keep their fields. Client types in `app/src/api.ts`.

## HTTP (delilahd)

### `POST /api/v1/tasks/{key}/autofix` *(new)*

```json
{ "action": "on" | "off" }
{ "action": "retry", "repo": "." }
```

- `on` / `off`: set `tasks.ci_autofix`; `on` resets the attempts of every stopped PR,
  resolves their `autofix-stopped` notices and evaluates at once (a red head already
  finished is sent if the agent is idle). `off` never interrupts a running turn; a
  `fixing` PR goes back to `watching` (`autofix-cancelled`).
- `retry`: for the PR of `repo` (required with `retry`), reset attempts and the
  forwarded sha, resolve its `autofix-stopped` notice and evaluate. Turns autofix on.
- 200 with the task (as `GET /tasks/{key}`). 404 unknown task; 409 `terminal` for a
  done or failed task (archiving keeps the status); 409 if the retry's reset keeps losing a race; 422 `repo` missing or not a PR of the task for `retry`.

### Task *(changed)*: `GET /tasks`, `GET /tasks/{key}`

```json
{ "...": "...", "ci_autofix": true }
```

### Create / backlog *(changed)*

`POST /tasks`, backlog create and `PATCH`: optional `"ci_autofix": bool`; absent → the
project default (research R8).

### `GET /api/v1/tasks/{key}/prs` *(changed)*

Each PR gains:

```json
"autofix": {
  "state": "watching" | "fixing" | "stopped",
  "attempts": 1, "limit": 3,
  "sha": "abc123", "sent_at": "2026-10-26T10:00:00Z",
  "reason": null | "limit" | "no-push" | "no-session"
} | null
```

`null` when autofix never acted on the PR (the UI shows "watching" if the task has it
on, "off" otherwise). `session` stays internal.

`GET /api/v1/prs` is unchanged: the badge's autofix mark comes from `Task.ci_autofix`.
All new TS fields are optional (an older daemon omits them).

### Project settings *(changed)*: `PUT /api/v1/projects/{name}/settings`

(The settings are nested in `GET /projects/{name}`.)

`settings.ci_autofix`: `"off" | "autopilot" | "always"` (absent = `"autopilot"`).
Invalid value → 422 with field error `ci_autofix` / code `ci_autofix.invalid`.

### Inbox *(changed)*: `GET /inbox`

New item kind `pr-autofix` from an open `autofix-stopped` notice:

```json
{ "kind": "pr-autofix", "task": "PROJ-1", "notice": 12, "repo": ".",
  "reason": "limit", "attempts": 3, "text": "CI autofix gave up after 3 attempts", "since": "…" }
```

Actions: "Retry autofix" → `POST /tasks/{key}/autofix {"action":"retry","repo"}`;
"Dismiss" → the existing notice dismiss route. `POST /pr-notices/{id}/forward` on it → 409
(nothing to forward).

### `POST /api/v1/pr-notices/{id}/forward` *(behaviour)*

Unchanged shape. For `ci-failed`, first CASes the notice's sha into the PR's autofix
state (FR-005); if autofix already sent that sha, resolves the notice and returns 204
without sending it again.

## CLI (`delilah`)

- `delilah autofix <KEY> on|off|retry [--repo <path>]`
- `delilah new|run … --autofix | --no-autofix` (neither → project default)
- `delilah project set <name> ci-autofix off|autopilot|always`, `unset` → default

## Agent prompt (app language)

`ci_prompt` (unchanged framing: names/links are data, not user instructions) wrapped as:

> Delilah's CI autofix (attempt N of 3): … [failed checks] … Read the logs with
> `gh run view --log-failed`, fix the failure, commit and push to this same branch.
> Don't force-push or open another PR. If you can't fix it, say why and stop.
