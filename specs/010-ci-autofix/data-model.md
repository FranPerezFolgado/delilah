# Data model: CI autofix

## Migration `0022_ci_autofix.sql`

```sql
-- CI autofix of the task: Delilah sends its PRs' CI failures to the agent on its own.
ALTER TABLE tasks ADD COLUMN ci_autofix INTEGER NOT NULL DEFAULT 0;
-- Autofix state of the PR, as JSON; NULL until autofix first acts on it.
ALTER TABLE pull_requests ADD COLUMN autofix TEXT;
-- Live tasks already on autopilot get it, as the project default "autopilot" would…
UPDATE tasks SET ci_autofix = 1
 WHERE autopilot IS NOT NULL AND status NOT IN ('done', 'failed'); -- check how `status` is encoded (name or JSON) and how archived tasks are marked; match it
-- …without firing at a failure from before the upgrade: their PRs' current head counts
-- as already forwarded.
UPDATE pull_requests
   SET autofix = json_object('state', 'watching', 'attempts', 0,
                             'sha', json_extract(snapshot, '$.head'))
 WHERE snapshot IS NOT NULL
   AND task_key IN (SELECT key FROM tasks WHERE ci_autofix = 1);
```

## `ProjectSettings.ci_autofix` (delilah-core, `projects.settings` JSON)

| Value | Meaning |
|---|---|
| `"off"` | new tasks start with autofix off |
| `"autopilot"` (default, also when absent) | on for tasks created with autopilot |
| `"always"` | on for every new task |

## `Task.ci_autofix: bool` (delilah-core)

Set at creation from the request or the project default (research R8); changed by
`POST /tasks/{key}/autofix`.

## `PrAutofix` (delilah-daemon `autofix/`; `pull_requests.autofix`; only a DTO reaches the API)

```json
{
  "state": "watching" | "fixing" | "stopped",
  "attempts": 0,            // attempts since the last green head (0..=3)
  "sha": "abc123",          // last head forwarded (by autofix or by hand); never sent again
  "session": "s-…",         // session the last attempt went to
  "sent_at": "RFC 3339",    // when the last attempt was sent
  "reason": "limit" | "no-push" | "no-session" | null   // when stopped
}
```

Constant `LIMIT = 3`.

### Transitions (only while `task.ci_autofix` and the PR is open)

| From | Event | To | Side effect |
|---|---|---|---|
| none/`watching` | head red, all finished, `head != sha`, idle, attempts < 3 | `fixing` (attempts+1, sha=head) | send prompt; resolve `ci-failed`; history `autofix-sent` |
| `fixing` | head changed and green | `watching` (attempts=0) | history `autofix-fixed` |
| `fixing` | head changed, red, finished, idle, attempts < 3 | `fixing` (attempts+1) | send; history `autofix-failed-again` + `autofix-sent` |
| `fixing`/`watching` | head red, finished, attempts == 3 | `stopped` (`limit`) | `autofix-stopped` notice; notification; history |
| `fixing` | primary turn ended, forced check, head == sha | `stopped` (`no-push`) | same |
| any | send impossible (no session) | `stopped` (`no-session`) | same |
| `watching`/`stopped` | head green | `watching` (attempts=0) | resolve `autofix-stopped` |
| `stopped` | retry / toggle on | `watching` (attempts=0, sha kept unless retry) | resolve notice; evaluate (retry clears `sha` so the current failure is sent) |
| `fixing` | toggle off, PR merged/closed, task archived/deleted/backlog | `watching` | history `autofix-cancelled`; resolve `autofix-stopped` |
| `fixing` | send failed (not no-session) | previous value | none (the attempt doesn't count) |
| — | manual forward of `ci-failed` for sha X | `sha = X` (CAS; created from NULL) | if `sha` was already X: resolve, don't send |

The rows of one task are evaluated together under the task's `AutofixGuard`; all rows
eligible to send go in one prompt (one section per PR) and are CASed before the send.
`no-push` is judged only at a turn end of `session` that started after `sent_at`, from a
snapshot checked after that turn end.

Every write: `Store::update_pr_autofix(pr_id, expected: Option<PrAutofix>, next)` →
`Ok(false)` when the row changed meanwhile (the caller re-reads and re-decides).

## `pr_notices` kind `autofix-stopped`

`cause = "<sha>:<reason>"`, `detail = {"reason", "attempts", "checks": [...]}`. Inbox
item kind `pr-autofix` (with `pr-failed`/`pr-review`/`pr-blocked`), actions "Retry
autofix" (`POST /tasks/{key}/autofix {"action":"retry","repo"}`) and "Dismiss".

## History events (task history: new `Event` variants in delilah-core, as autopilot's)

`autofix-sent` (repo, sha, attempt, checks), `autofix-fixed`, `autofix-failed-again`,
`autofix-stopped` (reason), `autofix-cancelled`.
