# Phase 2 migration review: 0027_pr_per_url.sql

No CRITICAL or HIGH findings.

[MEDIUM] crates/delilah-daemon/migrations/0027_pr_per_url.sql:47
Problem: No down migration (project convention is forward-only). The rebuild is lossy to reverse: UNIQUE(task_key, url) allows several PRs per repo, which the old UNIQUE(task_key, repo) cannot hold, and replaced_by and pr_url are dropped.
Fix: State in docs/decisions.md that 0027 is one-way (back up ~/.delilah db before upgrading). An older daemon on the migrated DB would fail at `ON CONFLICT (task_key, repo)` (no matching constraint), so rollback needs the backup.

[LOW] crates/delilah-daemon/migrations/0027_pr_per_url.sql:78
Problem: resolved_at is written as `%Y-%m-%dT%H:%M:%SZ`, while the code writes RFC 3339 `+00:00`. If any reader parses it, both parse, but formats are mixed.
Fix: Acceptable; optionally use strftime('%Y-%m-%dT%H:%M:%f+00:00').

[LOW] crates/delilah-daemon/migrations/0027_pr_per_url.sql:7
Problem: Rebuilds copy all data in one transaction (sqlx runs each migration in one), but these tables are small (PRs per task), so no lock concern. Indexes: none existed beyond the UNIQUE, which is recreated.
Fix: None.

## Checked, OK
- Number 0027 follows 0026; no shipped migration edited.
- Data survival (SC-004): the pull_requests copy keeps ids, every column including error_kind/autofix (added by 0012/0022), snapshot and marks. replaced_by = NULL (new column). No row can violate UNIQUE(task_key, url), because the old table had at most one row per (task, repo) and each PR URL is unique per repo.
- No FK references pull_requests or pr_notices (grep), and no triggers, views or indexes on them, so DROP TABLE with foreign_keys=ON and RENAME are safe.
- pr_notices: ids preserved; PR-kind notices get the url of their (task, repo) PR; orphaned unresolved ones are resolved instead of being stuck; task-level kinds get ''. The new UNIQUE including pr_url is looser than the old one, so the copy cannot violate it.
- Code: PR_COLUMNS includes replaced_by; the upsert uses ON CONFLICT (task_key, url) and never INSERT OR REPLACE, so ids stay stable; notice insert binds pr_url. The queries use named columns, no positional `*`.
- Deploy order: the daemon is a single binary and migrates on start, so old code never meets the new schema except on rollback (see MEDIUM).
- Test: store_prs.rs `migration_0027_keeps_prs_and_notices_and_backfills_pr_url` migrates a DB at version 26 with populated rows (ids, marks, autofix, notices resolved/orphaned), plus a fresh-DB run via migrate!. Test not executed by me.

VERDICT: ✅ SAFE
