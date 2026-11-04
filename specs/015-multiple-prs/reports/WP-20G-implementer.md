## WP-20G — done, with one OUT OF SCOPE test fix
Verdict: finding VERIFIED (HIGH 2) and fixed; LOW fixed.
Files:
- crates/delilah-git/src/workspace.rs — check_merged iterates ws.pr_repos(opts) (adds "." when the parent is on the branch; its dirty check is the parent check above, submodule pointers ignored); 2 tests added (parent merged -> ok, extra commit beyond head -> NotMerged ".", no head -> skipped).
- crates/delilah-daemon/src/pr_loop/merge.rs — verify() uses ws.pr_repos(opts): fetches "." head, checks it, returns its branch tip in `verified`.
- crates/delilah-daemon/src/archive.rs — removed the block that inserted "." into the deleted branches whenever anything was verified (LOW): "." is now deleted only when its own counted merged PR was verified (a parent without PR keeps its branch).
Tests: cargo test -p delilah-git ok; cargo clippy -p delilah-git -p delilah-daemon --all-targets -D warnings ok; cargo test -p delilah-daemon merge: all pass but 1 (below).
OUT OF SCOPE: crates/delilah-daemon/src/actions.rs:1047 test `a_merge_archive_deletes_the_parents_own_branch_too` encoded the old behaviour (verified = only "svc", expects "." branch deleted). Change needed: pass verified = {"svc": zeros, ".": <rev-parse feature/PROJ-1>} for the deletion assertion, and add a sibling test with only "svc" asserting `git branch --list feature/PROJ-1` is NOT empty (case c, parent without PR keeps branch). Case (a)/(b) covered in delilah-git tests (tests/workspace.rs not allowed; added inline in src).

Update (scope extended): crates/delilah-daemon/src/actions.rs — test a_merge_archive_deletes_the_parents_own_branch_too now passes {"svc","."=tip}; new test a_merge_archive_keeps_the_parents_branch_when_the_parent_had_no_merged_pr (case c). Both pass (cargo test -p delilah-daemon parents: 6 ok). Clippy: only the link_pr dead-code (WP-20F); no other errors in my files.
