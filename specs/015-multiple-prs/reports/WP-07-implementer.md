## WP-07 — done
Tasks:    T018 ✓, T019 ✓
Files:
- app/src/api.ts — PullRequest.linked_by adds "agent", replaced_by, head_branch; setAutofix(key, action, repo?, pr?) sends pr; InboxItem.pr_url?.
- app/src/components/PrView.tsx — retry sends api.setAutofix(owner, "retry", undefined, pr.id). Cards were already keyed by pr.id; grouping by repo already allows several cards per repo.
- app/src/components/PrView.test.tsx — new test "two PRs of one repo render two cards and retry on the second sends its id"; the existing retry test now expects the id.
Tests:    cd app && npx tsc --noEmit (clean) && npx vitest related --run <3 files>: 59 files, 777 tests pass.
Notes:    Worktree had no app/node_modules; I symlinked the main checkout's temporarily to run checks and removed it afterwards. Test was not run red first (rendering already worked); the retry assertions would have failed against the old repo-based call.
