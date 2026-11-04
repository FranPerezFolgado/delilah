## WP-05 — done, except the `pr` field in the API body (OUT OF SCOPE file)
Tasks:    T007 ✓, T008 ✓, T014 ✓ (daemon logic; request body needs autofix/api.rs), T015 ✓
Files:
- crates/delilah-daemon/src/autofix/mod.rs — per-PR: `resolve_stopped(app,key,&PrRow)` (resolves that PR's autofix-stopped notices plus legacy ones with empty pr_url); `stop()` raises the notice with insert_notice_for(row.url); `deliver` resolves ci-failed with resolve_notices_of_pr; new `cancel_pr`, `cancel_pr_logged`, `cancel_where`; `reset_row` by PR id; new `retry_pr(app,key,repo,pr)` (pr id wins; repo alone = first counted PR of that repo; neither = 422 required; unknown = 422). `retry(app,key,repo)` kept as wrapper. `cancel(app,key,repo)` kept (repo-wide) for compatibility.
- crates/delilah-daemon/src/integration.rs — subtask_facts filters `replaced_by IS NULL`.
- crates/delilah-daemon/src/api.rs — untouched (the route is unchanged).
Tests:    cancel_of_one_pr_leaves_its_sibling_in_the_repo_alone, retry_by_pr_id_resets_only_that_pr, retry_by_repo_alone_picks_the_first_counted_pr_of_the_repo, retry_of_a_pr_that_is_not_the_tasks_is_unknown (autofix/mod.rs); a_replaced_pr_does_not_count_in_the_rollup (integration.rs; I wrote the filter in the same step, so no separate RED run). Adjusted reset_row tests to ids. `cargo test -p delilah-daemon`: 1136 pass, 1 flaky (setup::exit_zero_runs_the_task..., passes alone; earlier run also had preview/run flakes under load).
Notes:
- OUT OF SCOPE: crates/delilah-daemon/src/autofix/api.rs (the real handler; tasks.md says mod.rs but `act` lives there). Needed change: add `#[serde(default)] pr: Option<i64>` to `AutofixReq` and call `super::retry_pr(&app, &key, req.repo.as_deref(), req.pr).await?` in the "retry" arm; update doc comment. Until then `pr` is ignored by the API (repo still works).
- WP-04: pr_loop/notices.rs:89 still calls `cancel_logged(app,&key,Some(&row.repo))`, which cancels every PR of that repo. Switch it to `crate::autofix::cancel_pr_logged(app,&key,row.id)`.
- `insert_notice` shim gives autofix-stopped notices pr_url '' (not a PR kind there); resolve_stopped covers that.

## Scope extension (approved)
- autofix/api.rs: `AutofixReq.pr: Option<i64>`; retry routes via `retry_pr(repo, pr)`. Test: retry_accepts_a_pr_id_and_refuses_an_unknown_one. Cancel/stop have no API action carrying a pr, so nothing more there.
- pr_loop/notices.rs:89 now `cancel_pr_logged(app,&key,row.id)`. No autofix-stopped notice is created in that file (stop() in autofix/mod.rs already uses insert_notice_for).
- `cargo test -p delilah-daemon`: 1138 pass, 0 fail.
