## WP-03 — done
Tasks:    T007 ✓
Files:    README.md — rewritten per the approved outline (134 lines)
Tests:    no code tests. Checks: 134 lines (<=200); Install section ~13 lines (<1/4); all relative links resolve (script); decision sections (#product, #architecture, #pull-requests, #working-on-delilah, #fresh-task-bases) and usage anchors (#install, #getting-started, #resource-usage) exist; English; no work/company/ticket names.
Notes:
- Figures re-run at the current HEAD, so they differ from outline.md: active days 12 (2026-10-19..2026-11-02), commits 594 (33 merges), tags 11, Rust 93,073 (outline said 92,073), e2e 1,723, docs 4,214, specs 31 features / 40,427, decisions 106, Playwright 39. Unchanged: crates 94,526, app/src 35,488, shell 585, scripts 2,635, 1,689 Rust / 1,175 vitest tests, 26 migrations. Commit count will grow with the feature's own commits; recompute before the PR.
- Hero uses <picture> with playground-light/dark.png (no GIF). Platform badge dropped (outline: CI, licence, release only).
- pixtuoid credit links the repo and app/src/playground/CREDITS.md.
- Decision links go to the section; "Decision 104" is not cited in the README text.
- board-light.png / board-dark.png are no longer referenced by README.md. Remaining references only in specs/008-public-repo-onboarding (packages.md, tasks.md); not by docs or app code, so they are unused and could be deleted by a file owner.
