## WP-13 — done
Tasks:    T034 ✓, T035 ✓, T037 ✓, T038 ✓
Files:    app/src/components/SubtaskList.tsx — preview on mount, banner, not-connected note, Skipped (n) + Undo, opens dialog in add mode
          app/src/components/JiraImportDialog.tsx — "add" mode (no project picker, create/attach rows + skipped section, per-row Skip/Undo -> skip/unskip)
          app/src/components/SubtaskList.test.tsx, JiraImportDialog.test.tsx — new tests
          app/src/i18n/en/SubtaskList.ts, JiraImportDialog.ts — English entries
Tests:    cd app && npx tsc --noEmit && npx vitest run SubtaskList JiraImportDialog i18n: pass (79)
Notes:    api.ts untouched (types already there). Undo in SubtaskList calls api.jiraImport({add:[],skip:[],unskip:[key]}) then reloads the preview. Banner counts non-skipped create/attach rows with selected=true. Preview errors are silent. "Check Jira" action not added (preview reloads on key/progress change and after dialog done). No CSS added for .subtasks-banner/.subtasks-note (SubtaskList.css not in allowed files).
