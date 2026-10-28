## WP-08 — done
Tasks:    T018 ✓, T019 ✓, T024 ✓, T025 ✓, T026 ✓
Files:    app/src/api.ts — import types + jiraImportPreview/jiraImport
          app/src/api.test.ts — case for both calls
          app/src/components/JiraImportDialog.tsx (new) — create-mode dialog (mode prop, `JiraImportMode = "create"`)
          app/src/components/JiraImportDialog.test.tsx (new)
          app/src/components/JiraView.tsx — "Crear tarea padre" button in the sub-tasks header + dialog
          app/src/components/JiraView.test.tsx — T019 case
          app/src/i18n/en/JiraImportDialog.ts (new), app/src/i18n/en/JiraView.ts — English strings
Tests:    tsc --noEmit + vitest run (JiraImportDialog, JiraView, api, i18n): 103 pass
Notes:    No Spanish i18n file exists (Spanish lives in components). The button shows only for subtask_count>0,
          non-subtask, with no existing task. Create mode sends skip: [] and unskip = ticked rows that were skipped.
          Dialog fetches preview and api.projects itself. app/node_modules was missing in this worktree; I symlinked it
          to /Users/franpf/dev/projects/delilah/app/node_modules (untracked/ignored, remove if unwanted).
