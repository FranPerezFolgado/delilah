# Quickstart: trying autopilot

Never on the installed app (7420). In a preview of this branch:

```bash
scripts/preview.sh --fresh          # prints UI http://localhost:150N, API http://127.0.0.1:750N
```

## Daemon only (no Claude): stubbed chain

`cargo test -p delilah-daemon autopilot` covers the chain, stalls, the nudge, pauses,
resume, answers, the outcome file, opening the PR once (fake `gh`) and the preview
process. `cargo test -p delilah-core` covers the state and events.

## Preview command

1. In the preview UI, open the fixture project `signal` and set its preview command to
   `python3 -u -m http.server 0 --bind 127.0.0.1` (it prints
   `Serving HTTP on 127.0.0.1 port N (http://127.0.0.1:N/)`; any command that prints a URL works).
2. On any of its tasks with a workspace, "Lanzar preview": the URL appears in a few
   seconds; "Parar preview" stops it. Archive the task: the preview stops.
3. A command that exits (`false`) shows "failed" with its output.

## End to end with real Claude (one run)

1. Register `fixtures/signal` as a project; set the preview command as above.
2. With a fake `gh` so nothing reaches GitHub:
   `DELILAH_GH_BIN=scripts/fake-gh.sh FAKE_GH_DIR=… scripts/preview.sh` (setup in `specs/003-pr-loop/quickstart.md`).
3. New task, "Autopiloto" ticked (saving mode ticks itself), a tiny idea
   ("add a `--version` line to the README").
4. Watch the chain: Spec y plan → Implementación → Revisión → Preview → PR, with no
   action. Expect at most one nudge per step in the history.
5. Force a pause: in a second task, answer nothing to a permission prompt, or make
   the review block (a failing test in the fixture): the Bandeja shows
   "Autopiloto en pausa: …"; "Reanudar autopiloto" continues.
6. Idea card: in a project's "Nueva tarea con Claude", end with a proposal and choose
   "Crear y lanzar en autopiloto".

What needs the Tauri app (notifications, Dock badge) is not tried here.
