## Spec compliance re-review — fixes WP-20F (FR-009) and WP-20G (FR-013)

FR-009 ✓ detect() now calls link_pr_on_branch(key, &new, Some(branch)) (pr_loop/mod.rs:377), which runs replace_closed (store_prs.rs:225,367) in the same transaction: a closed-unmerged PR of the same repo+head branch gets replaced_by = new id and its notices resolved. Test: a_detected_pr_replaces_the_closed_one_of_the_same_branch (mod.rs ~1005) asserts replaced_by and counted()==1. Single-PR tasks: a first detection has no older row, so nothing changes; the "already linked" and "unlinked" early returns are untouched.
FR-013 ✓ check_merged (delilah-git workspace.rs:877) and verify (merge.rs:288) iterate ws.pr_repos(opts), so "." is fetched, checked for merge-base and dirtiness, and its tip is returned in `verified` only when it has a counted merged PR head. archive.rs no longer adds "." on its own, so a parent with no merged PR keeps its branch. Plain repos: pr_repos == ["."] as before (test pr_repos_of_a_plain_repo_is_the_repo).
Tests: delilah-git (inline: parent merged ok, extra commit -> NotMerged ".", no head skipped; pr_repos tests); actions.rs a_merge_archive_deletes_the_parents_own_branch_too and a_merge_archive_keeps_the_parents_branch_when_the_parent_had_no_merged_pr.
Run: cargo test -p delilah-git: 35+8 pass. cargo test -p delilah-daemon (filters pr_loop/parents/merge): 104 pass.

Missing: none. Extra: none (link_pr made cfg(test) is housekeeping).
Deviates: none.
Untested (LOW): no end-to-end test of archive deletion of "." through verify() (merge.rs) itself; covered piecewise by the git and actions tests.

VERDICT: COMPLIANT (no CRITICAL/HIGH)
