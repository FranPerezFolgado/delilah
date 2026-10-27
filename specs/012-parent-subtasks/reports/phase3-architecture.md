# Phase 3 architecture review (163c0ea..HEAD)

Verdict: sound layering, no CRITICAL. 1 HIGH, 3 MEDIUM.

[HIGH] crates/delilah-daemon/src/api_hierarchy.rs:77
Problem: `push-branch` pushes only `ws.path`, while the start in run.rs:~585 pushes every repo in `ws.repos()` (submodules of a parent project). The retry path cannot repair a partial push; two copies of "push the integration branch" have diverged.
Fix: one helper (integration.rs or a small module) `push_integration_branch(app, &Workspace)` iterating `ws.repos()`, used by run.rs and the endpoint; also by pr_api/open.rs:220 if applicable.

[MEDIUM] crates/delilah-daemon/src/run.rs:573
Problem: plan says run.rs holds only calls, with kind rules in integration.rs. The parent-task branch (share_entries, ready, push, task reload, ~30 lines) is inline in a 5.2k-line file.
Fix: move into `async fn finish_parent_task_start(...)` outside run.rs (integration or a hierarchy start module).

[MEDIUM] crates/delilah-daemon/src/integration.rs:64 (StartPlan)
Problem: `in_checkout` and `branchless` fields are never read. run.rs uses `plan.is_none()` and passes `kind` to `starts_branchless`, so the kind rule lives in two places (StartPlan and branching.rs) and can drift.
Fix: drop the dead fields, or consume them (`in_checkout && plan.is_none()` becomes `plan.map_or(in_checkout, |p| p.in_checkout)`; branchless from the plan). Keep one source.

[MEDIUM] crates/delilah-daemon/src/branching.rs:142
Problem: `starts_branchless` gained a `kind` parameter (crate-private, one caller updated). Fine, but `pending()` repeats the Normal check inline; and the test module `kind_tests` is separate from the existing `tests`.
Fix: a `TaskKind::is_normal()` helper in core; merge the test into `tests`.

Notes: integration.rs stays free of git/gh (respects the plan); new routes live in api_hierarchy.rs as planned; UI changes follow the planned component split. UI components were not reviewed in depth.
