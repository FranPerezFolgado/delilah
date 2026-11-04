## Spec compliance — Phase 2 (Foundational, T003-T019)

Covered: all 17 tasks (T003-T019) have code and tests in the diff.
- T009 migration 0027: both tables rebuilt with explicit ids; `UNIQUE (task_key, url)`; `replaced_by`; notices get `pr_url`, backfill for the four PR kinds, orphans resolved, `UNIQUE (task_key, repo, pr_url, kind, cause)`. No PRAGMA. No indexes existed on either table, so none to re-create. Test in store_prs.rs (T003).
- T011 `link_pr` upsert by url, `unlink_pr` clears `replaced_by` (store_prs.rs:349, test :909), `counted()` (:41).
- T014 autofix `pr` id next to `repo`; `pr` wins, unknown id gives 422, `repo` alone picks the first counted PR (autofix/mod.rs:~995, autofix/api.rs test).
- T015 `subtask_facts` filters `replaced_by IS NULL` (integration.rs:147, test :417).
- T017 DTO `replaced_by`, `head_branch`, `linked_by` "agent"; ordering test (dto.rs:239); `summary` over `counted` (prs.rs:277).
- T018/T019 api.ts mirrored; PrView keys cards by `pr.id` (PrView.tsx:303) and retry sends `pr.id` (:~308).
- T010, T012, T013 present per diff.

Missing: none in scope.
Extra: none noteworthy.
Deviates: none.
Untested / not verified:
- Web UI checks (tsc, vitest) were not run: `app/node_modules` is absent in this worktree. PrView.test.tsx was read in the diff only.

Observations (LOW):
- pr_loop/mod.rs:203 `has_row = rows.iter().any(|r| r.repo == repo)`, and the same pattern in pr_api/open.rs:187,347 and autopilot/finish.rs:457. These are "does the repo have a PR" checks. They fit US1/US3 (later phases, T020+), but T016 asked for every remaining "the PR of a repo" lookup to move. I read these as repo-level presence checks and not as a miss, but confirm they are handled in Phase 3.
- store.rs:1447 is a test-only `(task_key, repo)` query. Fine while no test seeds two PRs in the same repo.

Verification: `cargo test -p delilah-daemon`: 1142 passed + 8 integration passed, 0 failed.

VERDICT: ✅ COMPLIANT (0 CRITICAL, 0 HIGH, 0 MEDIUM, 2 LOW)
