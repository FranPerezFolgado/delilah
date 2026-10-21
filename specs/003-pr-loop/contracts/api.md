# Contracts: PR loop

All new; nothing existing changes shape except the additions marked *(changed)*.
Client types live in `app/src/api.ts`.

## HTTP (delilahd)

### `GET /api/v1/tasks/{key}/prs`

```json
{
  "account": "FranPerezFolgado",
  "problem": null,
  "prs": [
    {
      "id": 7, "repo": ".", "host": "github.com", "owner_name": "me/repo",
      "number": 42, "url": "https://github.com/me/repo/pull/42", "linked_by": "detected",
      "checked_at": "2026-10-20T10:00:00Z", "error": null,
      "snapshot": {
        "state": "open", "draft": false, "title": "…", "head": "abc123", "base": "main",
        "checks": [{ "name": "test", "outcome": "failure", "url": "https://…" }],
        "ci": "failure",
        "review_decision": "changes-requested",
        "comments": 3
      }
    }
  ]
}
```

- `problem`: `{ "kind": "missing" | "signed-out" | "no-access" | "unsupported-remote" | "network" | "rate-limited", "message": "…" }` or null.
- `snapshot.comments` is a count here; the full comments travel only in notices.
- 404 for an unknown task.

### `GET /api/v1/prs`

Summaries for badges, one per task with linked PRs:

```json
[{ "task": "PROJ-1", "state": "open", "draft": false, "ci": "success", "review_decision": "approved", "count": 1 }]
```

For a changeset: the worst of each field (`failure` > `pending` > `success`;
`open` while any PR is open), `count` = number of PRs.

### `POST /api/v1/tasks/{key}/prs/refresh`

Checks the task's PRs (and detects missing ones) now, ignoring back-off. 202; the
result arrives through the `prs` topic.

### `POST /api/v1/tasks/{key}/prs/link`

```json
{ "url": "https://github.com/me/repo/pull/42" }
```

200 + the PR row. 400 for a URL that isn't a PR; 409 with the `gh` message if it
can't be read. The repo is matched to one of the workspace's repos by owner/name,
else `.`. Removes the URL from `pr_unlinked`.

### `DELETE /api/v1/tasks/{key}/prs/{id}`

Unlinks: deletes the row, resolves its notices, adds the URL to `pr_unlinked`. 204.

### `GET /api/v1/tasks/{key}/prs/proposal?repo=.`

```json
{ "repo": ".", "title": "…", "body": "Jira: https://…/browse/PROJ-1\n\n- commit subject\n- …", "base": "main", "needs_push": true }
```

`repos` *(added during implementation)*: the task's repos (`.` or submodule paths) that
have no open PR, so the dialog can offer a submodule picker for parent projects:
`"repos": ["svc-a", "svc-b"]`.

409 `{ "error": "…", "reason": "no-commits" | "has-pr" | "no-workspace" }`.

### `POST /api/v1/tasks/{key}/prs`

```json
{ "repo": ".", "title": "…", "body": "…", "draft": false }
```

Pushes if needed, creates the PR, links it (`linked_by: "opened"`), moves the phase to
review. 201 + the PR row. 409 with git's or `gh`'s stderr; nothing linked.

### `POST /api/v1/pr-notices/{id}/forward`

Sends the notice to the task's agent and resolves it. 204. 409 when the task is
paused by the limit (`"resume_at"` in the body) or has no session to reopen.

### `POST /api/v1/pr-notices/{id}/dismiss`

Resolves it. 204.

### `GET /api/v1/inbox` *(changed)*

New `kind` values `pr-failed`, `pr-review`, `pr-blocked`, and a new optional field:

```json
{ "kind": "pr-failed", "task": "PROJ-1", "title": "…", "text": "CI falló: test, lint", "since": "…",
  "pr": { "notice": 12, "url": "https://github.com/me/repo/pull/42", "repo": "." } }
```

`pr-blocked` items offer a forced archive (`POST /tasks/{key}/archive?force=true`,
unchanged).

### `POST /api/v1/tasks/{key}/archive` *(changed behaviour only)*

For a task with an `archive-partial` notice, finishes the remaining steps (branch
deletion) and resolves the notice.

### `GET /api/v1/events` *(changed)*

New topic `{"type":"prs"}`: a task's PRs or their summaries changed.

## App (`app/src/api.ts`)

```ts
export type InboxKind = … | "pr-failed" | "pr-review" | "pr-blocked";
export const BLOCKING: InboxKind[] = [..., "pr-failed", "pr-review", "pr-blocked"];
export interface PrCheck { name: string; outcome: "pending" | "success" | "failure" | "neutral"; url: string | null }
export interface PrSnapshot { state: "open" | "merged" | "closed"; draft: boolean; title: string; head: string; base: string;
  checks: PrCheck[]; ci: "pending" | "success" | "failure" | "none"; review_decision: "approved" | "changes-requested" | "review-required" | null; comments: number }
export interface PullRequest { id: number; repo: string; host: string; owner_name: string; number: number; url: string;
  linked_by: "detected" | "opened" | "pasted"; checked_at: string | null; error: string | null; snapshot: PrSnapshot | null }
export interface TaskPrs { account: string | null; problem: { kind: string; message: string } | null; prs: PullRequest[] }
export interface PrSummary { task: string; state: PrSnapshot["state"]; draft: boolean; ci: PrSnapshot["ci"]; review_decision: PrSnapshot["review_decision"]; count: number }
export interface PrProposal { repo: string; title: string; body: string; base: string; needs_push: boolean; repos: string[] }
// api.taskPrs(key), api.prSummaries(), api.refreshPrs(key), api.linkPr(key, url), api.unlinkPr(key, id),
// api.prProposal(key, repo), api.openPr(key, body), api.forwardNotice(id), api.dismissNotice(id)
```
