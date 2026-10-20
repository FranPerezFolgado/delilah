# Quickstart: trying the backlog and delete

Always in a preview, never against the daemon on 7420:

```bash
scripts/preview.sh --fresh          # prints UI http://localhost:150N, API http://127.0.0.1:750N
export DELILAH_URL=http://127.0.0.1:750N
```

Register one of the fixture repos it prints as a project (`target/debug/delilah project add …`).

## Automated

```bash
cargo test -p delilah-core          # StartAborted transitions
cargo test -p delilah-daemon        # create/patch/start-existing/delete, cascade, free keys, concurrent start
cargo test -p delilah-git           # check_workspace without removing
(cd app && npx tsc --noEmit && npx vitest run)
```

## By hand

1. **Backlog (US1)**: "Nueva tarea" → free task, project, notes → "Guardar en backlog".
   The project's "Backlog (1)" section appears folded in the sidebar; "Backlog" column on the board;
   `git -C <fixture> branch` shows no `feature/LIBRE-1`; `delilah ls` shows `backlog`.
2. **Start (US2)**: open it → "Empezar" → worktree → confirm. It leaves the backlog, the
   agent starts and `~/.delilah…/tasks/LIBRE-1/notes.md` exists with the notes.
   Start another one whose branch already exists → error, still in the backlog.
3. **CLI start**: `delilah new PROJ-1 --title x --project <p>`, then `delilah run PROJ-1 --project <p>`.
4. **Delete (US3)**:
   - backlog task → "Eliminar" → gone.
   - running task in a worktree with a new commit not pushed → warning; "Cancelar" leaves the
     agent running; "Eliminar igualmente" → agent stopped, worktree gone, `feature/<key>` still in
     `git branch`, task gone from sidebar, board and inbox, and after restarting the preview.
   - checkout task → checkout back on its previous branch.
   - `delilah rm <key>` with unpushed work → exit 1 naming `--force`.
5. **Free keys**: delete `LIBRE-2` (the newest), create a free task → `LIBRE-3`.
