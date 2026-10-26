# Progress — 010-ci-autofix

Branch: 010-ci-autofix · Base: 8d6c284 · Started: 2026-10-26
Mode: format=markdown commits=yes autonomy=auto

| WP | Phase | Group | Status | Attempts | Commit | Notes |
|----|-------|-------|--------|----------|--------|-------|
| WP-01 | 1 | 1A | done | 1 | a9a12b5 | resumed after pause |
| WP-01F | 1 | fix | done | 1 | 42c7c06 | migration backfill = not archived; backfill test |
| WP-02 | 2 | 2A | done | 1 | c405e23 | Task.ci_autofix not persisted yet (WP-03); AutofixDefault not re-exported from core lib.rs |
| WP-04 | 2 | 2A | done | 1 | af18699 | claim_autofix takes &str; forward() paused check now after prompt build |
| WP-03 | 2 | 2B | done | 1 | c897f5c | +prs.rs test literal (allowed); CAS by value in BEGIN IMMEDIATE |
| WP-05 | 2 | 2C | done | 1 | 3f4e2b7 | +run.rs test literal (compile-only); PATCH uses set_ci_autofix, BacklogChange untouched |
| WP-05F | 2 | fix | done | 1 | a8d0b61 | Phase 2 review fixes |
| WP-06 | 3 | 3A | verified | 1 | d7fd9ad | prompt duplicated in autofix (forward::ci_prompt/fit private) → dedupe in WP-07; FR-006 vs R3: followed R3 |
| WP-08 | 3 | 3A | verified | 1 | dabc503 | manual forward: CAS sha before send, restore on failure, 409 after 3 CAS tries |
| WP-07 | 3 | 3B | verified | 1 | 39c4788 | no re-read from gh before send (uses just-saved snapshot, FR-006?); stopped notice hook for WP-12 |
| WP-09 | 3 | 3C | implementing | 0 | | |
| WP-10 | 4 | 4A | pending | 0 | | |
| WP-11 | 4 | 4B | pending | 0 | | |
| WP-12 | 4 | 4B | pending | 0 | | |
| WP-13 | 4 | 4C | pending | 0 | | |
| WP-14 | 5 | 5A | pending | 0 | | |
| WP-15 | 5 | 5A | pending | 0 | | |
| WP-16 | 5 | 5A | pending | 0 | | |
| WP-17 | 6 | 6A | pending | 0 | | |
| WP-18 | 7 | 7A | pending | 0 | | |
| WP-19 | 7 | 7B | pending | 0 | | |

## Open findings

| Phase | Reviewer | Severity | Finding | File |
|-------|----------|----------|---------|------|
| 1 | architecture | LOW | `pub(crate) mod api;` vs autopilot's private `mod api;` + re-export | crates/delilah-daemon/src/autofix/mod.rs:5 |
| 1 | architecture | LOW | Placeholder fns with `#[allow(dead_code)]`; drop the attribute as each gets a caller (`cancel` not in plan's mod.rs description) | crates/delilah-daemon/src/autofix/mod.rs:8-15 |
| 1 | migration | LOW | Backfill also matches autopilot `paused`; confirm intended | crates/delilah-daemon/migrations/0022_ci_autofix.sql:9 |
| 1 | migration | LOW | PRs with NULL snapshot get no seeded sha: a pre-upgrade failure could fire on first check; PrAutofix.sha must accept null | crates/delilah-daemon/migrations/0022_ci_autofix.sql:16-19 |
| 2 | orchestrator | LOW | One unidentified daemon test failed once after WP-05 (836/837), not reproduced in 4 reruns: possible flaky test | crates/delilah-daemon |
| 2 | contract | HIGH (tracked) | UI settings PUT drops `ci_autofix` → resets to autopilot; must be fixed by WP-16 (TS type + DEFAULT_PROJECT_SETTINGS + round trip) before release | app/src/api.ts:101-109 |
| 2 | contract | MEDIUM | Version skew: new CLI vs old daemon 422 on project set (deny_unknown_fields); ship together, note in release notes; event downgrade also fails | crates/delilah-cli/src/main.rs:924 |
| 2 | spec-compliance | MEDIUM (tracked) | PATCH `ci_autofix` flips the column without the toggle's side effects/409 → WP-14 must route PATCH through the same toggle fn or limit it to backlog | crates/delilah-daemon/src/api.rs:563-566 |
| 2 | spec-compliance | LOW | Core T003 test checks a serde parse error; field/code only checked in daemon test | crates/delilah-core/src/project.rs |
| 2 | spec-compliance | LOW | PrAutofix.sha is Option (deliberate, backfill null head) | crates/delilah-daemon/src/autofix/mod.rs |
