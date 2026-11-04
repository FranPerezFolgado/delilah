## WP-13 — done
Tasks:    T031 ✓, T034 ✓
Files:    crates/delilah-daemon/src/pr_api/open.rs — source_of uses Workspace::pr_repos (via pr_repos_of, spawn_blocking); proposal `repos` and default candidate for a parent (no explicit repo) = pr_repos without an open PR; new has_open_pr/parent_branch_of used by proposal and open handler; test a_parent_on_its_branch_offers_dot_and_opens_from_the_parent_folder.
Tests:    cargo test -p delilah-daemon -- pr_api: 67 pass; cargo clippy -p delilah-daemon --all-targets: clean for open.rs (one pre-existing unused LinkError import in pr_loop/capture.rs, not mine).
Notes:
- "Repo has an open PR" now ignores, for `.` of a PARENT workspace only, PRs whose snapshot head branch is not the task branch (hand-linked PRs of other branches). Not applied to submodules/plain repos: FakeGh always reports head "feat/x", which would break existing tests, and I may not edit the fake. If wanted everywhere, FakeGh needs a settable head.
- Default proposal for a parent used to be keyed on `repo == "."` in the query; now on "no repo given", so an explicit `?repo=.` is honoured.
- Test covers the head-branch rule through has_open_pr directly (the fake's head cannot be the task branch).
- autopilot/finish.rs:457 still has a repo-only check (outside scope).
