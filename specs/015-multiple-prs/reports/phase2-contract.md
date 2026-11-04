# Phase 2 contract review (spec 015, daemon HTTP API vs app/src/api.ts)

[OK] POST /tasks/{key}/autofix: autofix/api.rs:11-18 <-> app/src/api.ts:1005
`repo` still accepted (`#[serde(default)] Option<String>`), `pr: Option<i64>` is optional and wins when both are sent. Old body `{action, repo}` resolves to the first counted PR of that repo (autofix/mod.rs:1000). The TS client omits absent keys.
Note (LOW): contract says "Both absent: as today (all PRs)". That holds for on/off. For `retry` with neither, the daemon answers 422 (mod.rs:1003-1009). That was most likely already the case (retry needed `repo`); update the contract wording. `pr` may also select a replaced PR (it searches `rows`, not `counted`), which is harmless.

[OK] GET /tasks/{key}/prs: pr_api/dto.rs:38-45,217-223 <-> api.ts:795-800
`replaced_by` and `head_branch` are always emitted (null when empty) and are optional/nullable in TS. `linked_by` is a plain String and the TS union adds "agent". An old app that narrows `linked_by` would fall into its default branch, which is acceptable. Sorting is done in mod.rs:114 (only the order changes).

[OK] GET /inbox: inbox.rs:284-286,871 <-> api.ts:728
`pr_url` is skip_serializing_if None and optional in TS, as the contract says.

[RISK - MEDIUM] Inbox "retry" does not use the new field: app/src/components/InboxView.tsx:502 calls `api.setAutofix(item.task, "retry", item.repo)`.
Problem: with two PRs in one repo, retry from the inbox always hits the first counted PR of that repo, not the PR the notice is about. `pr_url` is exposed but nothing consumes it, and the inbox item carries no PR id. This matches the contract's accepted break, but it is a functional gap in the phase.
Fix: let the retry path accept a URL or add `pr_id` to the item (or look up the PR id by `pr_url` from the prs list in the UI), then pass `pr`.

[RISK - LOW] Notices store `pr_url`; consumers match on it. forward.rs:267 and inbox.rs:778 now match `p.url == n.pr_url` instead of repo.
Problem: notices with an empty `pr_url` (task-level kinds, or PR-kind notices inserted through the old `insert_notice` path) no longer match any PR. Forward then sends an empty URL; the inbox skips the item unless archive. Migration 0027 backfills PR-kind notices and resolves orphans, so existing data is fine. New PR-kind notices must always go through `insert_notice_for` with a URL.
Fix: confirm no non-test caller still uses `insert_notice` for ci-failed/changes-requested/comments/pr-opened. Add a fallback to match by repo when `pr_url` is empty.

[OK] Migration 0027 (producer-side data): UNIQUE (task_key, url) and notice pr_url are consistent with the DTOs. It copies ids explicitly, so PR ids that clients hold stay valid. Rebuilding both tables in one migration is not atomic only if sqlx lacks a transaction; sqlx wraps migrations in one.

[NOTE] Not covered by these files: link/proposal/open-PR routes and PrSummary (`count`, no extra fields), which are additive per the contract and which I did not diff in depth. Tests exist for the new DTO fields, the retry by `pr` id, the inbox `pr_url` and the forward-by-URL path. There is no test that an old body `{action:"retry", repo}` still works with two PRs in the repo, and none that the old `{action:"retry"}` payload shape is unchanged. Consider adding them.

No CRITICAL or HIGH findings: 1 MEDIUM, 2 LOW.

VERDICT: ⚠️ NEEDS CARE
