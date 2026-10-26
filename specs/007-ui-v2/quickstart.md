# Quickstart: trying UI v2

Always in a preview of this branch. **Never** port 7420, `scripts/dev-daemon.sh`, or stopping
any `delilahd` by name (the installed app runs the agents building this).

```bash
scripts/preview.sh --fresh      # after a migration (0020, 0021) or for an empty state
scripts/preview.sh              # keep the preview's state
```

It prints the UI (`http://localhost:150N`) and the API (`http://127.0.0.1:750N`, below
`$API`). Run it in the background; stop it with Ctrl-C or by stopping that process.

Seed the states the boards show (projects signal / delilah / fixture-parent, tasks in every
status, a pending question, a pending chat-mode permission, PRs through the fake `gh`):

```bash
scripts/seed-preview.sh $API     # see the script's header for what it creates
```

Until T098 extends it, the seed covers projects, backlog tasks and one chat-mode task (it
starts a real Claude: keep it short); create the other states by hand as each step says.

Open the UI in the browser pane. Check **both themes** with the pane's colour-scheme
emulation, at **1440×900** and **1280** (and 1024 for Phase 8). Compare each screen with
the board named in tasks.md.

## Per step

1. **Tokens + kit (US1)**: open a task with a terminal tab and a file tab; flip the colour
   scheme: everything, the terminal and the editor re-theme without reload. Board Sistema.
2. **Shell (US2)**: statusbar counts and Logs button; press ⌘\ (rail). Stop the preview's
   daemon (Ctrl-C on the preview process, then start `scripts/preview.sh` again in the same
   slot): the disconnected banner shows, then recovers alone. Boards Lateral, Desconectado,
   Logs.
3. **Task screen (US3)**: open the waiting task (seeded question): answer it in the Tarea
   tab. Open the chat-mode task with the pending permission: status «Te espera»; answer it in
   the chat; the card stays with its outcome. «Marcar como hecha» → «Deshacer». PR tab with
   the fake `gh` (`DELILAH_GH_BIN=$PWD/scripts/fake-gh.sh`, steps in
   `specs/003-pr-loop/quickstart.md`): set a check running then passing; «Fusionar»
   enables. Setup failure: a project whose setup command is `exit 3`. Boards Main, Estados,
   Tarea, TareaSetup, TareaHecha.
4. **Bandeja, board, Jira (US4, US5)**: «Limpiar avisos» then «Deshacer»; keys ↑↓ 1–3 ⌫.
   Board status filter from the statusbar. `--fresh` preview: first-run checklist; «Omitir»
   Jira, reload. Jira with «Datos de ejemplo»: the parent with 20 subtasks. Boards Bandeja,
   Tablero, Jira, PrimerUso.
5. **Dialogs, projects, settings (US6)**: Nueva tarea: switch origins and open «Opciones
   avanzadas», the height never changes. A new task with no choice starts in the checkout.
   Proyectos › «Quitar…» on a project with a task: blocked. Ajustes › Diagnóstico shows the
   data folder. Boards NuevaTarea, Eliminar, QuickOpen, BacklogTarea, Proyectos,
   ProyectoVista, Ajustes, Conversacion.
6. **Widths (US7)**: resize to 1280 and 1024 on every screen: only the board scrolls
   horizontally. Boards Tarea1280, Tablero1280, Jira1024, EscalaLateral, EscalaJira,
   Cargando, AvisosGlobales.

Native-only (titlebar drag regions, notifications, Dock badge, menu, «Mostrar en Finder»):
not in a preview; the owner checks them in the desktop app.

## API checks

```bash
curl -s $API/api/v1/health | jq .home
curl -s $API/api/v1/tasks | jq '.[0].created_at'
curl -s $API/api/v1/inbox | jq '.[] | {id, kind}'
curl -s -XPOST $API/api/v1/inbox/clear | jq .ids          # never a blocking kind
curl -s -XPOST $API/api/v1/inbox/restore -H 'content-type: application/json' -d '{"ids":[…]}'
curl -s -XPOST $API/api/v1/tasks/LIBRE-3/done; curl -s -XPOST $API/api/v1/tasks/LIBRE-3/reopen | jq .status
curl -s $API/api/v1/tasks/LIBRE-7/setup | jq '{exit_code, command}'
curl -s $API/api/v1/jira/issues/SIG-100/subtasks | jq length
curl -s $API/api/v1/sessions | jq '.[] | {id, runtime, runtime_override, last_activity_at}'
DELILAH_URL=$API target/debug/delilah reopen LIBRE-3
```

## Checks

```bash
cd app && npx tsc --noEmit && npx tsc --noEmit -p e2e && npx vitest run
scripts/e2e.sh ui
cargo test --workspace && scripts/e2e.sh api        # when the daemon changed
cargo test --manifest-path app/src-tauri/Cargo.toml  # when the shell changed
```

## Before/after record (last step)

```bash
cargo build -p delilah-daemon -p delilah-cli -p delilah-e2e
cd app && DELILAH_SCREENS=despues npx playwright test screens
```

Fills `docs/redesign/despues/` with the same 46 images as `docs/redesign/antes/`. Fix
selectors, never framing or data, if the redesign broke one.
