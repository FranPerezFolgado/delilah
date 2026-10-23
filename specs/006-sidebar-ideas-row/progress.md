# Progress — 006-sidebar-ideas-row

Branch: feature/LIBRE-14 · Base: ff78961 · Started: 2026-10-24
Mode: format=markdown commits=yes autonomy=auto

| WP | Phase | Group | Status | Attempts | Commit | Notes |
|----|-------|-------|--------|----------|--------|-------|
| WP-01 | 2 | 2A | done | 1 | 577a63c | runs with WP-02 |
| WP-02 | 3 | 3A | done | 1 | 6a37e4a | runs with WP-01 |
| WP-03 | 4 | 4A | done | 1 | e1e4a53 | T008 no-op |
| WP-04 | 5 | 5A | verified | 1 | | +2 tests from phase 2-3 LOWs |
| WP-05 | 6 | 6A | pending | 0 | | |
| WP-06 | 6 | 6B | pending | 0 | | orchestrator does it at finish |

## Open findings

| Phase | Reviewer | Severity | Finding | File |
|-------|----------|----------|---------|------|
| 2-3 | spec-compliance | LOW | No test asserts the idea icon on the Ideas row (`.srow.ideas svg.idea`) — handed to WP-04 | app/src/components/Sidebar.test.tsx |
| 2-3 | spec-compliance | LOW | Scenario 2 order "Ideas → other sessions → tasks" not tested with a task present — handed to WP-04 | app/src/components/Sidebar.test.tsx |
| 2-3 | spec-compliance | LOW | Until US3 lands, an unfolded project hides a waiting idea (fixed by WP-04 / T011) | app/src/components/Sidebar.tsx |
| 4 | spec-compliance | LOW | No test for the App glue (last shown read via `sessionOf(sessionFor(owner).active)`); covered by the preview check of US2 scenario 2 | app/src/App.tsx |
