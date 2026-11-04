## WP-18 — done
Tasks:    T044 ✓, Inbox retry by pr_url ✓
Files:
- app/src/components/PrView.tsx — replaced_by set: card gets .pr-replaced + inline opacity 0.6 (no CSS file allowed) and a "reemplazada" label
- app/src/components/PrView.test.tsx — test "shows a replaced PR muted with a label"
- app/src/components/InboxView.tsx — pr-autofix retry resolves PR id via api.taskPrs by pr_url, calls setAutofix(task,"retry",undefined,id); falls back to repo
- app/src/components/InboxView.test.tsx — new test for pr_url match; existing repo-fallback test now mocks taskPrs
- app/src/i18n/en/PrView.ts — "reemplazada": "replaced"
Tests:    tsc passes; PrView/InboxView tests pass; i18n coverage has 1 failure NOT from this WP: "components/OpenPrDialog.tsx: Repositorio padre" lacks an English entry (OpenPrDialog from another WP; add to i18n/en/OpenPrDialog.ts).
Notes:    "Badge count": PrView has no count; the only PR chip outside is TaskScreen.tsx:680 (prs[0]) and SessionChain/TaskScreen use prs.length > 0 — outside allowed files, so replaced PRs are not excluded there (FR-009 count part needs a follow-up there, e.g. filter replaced_by == null). Not run red first. node_modules symlink removed.

## Scope extension
- TaskScreen.tsx: taskPrs result filtered to replaced_by == null (drives hasPrs and the header chip). SessionChain.tsx: prLinked = some PR with replaced_by == null.
- Tests: TaskScreen.test.tsx (chip shows #42, not replaced #41; taskPrs added to the api proxy mock), SessionChain.test.tsx (replaced-only PR is not "linked", live one is).
- The i18n coverage failure was a stale vitest cache: with --fsModuleCache=false, src/i18n passes. tsc and all 6 files (219 tests) pass. node_modules symlink removed.
