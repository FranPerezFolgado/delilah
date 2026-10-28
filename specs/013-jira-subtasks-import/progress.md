# Progress — 013-jira-subtasks-import

Branch: 013-jira-subtasks-import · Base: 329499b · Started: 2026-10-28
Mode: format=markdown commits=yes autonomy=auto

| WP | Phase | Group | Status | Attempts | Commit | Notes |
|----|-------|-------|--------|----------|--------|-------|
| WP-01 | 1 | 1A | done | 1 | 2de38ee | |
| WP-02 | 2 | 2A | done | 1 | 6751f04 | |
| WP-03 | 2 | 2A | done | 1 | e49c4b6 | |
| WP-04 | 2 | 2A | done | 1 | 2b6fdfc | |
| WP-05 | 2 | 2A | done | 1 | f90b4cb | |
| WP-06 | 2 | 2A | done | 1 | cd5ecc2 | |
| WP-07 | 3 | 3A | pending | 0 | | |
| WP-08 | 3 | 3A | verified | 1 | 2ea9c15 | |
| WP-09 | 3 | 3B | pending | 0 | | |
| WP-10 | 4 | 4A | pending | 0 | | |
| WP-11 | 4 | 4A | verified | 1 | f034d1c | |
| WP-12 | 5 | 5A | pending | 0 | | |
| WP-13 | 5 | 5A | verified | 1 | PEND | |
| WP-14 | 5 | 5B | pending | 0 | | |
| WP-15 | 6 | 6A | pending | 0 | | |
| WP-16 | 7 | 7A | pending | 0 | | |
| WP-17 | 7 | 7B | pending | 0 | | |

## Open findings

| Phase | Reviewer | Severity | Finding | File |
|-------|----------|----------|---------|------|
| 1 | migration | LOW | No down script (forward-only by project convention) | migrations/0026_jira_subtask_skips.sql |
| 1 | migration | LOW | Skip rows must be deleted with the parent task in code (no FK); check in WP-04 | store/jira_skips.rs, actions.rs |
| 2 | security | LOW | tasks_by_keys IN (...) could exceed SQLite bind limit with >32k keys | store.rs:536 |
| 2 | architecture | MEDIUM | Planner builds hypothetical parent with project=None; check_attach same_project may give false other-project conflicts (handed to WP-07) | jira_import.rs ~150 |
| 2 | architecture | MEDIUM | Free store fns vs impl Store; apply must open tx via Store::begin_immediate only | store.rs, store/jira_skips.rs |
| 2 | architecture | LOW | begin_immediate duplicates BEGIN IMMEDIATE in 6 places; parent invariants live in resolve_new_task not new_task; refusal_text duplicates api_hierarchy messages; remove #![allow(dead_code)] when wired (WP-07) | various |
