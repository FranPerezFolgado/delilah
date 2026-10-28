## WP-08F — done
Tasks:    finding 1 ✓, 2 ✓, 3 ✓
Files:    app/src/api.ts — JiraImportTask.kind: TaskKind, status: TaskStatus; JiraImportRow.reason optional
          app/src/components/JiraView.tsx — "Crear tarea padre" shows when no task or the loaded task is normal
          app/src/components/JiraView.test.tsx — new test (normal task shows, parent_task hides)
          app/src/components/JiraImportDialog.test.tsx — fixtures use {type:"queued"}, kind "normal", typed JiraImportTask
Tests:    tsc + vitest (dialog, view, subtasklist, api, i18n): 116 pass
Notes:    dialog code never rendered task.status and only truthy-checks reason, so no change needed there. Sub-tasks stay excluded via hasSubs (!issue.subtask).
