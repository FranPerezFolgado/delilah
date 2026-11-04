## WP-12 — done
Tasks:    T030 ✓, T033 ✓
Files:    crates/delilah-daemon/src/pr_loop/mod.rs — repo_of reworked (pub(super)): workspace pr_repos (or repos if no branch), else project checkout `.`, parent submodules via list_submodules + Workspace::home_of, else `host/owner/name`; LinkError::NotFromTask removed; tests adapted/added
          crates/delilah-daemon/src/pr_api/mod.rs — 409 mapping removed; 409 test turned into host-label test; new "unlink one of two leaves the other" test
          crates/delilah-daemon/src/pr_loop/capture.rs — workaround arm and LinkError import dropped
Tests:    pr_api::tests::linking_a_pr_of_another_repo_is_linked_with_the_host_label, unlinking_one_of_two_prs_leaves_the_other; pr_loop::tests::linking_refuses_urls_that_are_not_prs_and_labels_other_repos_by_host, without_a_workspace_the_projects_checkout_is_dot_and_others_are_labelled, without_a_workspace_a_parents_submodule_is_matched_before_the_host_label. cargo test -p delilah-daemon -- pr_loop pr_api::tests: 74 pass; clippy --all-targets clean.
Notes:    4 tests in pr_api/open.rs fail (WP-13's file, in progress; not touched). URL parsing unchanged (parse_pr_url); submodule paths go through home_of (rejects paths leaving the root). Ran cargo fmt -p delilah-daemon.
