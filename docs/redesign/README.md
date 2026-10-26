# Redesign screenshots

This folder records the v2 redesign; its screenshots show the UI in Spanish.

Screenshots of every screen of the web UI, to compare before and after the redesign.

- `antes/`: the UI at the local tag `ui-v1` (commit `793e4cf`, version 0.11.0).
- `despues/`: the same set with the redesign applied.

Each screen appears twice, `<Screen>-oscuro.png` (dark) and `<Screen>-claro.png` (light), 23 per theme,
named after the boards of the redesign canvas. The window is 1440×900 except
`Tablero1280` and `Tarea1280` (1280×800) and `Jira1024` (1024×700). The UI is in Spanish, the time zone is
Europe/Madrid, and animations and the cursor are turned off.

## Regenerating them

The script is `app/e2e/screens.spec.ts` (Playwright). It starts its own isolated daemon and its
own Vite (`app/e2e/setup.ts`, with the fake Claude, `gh` and Jira of the e2e tests), so it does not touch
the installed app (7420) or the previews, and leaves nothing running. Without `DELILAH_SCREENS` it is skipped.

```bash
cargo build -p delilah-daemon -p delilah-cli -p delilah-e2e
cd app && DELILAH_SCREENS=despues npx playwright test screens   # or antes
```

Run it alone (with the `screens` filter): it needs an empty daemon so the keys come out
the same (`LIBRE-1`…`LIBRE-8`, `PROJ-1246`), and it fails if it finds projects or tasks.
To go back to the old UI: `git checkout ui-v1`.

## Data

Projects `signal` (repo) and `delilah` (parent, with submodules), on the e2e test repos.

| Task | State |
|---|---|
| LIBRE-1/2/3 | backlog |
| LIBRE-4 | done, with PR #12 merged |
| LIBRE-5 "Chat: tarjetas de permiso en vivo" | chat mode with a pending permission card, changes in README.md, a terminal and PR #14 (CI failed, changes requested, one comment) |
| LIBRE-6 | running (terminal agent) |
| LIBRE-7 | paused by the usage limit |
| LIBRE-8 | setup failed |
| PROJ-1246 | Jira ticket in `delilah`, waiting for you |

Jira is connected with the sample data (`sample` mode), with PROJ-1242 selected.

## What differs from the mock-ups, and why

- **LIBRE-5 shows "En ejecución" (running), not "Te espera" (waiting for you)**: with a permission card pending,
  the daemon keeps the task running (the wait shows on the card and in the Inbox).
  The task that is "waiting for you" is PROJ-1246.
- **A task in the "Fallida" (failed) state**: the e2e fakes have no way to reach it, so it is not captured.
- **Logs**: in the native app they open from Help › Ver logs; on the web they open with the
  "Logs" button in the status bar, which is the one the script clicks.
- **Jira-conectar** is the form of an unconfigured Jira (captured before connecting it).
- **Relative times** ("hace N min", "Pausada hasta las HH:MM", the task history) and
  the logs depend on when the script runs, and the project paths (Proyectos, project view,
  checkout card) are temporary folders: they change between runs.
- What is native to macOS (notifications, menu, Dock, global ⌘P) does not show in the web UI.
- **Projects with worktree**: the seed sets `workspace: "worktree"` on both projects, as in the "before" capture (then it was the default; now the default is checkout), so the pairs are comparable.
