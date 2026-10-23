# Progress — 006-sidebar-ideas-row

Branch: feature/LIBRE-14 · Base: ff78961 · Started: 2026-10-24
Mode: format=markdown commits=yes autonomy=auto

| WP | Phase | Group | Status | Attempts | Commit | Notes |
|----|-------|-------|--------|----------|--------|-------|
| WP-01 | 2 | 2A | done | 1 | 577a63c | runs with WP-02 |
| WP-02 | 3 | 3A | done | 1 | 6a37e4a | runs with WP-01 |
| WP-03 | 4 | 4A | done | 1 | e1e4a53 | T008 no-op |
| WP-04 | 5 | 5A | done | 1 | 77c87c0 | +2 tests from phase 2-3 LOWs |
| WP-05 | 6 | 6A | done | 1 | 2195f27 | docs only |
| WP-06 | 6 | 6B | done | 1 | a96dfc8, e4bff74 | merge main + bump 0.7.0; T016 preview left to user |

## Open findings

| Phase | Reviewer | Severity | Finding | File |
|-------|----------|----------|---------|------|
| 2-3 | spec-compliance | LOW | No test asserts the idea icon on the Ideas row (`.srow.ideas svg.idea`) — handed to WP-04 | app/src/components/Sidebar.test.tsx |
| 2-3 | spec-compliance | LOW | Scenario 2 order "Ideas → other sessions → tasks" not tested with a task present — handed to WP-04 | app/src/components/Sidebar.test.tsx |
| 2-3 | spec-compliance | LOW | Until US3 lands, an unfolded project hides a waiting idea (fixed by WP-04 / T011) | app/src/components/Sidebar.tsx |
| 4 | spec-compliance | LOW | No test for the App glue (last shown read via `sessionOf(sessionFor(owner).active)`); covered by the preview check of US2 scenario 2 | app/src/App.tsx |
| 5 | spec-compliance | LOW | Count class ends in a trailing space when nothing waits (same as the folded count; no effect) | app/src/components/Sidebar.tsx |
| 5-6 | ui-verifier | — | Not checked: no browser tools in the agent session; T016 (quickstart in a preview) left to the user | — |
| 6 | code-quality | MEDIUM | Ideas row's aria-label/title don't say an idea is waiting (only the colour does) | app/src/components/Sidebar.tsx |
| 6 | code-quality | MEDIUM | First waiting idea beats a currently shown waiting idea (as FR-007 says; confirm intent) | app/src/components/ideas.ts |
| 6 | code-quality | LOW | Read-only ideas lose their lock icon now that ideas collapse into one row | app/src/components/Sidebar.tsx |
| review | /review | HIGH | `.pcount` overrode `.c-wait`, so the waiting colour never showed on a count (also the folded one); fixed with `.pcount.c-wait` | app/src/styles.css |
| review | user | — | T016 checked by hand in a preview (7 ideas in two projects, waiting colour, open/last-shown, fold): OK | — |
