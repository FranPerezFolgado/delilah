# Phase 2 architecture review (per-PR-URL keying, notices by pr_url, autofix per PR)

Verdict: sound. No CRITICAL or HIGH findings. 3 MEDIUM, 3 LOW.

The shape is consistent: Store keeps all SQL, `counted()` is the single filter for replaced
rows, autofix gains `cancel_pr`/`retry_pr` next to the repo-scoped versions, and no
repo-wide resolver is left that could clobber a sibling PR's notices (only base_update
and a test call `resolve_notices_of`). The migration is correct: no indexes or FKs point
at either table, columns are listed explicitly, ids are preserved.

[MEDIUM] crates/delilah-daemon/src/store_prs.rs:~480 (insert_notice)
Problem: `insert_notice` silently writes pr_url = '' for any kind, so a PR-kind notice
inserted through it can never be resolved or matched per PR (resolve_notices_of_pr, forward,
inbox all match on url). The safe/unsafe split rests on caller discipline.
Fix: rename to `insert_task_notice` and debug_assert the kind is not one of
ci-failed/changes-requested/comments/pr-opened/autofix-stopped, or move the PR-kind set
to one const shared with the migration doc and the inbox/forward code.

[MEDIUM] crates/delilah-daemon/migrations/0027_pr_per_url.sql + autofix/mod.rs resolve_stopped
Problem: the migration only assigns pr_url to four kinds, so existing `autofix-stopped`
notices keep ''. `resolve_stopped` then carries a permanent legacy fallback
(`resolve_notices_of_pr(.., "", ..)`) that resolves every legacy '' stopped notice of the
repo, including those of sibling PRs, forever, and the inbox/forward lookups
(`p.url == n.pr_url`) lose the PR for those items.
Fix: add 'autofix-stopped' to the migration's PR-kind list (it is resolvable to the same
(task, repo) PR at that point, only one exists per repo), then drop the fallback.

[MEDIUM] crates/delilah-daemon/src/prs.rs:summary
Problem: `summary` now clones all rows (`counted(..).cloned().collect()`) just to
filter; it runs on every list/badge. Also the "counted" rule is now enforced in
summary, all_merged and retry but not in sort_for_display or pr_api list, so the
contract of what clients see for replaced rows is implicit.
Fix: make summary iterate `counted(rows)` directly (helpers take `&[&PrRow]` or an
iterator) and document in pr_api whether replaced rows are listed (flagged) or hidden.

[LOW] store_prs.rs / pr_types.rs: `replaced_by` is read, filtered and reset, but nothing
in non-test code sets it in this phase (integration.rs already filters on it). Fine if a
later package writes it; make sure that package is in plan.md, otherwise it is dead schema.

[LOW] pr_types.rs / gh/graphql.rs: `head_branch` added to Snapshot in this diff; it is
unrelated to keying. Confirm it is used by the later "replaced" detection, else defer.

[LOW] autofix/api.rs: `retry` by `repo` now picks the first counted PR of the repo, which is
ambiguous with several PRs. Acceptable for backwards compat; mark `repo` deprecated in
specs/015-multiple-prs/contracts/api.md and have the UI always send `pr`.

Size: store_prs.rs (1338) and autofix/mod.rs (2182) keep growing, mostly tests; not
blocking, but splitting tests into sibling files would pay off.
