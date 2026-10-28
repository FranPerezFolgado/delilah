## WP-15F — done
Tasks:    finding 1 (banner count) ✓, finding 2 (Check Jira) ✓
Files:    app/src/components/SubtaskList.tsx — banner counts non-skipped create/attach rows (no `selected` filter); "Comprobar Jira" button in header bumps `check` (re-fetch, silent on error)
          app/src/i18n/en/SubtaskList.ts — "Comprobar Jira": "Check Jira"
          app/src/components/SubtaskList.test.tsx — 2 tests added
Tests:    banner with selected:false rows; Check Jira re-fetch + quiet on error. tsc + vitest (SubtaskList, JiraImportDialog, i18n) pass: 82 tests.
Notes:    SubtaskList.css untouched. The error-case test's `onError` is not passed to the component (it only asserts the preview vanishes quietly).
