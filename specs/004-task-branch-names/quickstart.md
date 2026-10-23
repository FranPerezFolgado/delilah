# Quickstart: trying task branch names

In a preview, never against 7420:

```bash
scripts/preview.sh --fresh
```

Register a fixture repo as a project (the script prints them).

1. **Proposal by Claude (US1, US2)**: "Nueva tarea" → free → title "Las tareas deben
   crearse con nombre de rama con sentido". About a second after you stop typing, the
   "Rama" field shows something like `feature/task-branch-names` with "propuesta por
   Claude". Start: `git -C <fixture> branch` shows it; the task view shows it under the
   title.
2. **Jira**: a Jira draft `PROJ-12 "Retry failed payments"` → `feature/PROJ-12-retry-failed-payments`.
3. **Edited name**: type `Arreglo del login` → starts on `feature/arreglo-del-login`;
   type `feature/feature/x` → `feature/x`; type `///` → the dialog shows the error and
   doesn't start.
4. **Taken**: `git -C <fixture> branch feature/arreglo-del-login`, start another task
   with that name → `feature/arreglo-del-login-2`.
5. **Fallback**: `DELILAH_CLAUDE_BIN=/usr/bin/false scripts/preview.sh` → the proposal
   is the title's slug ("a partir del título"); the task still starts.
6. **CLI**: `DELILAH_URL=<API> target/debug/delilah run PROJ-12 --title "Retry failed payments" --project <p>`
   → `feature/PROJ-12-retry-failed-payments` (or Claude's name); with
   `--branch "Mi rama"` → `feature/mi-rama`.
7. **Old tasks**: a task started before (branch `feature/PROJ-1`) keeps it in the task
   view, the PR tab and on archive.
