## WP-04 — done
Tasks:    T006 ✓, T012 ✓, T013 ✓
Files:
- crates/delilah-daemon/src/pr_loop/notices.rs — "raised" check in `record` also matches `n.pr_url == row.url` (store.save_check already inserts/resolves by the PR's url).
- crates/delilah-daemon/src/pr_loop/mod.rs — `detect` duplicate check is `r.url == url`.
- crates/delilah-daemon/src/pr_api/forward.rs — notice -> PR by `p.url == notice.pr_url`; test added.
- crates/delilah-daemon/src/inbox.rs — notice -> PR by `p.url == n.pr_url` (archive fallback to first PR kept); `pr-autofix` item gets new optional `pr_url` field (skipped when empty); tests added.
Tests:    forward::forwarding_the_notice_of_the_second_pr_of_a_repo_names_that_pr; inbox::two_prs_in_one_repo_each_have_their_own_items. Both failed first (red), pass now. `cargo test -p delilah-daemon -- forward inbox pr_loop run:: preview`: 250 pass. Full run: only failures were autofix::api::on_resets_stopped_prs_and_their_notice (WP-05's file, in flight) and timing-flaky run::/preview:: tests (pass in isolation).
Notes:
- Contract addition: `InboxItem.pr_url` (pr-autofix items only). WP-06/07 or contracts/api.md may need to mention it; the app can use it to open the right PR.
- The pr-autofix notice only carries a pr_url if autofix (WP-05) inserts it with `insert_notice_for(..., pr.url)`; `autofix-stopped` is not in store PR_KINDS so the `insert_notice` shim stores ''. WP-05 must use insert_notice_for.
- `cancel_logged(app,&key,Some(&row.repo))` in notices.rs:89 left: it is autofix/mod.rs (WP-05 signature).
- pr_loop/merge.rs (lines 205, 305) still uses the `insert_notice` shim; not in my files, so shims stay. `has_row` by repo in pr_loop/mod.rs:203 kept on purpose (detect runs only for repos with no PR).
