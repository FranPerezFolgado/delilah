# Progress — 012-parent-subtasks

Branch: 012-parent-subtasks · Base: d7567a3 · Started: 2026-10-27
Mode: format=markdown commits=yes autonomy=auto

| WP | Phase | Group | Status | Attempts | Commit | Notes |
|----|-------|-------|--------|----------|--------|-------|
| WP-01 | 1 | 1A | done | 1 | | |
| WP-02 | 2 | 2A | done | 1 | ed727b8 | |
| WP-03 | 2 | 2A | done | 1 | e8291d0 | |
| WP-04 | 2 | 2B | done | 1 | ffb857f | |
| WP-05 | 2 | 2B | done | 1 | 1e6a9a3 | |
| WP-06 | 2 | 2C | done | 1 | | |
| WP-07 | 3 | 3A | done | 1 | | |
| WP-10 | 3 | 3A | done | 1 | | |
| WP-08 | 3 | 3B | done | 1 | | |
| WP-09 | 3 | 3B | done | 1 | | |
| WP-11 | 4 | 4A | done | 1 | | |
| WP-12 | 4 | 4A | done | 1 | | |
| WP-13 | 5 | 5A | done | 1 | | |
| WP-14 | 5 | 5A | done | 1 | | |
| WP-15 | 5 | 5B | done | 0 | a7b5001 | |
| WP-16 | 6 | 6A | done | 1 | 06196e2 | |
| WP-17 | 7 | 7A | verified | 1 | | |
| WP-18 | 8 | 8A | pending | 0 | | |
| WP-19 | 8 | 8B | pending | 0 | | |

## Open findings

| Phase | Reviewer | Severity | Finding | File |
|-------|----------|----------|---------|------|
| 2 | security | MEDIUM | merge_refs runs `git merge` without `--`/ref validation; base override interpolated unvalidated into refs/remotes/origin/{base}^{commit} (fix when wiring in WP-08/WP-14) | crates/delilah-git/src/workspace.rs, base.rs |
| 2 | security | LOW | merge_refs reset --hard on failure can discard uncommitted changes | crates/delilah-git/src/workspace.rs |
| 2 | architecture | MEDIUM | integration.rs raw SQL via store.pool; move into Store | crates/delilah-daemon/src/integration.rs |
| 2 | architecture | MEDIUM | MergePreference→MergeMethod via serde round trip; export + explicit conversion | integration.rs, delilah-core lib.rs |
| 2 | architecture | MEDIUM | merge_refs duplicates base_update::merge_all (reuse in WP-14) | delilah-git, base_update.rs |
| 2 | contract | MEDIUM | TS ProjectSettings.parent_task_merge required; make optional (`?? "merge"`) | app/src/api.ts |
| 2 | migration | LOW | doc comment stranded above subtasks_of; subtasks_of scans all tasks; no CHECK on kind | store.rs |
| 3 | architecture | MEDIUM | parent-task start branch inline in run.rs (~30 lines); StartPlan.in_checkout/branchless unread (rule duplicated with branching.rs) | run.rs, integration.rs, branching.rs |
| 3 | spec | LOW | failed branch push of a parent task at start is only logged | run.rs |
| 3 | notes | INFO | Phase 3 HIGHs (UI refusal parsing, push helper) fixed in the WP-03F commit; the reviewers were not re-run on the fix (tests cover both). ui-verifier not run yet: do it against a preview at the end. |  |
| 4-5 | various | LOW | `parent_ref`-missing refusal untested; TS `method` lacks `\| null`; `expected_base` null when `base_for` errors; non-draft open when ready untested; first-ref conflict untested | pr_api, api.ts |
| 5 | security | LOW | `pr_ready` has no gh account check; task title unsanitised in empty-commit message; empty commit goes to `source_of` repo | pr_api/ready.rs, open.rs |
| 5 | ui | LOW | Empty-commit open always uses repo "." and draft; draft badge shows on any draft PR | PrView.tsx |
| 6 | spec | LOW | T047 names TaskHeader.tsx but change is in TaskView.tsx; archive of a parent with only archived subtasks untested | TaskView.tsx, actions.rs |
