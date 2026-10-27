## WP-10 — done (App.tsx wiring OUT OF SCOPE)
Tasks:    T025 ✓, T026 ✓, T027 ✓
Files:    app/src/components/NewTaskDialog.tsx(+test) — kind choice (normal / parent task), `parentTask` prop creates a subtask; container kinds hide workspace choice, parent task also hides workflow/model/autopilot/submodules/branch; created in the backlog via api.createTask (kind/parent_task), no Iniciar
          app/src/components/Sidebar.tsx(+test) — subtasks nested/collapsible under listed parent, N/M badge, orphan subtask at top level, fold state in localStorage
          app/src/components/SubtaskList.tsx + SubtaskList.test.tsx — new; list with state, PR state, dropped, "Nueva subtarea"; exports progressLabel()
          app/src/components/TaskHeader.tsx(+test) — progress chip for parent task, link to parent for subtask, "Volver a subir la rama" (Push branch again) when `branchMissing`
          app/src/components/TaskScreen.tsx(+test) — SubtaskList on the Tarea tab of parent tasks, new `onNewSubtask` prop, branchMissing set from ApiError.reason "parent-task-branch-missing"
          app/src/i18n/en/{NewTaskDialog,TaskHeader,SubtaskList}.ts — English strings
          app/src/api.ts — Settings.parent_task_merge now optional (tsc clean, no fixture change needed)
Tests:    new cases in each *.test.tsx; `npx tsc --noEmit` clean; vitest on the 5 components + ProjectSettings + api + i18n coverage pass (240)
Notes:    - OUT OF SCOPE: app/src/App.tsx must pass `parentTask` to NewTaskDialog and `onNewSubtask` to TaskScreen (open dialog with that parent), and onOpenTask already exists. Without it the subtask button is disabled.
          - No CSS files allowed: nesting/header layout use inline styles and existing classes; a later pass may move them to Sidebar.css/TaskHeader.css.
          - RunRequest has no `kind`, so parent tasks/subtasks are created in the backlog (start them from the backlog view).
          - branchMissing is only detected from errors raised in TaskScreen/SubtaskList/header; the start refusal itself lives in BacklogTaskView (not allowed), which should surface it too.
          - Kind choice is hidden when a Jira draft is given (existing branch tests rely on radio order).
          - Vitest cache gotcha: after adding a new app/src/i18n/en/*.ts file, coverage.test.ts can fail with a stale import.meta.glob until en.ts content changes once; it passed when en.ts was touched. If the hook reports missing "SubtaskList.tsx" English keys, that is this cache, not a missing entry.
          - app/node_modules symlink removed.
