# Contracts: Track every PR a task opens

Shapes are additive and the routes keep their paths. Two deliberate behaviour breaks
for an old app (accepted: the app ships with the daemon): Link by URL no longer answers
409 for a PR of another repo, and an app that keys cards or autofix by `repo` acts on
the first PR when a repo has two.

## `GET /api/v1/tasks/{key}/prs` → `TaskPrs`

`prs[]` may now hold several PRs with the same `repo`, and PRs whose `repo` is
`host/owner/name`. Each `PullRequest` adds:

```jsonc
{
  "linked_by": "detected" | "opened" | "pasted" | "agent",   // "agent" is new
  "replaced_by": 42 | null,          // id of the PR that replaced this closed one
  "head_branch": "feature/x" | null  // from the snapshot
}
```

Order: by `repo` (`.` first, then submodule paths, then outside repos), then by id.

## `POST /api/v1/tasks/{key}/prs/link` `{ "url": "…" }`

- Before: 409 when the PR is not from a repo of the task's workspace.
- Now: links it; `repo` is the workspace label when its remote matches a repo of
  `pr_repos`, else `host/owner/name`. Still 400 for a URL that is not a PR, 409 when `gh`
  cannot read it.

## `GET /api/v1/tasks/{key}/prs/proposal` → `PrProposal`

`repos` now includes `.` for a parent task whose own repo is on the task's branch, when
that repo has no open PR from the task's branch.

## `POST /api/v1/tasks/{key}/prs` (open) `{ "repo": "." | "<submodule>", … }`

Accepts `.` for a parent task under the same condition. Unknown repo → 400 as today.

## `POST /api/v1/tasks/{key}/autofix` `{ "action", "repo"?, "pr"? }`

New optional `pr: <id>` selects the PR. `repo` alone is kept for old clients: the first
counted open PR of that repo. Both absent: as today (all of the task's PRs).

## `GET /api/v1/inbox`

`pr-autofix` items add an optional `pr_url` (the PR the notice is about; omitted when empty).

## Summaries (`PrSummary`, board/sidebar)

Computed over counted PRs (`replaced_by` null); `count` is how many. A task with only
replaced PRs has no badge.

## Hook input (internal, Claude Code → daemon)

`PostToolUse` with `tool_name: "Bash"`, `tool_input.command` containing `gh pr create`:
`tool_response.stdout` / `tool_response.stderr` are scanned for PR URLs. No change to the
hook settings file: `PostToolUse` is already forwarded.

## TypeScript (`app/src/api.ts`)

```ts
export interface PullRequest {
  // …existing
  linked_by: "detected" | "opened" | "pasted" | "agent";
  replaced_by?: number | null;
  head_branch?: string | null;
}
setAutofix: (key: string, action: "on" | "off" | "retry", repo?: string, pr?: number) => …
```
