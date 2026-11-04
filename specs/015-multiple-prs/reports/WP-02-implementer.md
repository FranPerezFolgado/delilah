## WP-02 — done (one test outside scope fails)
Tasks:    T003 ✓, T004 ✓, T009 ✓, T011 ✓
Files:
- crates/delilah-daemon/migrations/0027_pr_per_url.sql — rebuilds pull_requests (UNIQUE (task_key,url), replaced_by) and pr_notices (pr_url, UNIQUE (task_key,repo,pr_url,kind,cause)); explicit ids; backfill of pr_url for ci-failed/changes-requested/comments/pr-opened; orphan PR-kind open notices resolved.
- crates/delilah-daemon/src/store_prs.rs — PrRow.replaced_by, NoticeRow.pr_url, `counted(rows)`, link_pr upserts on (task_key,url) and resolves nothing, notices insert/resolve by pr_url, unlink_pr resolves only that PR's notices and clears replaced_by pointing at it, save_check uses the PR's url. New: `insert_notice_for(key,notice,pr_url)`, `resolve_notices_of_pr(key,repo,pr_url,kinds)`.
- crates/delilah-daemon/src/prs.rs — OUTSIDE LIST, unavoidable: one test fixture literal `PrRow {..}` got `replaced_by: None` (2 lines).
Tests:    migration_0027_keeps_prs_and_notices_and_backfills_pr_url; two_urls_in_one_repo_keep_two_rows_and_the_same_url_keeps_one; linking_a_new_pr_leaves_the_notices_of_the_others_alone; unlink_pr_clears_replaced_by_pointing_at_it_and_only_its_notices. Rewrote the old tests that encoded one-PR-per-repo (pr-opened per url, relinking autofix).
          `cargo test -p delilah-daemon`: 1129 pass, 1 FAIL (see below). Two other tests (autofix the_fourth_red..., hooks a_held_terminal...) failed once and passed on rerun: flaky, not related.
Notes:
- FAIL autopilot::finish::tests::a_failure_on_the_second_repo_pauses_naming_it_and_a_resume_opens_only_it (src/autopilot/finish.rs ~1095): the fake gh returns the same PR_URL (src/gh/fake.rs:15) for creates in svc-a and svc-b; now one URL = one row, so only 1 row. OUT OF SCOPE: needs the fake (or this test) to return a distinct URL per create (gh/fake.rs is WP-03; finish.rs is no one's list) — assign to WP-04/06.
- Compatibility shims kept so callers compile: NewNotice unchanged; `insert_notice` infers pr_url (first counted PR of the repo for PR kinds, '' otherwise); `resolve_notices_of` still repo-wide. WP-04 should move callers to insert_notice_for / resolve_notices_of_pr and then the shims can go.
- Setting replaced_by (replacement on link) is not in T011; only the column, counted() and the clear in unlink_pr.
- Migration test builds `Store { pool }` after running migrations (Store::connect is private).
- Dead-code warnings for counted/insert_notice_for/resolve_notices_of_pr until later WPs use them.
- Other agent's edits seen in the tree (autofix/mod.rs head_branch, gh/*, pr_types.rs) are WP-03's; I did not touch them. cargo fmt -p ran on the crate; check it did not reformat their files (autofix/mod.rs diff is only their head_branch line).
