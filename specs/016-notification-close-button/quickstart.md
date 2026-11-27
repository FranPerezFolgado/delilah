# Quickstart: trying the close buttons

Never against the installed app (7420). From the worktree:

```bash
cd app && npx tsc --noEmit && npx vitest run      # all UI tests
scripts/preview.sh --fresh                        # from the repo root; prints UI and API URLs
```

In the preview UI:

1. **Toasts**: do anything that toasts (e.g. "Actualizar rama" on a task, or a failing action); the toast has the X; "Cerrar todo" with several.
2. **Base-stale bar**: register a fixture repo, start a task, add a commit to the fixture's base branch so the base moves; the bar shows with X (no "Descartar"); X dismisses it and it stays gone after reload.
3. **Setup banner**: a project with a failing setup command (project settings) → task shows the failed banner; X hides it; retry from settings → running banner shows.
4. **Autopilot paused**: a task with autopilot that pauses (fixture) → the global bar and the task-screen banner each have X; closing one does not resume.
5. **Disconnected**: stop *the preview's* daemon process (Ctrl-C on its own process, never by name) → banner with X; close it, the app stays greyed; start the preview again → disconnect again → banner back.
6. **Jira 401, usage limit, update states, PR notices**: covered by component tests; not easily reachable in a preview.

Expected everywhere: the same X at the end of the bar, named "Cerrar" for screen readers, reachable with Tab.
