# Progress — 008-public-repo-onboarding

Branch: 008-public-repo-onboarding · Base: df896ca · Started: 2026-10-26
Mode: format=markdown commits=yes autonomy=auto

| WP | Phase | Group | Status | Attempts | Commit | Notes |
|----|-------|-------|--------|----------|--------|-------|
| WP-01 | 1 | 1A | verified | 1 | c328ca8 | tools: git-cliff 2.14.2, gitleaks 8.30.1, shellcheck 0.11.0, lychee 0.24.2 (brew; `npx lychee` doesn't work → call `lychee` directly) |
| WP-02 | 2 | 2A | verified | 1 | ff6cbd5 | + comment paths in 6 code files |
| WP-03 | 3 | 3A | verified | 1 | 3baf382 |  |
| WP-04 | 3 | 3A | verified | 1 | 8f3b63a | Codex/Pi reworded as other agents |
| WP-05 | 3 | 3A | verified | 1 | fd20712 |  |
| WP-06 | 3 | 3A | verified | 1 | 05691c8 | English seed via tr(); seed poll flaky once |
| WP-09 | 3 | 3A | verified | 1 | — | done by orchestrator via gh (account FranPerezFolgado checked) |
| WP-07 | 3 | 3B | pending | 0 | | |
| WP-08 | 3 | 3C | pending | 0 | | |
| WP-10 | 4 | 4A | verified | 1 | a5df7ea | user 'no' at prompt exits 0 (contract silent) |
| WP-12 | 4 | 4A | implementing | 0 | | |
| WP-11 | 4 | 4B | implementing | 0 | | |
| WP-13 | 4 | 4B | pending | 0 | | |
| WP-14 | 5 | 5A | verified | 1 | e124e91 |  |
| WP-15 | 5 | 5A | verified | 1 | 8614fd9 | SECURITY.md says no checksums yet; WP-11 must update it |
| WP-16 | 5 | 5A | pending | 0 | | |
| WP-17 | 5 | 5A | implementing | 0 | | |
| WP-18 | 5 | 5A | verified | 1 | — | done by orchestrator via gh: {"enabled":true} |
| WP-19 | 6 | 6A | verified | 1 | d7954db | T041 pending: preview check via ui-verifier; native menu = unit tests only; empty defaults output now → English |
| WP-20 | 6 | 6B | pending | 0 | | |
| WP-21 | 7 | 7A | implementing | 0 | | |
| WP-22 | 8 | 8A | pending | 0 | | |
| WP-23 | 8 | 8B | pending | 0 | | |
| WP-24 | 8 | 8C | pending | 0 | | |

## Open findings

| Phase | Reviewer | Severity | Finding | File |
|-------|----------|----------|---------|------|
| 3 | implementer WP-04 | LOW | architecture.md quotes the deny reason and "session stopped" in English, but the daemon may emit them in Spanish | docs/architecture.md |
