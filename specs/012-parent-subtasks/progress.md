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
| WP-07 | 3 | 3A | pending | 0 | | |
| WP-10 | 3 | 3A | pending | 0 | | |
| WP-08 | 3 | 3B | pending | 0 | | |
| WP-09 | 3 | 3B | pending | 0 | | |
| WP-11 | 4 | 4A | pending | 0 | | |
| WP-12 | 4 | 4A | pending | 0 | | |
| WP-13 | 5 | 5A | pending | 0 | | |
| WP-14 | 5 | 5A | pending | 0 | | |
| WP-15 | 5 | 5B | pending | 0 | | |
| WP-16 | 6 | 6A | pending | 0 | | |
| WP-17 | 7 | 7A | pending | 0 | | |
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
