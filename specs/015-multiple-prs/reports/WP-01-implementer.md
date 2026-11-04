## WP-01 — done
Tasks:    T001 ✓ (fixtures hand-assembled, not captured live), T002 ✓
Files:
- crates/delilah-daemon/tests/fixtures/hooks/post_tool_use_gh_pr_create.json — PostToolUse Bash payload, URL on stdout
- crates/delilah-daemon/tests/fixtures/hooks/post_tool_use_gh_pr_create_exists.json — "already exists", stdout empty, URL on stderr
- specs/015-multiple-prs/research.md — R2: confirmed shape and differences
- crates/delilah-daemon/src/pr_testkit.rs — `DirGh` (fake gh per repo folder: repo view / pr list / api graphql), `parent_workspace(base, branch) -> (root, Workspace)` (svc-a, svc-b submodules, parent's own repo on the branch), 2 tests
Tests:    pr_testkit::tests::parent_workspace_has_both_submodules_on_the_task_branch, pr_testkit::tests::dir_gh_answers_per_repo_folder; cargo test -p delilah-daemon: pass (1128 + 8)
Notes:
- test_support.rs does not exist; used the existing pr_testkit.rs (already used by PR tests). No lib.rs change needed.
- Fixtures are not live recordings (no Claude session could be run). Shape {stdout, stderr, interrupted, isImage, noOutputExpected} was confirmed from real session transcripts' toolUseResult; paths are placeholders. Stated in research.md R2.
- DirGh is a generalized copy of the private one in pr_loop/merge.rs tests; that file was not touched (could be switched to it later).
- Success fixture stderr contains progress text with no URL; exists fixture has URL as last stderr line.
