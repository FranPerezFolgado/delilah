# Phase 5 contract review (f2bec6a..HEAD)

Scope: open `empty_commit`, `POST .../prs/{id}/ready`, merge `confirm_live_subtasks` and the
`live-subtasks` 409, base-status `subtask_commits`, `parent-task-ready` notice.

[OK] open `empty_commit` - pr_api/dto.rs:153, pr_api/open.rs (open_with) <-> app/src/api.ts:975
Serde default false; not a parent task gives 409 `empty-commit-not-allowed` (Refusal exists in
core). The existing `no-commits` refusal still fires without the flag, so the retry flow in the
contract holds. Parent task PRs are forced to draft until progress is ready (matches comment; the
contract does not state it, doc-only gap).

[OK] ready endpoint - api.rs route, pr_api/ready.rs <-> api.ts:879 `markReady`
Returns PrView like merge. 404 for an unknown PR, 409 `not-open` (reason), 502 with a classified
reason when gh fails. Test covers the call and the 404.

[OK] merge `confirm_live_subtasks` - pr_api/merge.rs <-> api.ts:869
Default false, so old clients still work. Only parent tasks are checked; 409 `error: live-subtasks`
with `subtasks: string[]`, as in the contract. The check runs before the gh account check, so no
gh call is made on refusal.

[OK] base-status - base_update.rs BaseStatus <-> api.ts:787
`subtask_commits` is always serialized (u32); the TS field is optional, which is tolerant of old
daemons. update-branch merges own branch first, then base, undoing both on conflict (R9).

[OK] notice `parent-task-ready` - pr_loop/merge.rs; inbox.rs already maps the kind to
ItemKind::ParentTaskReady (inbox.rs:229). Deduplicated by cause `pr <url>`.

[RISK, low] `no-commits` carries `reason`, not `error`, unlike the other refusals; contract table
lists it as a 409 refusal. Pre-existing shape; check the UI reads `reason` for it.
[RISK, low] empty_commit commits in the repo returned by `source_of`, while the proposal picks the
first repo with commits. Fine for parent tasks (single repo), but not stated.

No CRITICAL or HIGH findings.

VERDICT: COMPATIBLE
