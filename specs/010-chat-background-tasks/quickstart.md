# Quickstart: try background work in chat mode

Always in a preview, never against the installed app (port 7420).

```bash
cargo build -p delilah-daemon -p delilah-cli
scripts/preview.sh --fresh          # prints UI http://localhost:150N and API http://127.0.0.1:750N
```

Register a fixture repo as a project (the preview prints them), then in the UI create a
task with **Modo chat (experimental)** on.

## US1 – working indicator

Prompt: "Run `sleep 60` in the foreground, then say done."
- Within 2 s: indicator with a running clock and "sleep 60".
- After 30 s with no output: "sin novedades desde hace 30 s" ticking.
- Turn ends: indicator gone.

## US2 – sub-agent card

Prompt: "Use a sub-agent to read every file under src/ and summarise them."
- The Agent card shows "en curso", a clock, a step count and the latest step, live.
- Expand: the list of steps. When done: "Hecho · N pasos · M min".

## US3 / US4 – background work

Prompt: "Run `sleep 90 && echo ok` in the background and wait for it; then say ALL-DONE."
- Turn ends: "En segundo plano" strip with the command and a clock; task stays working on
  the board and sidebar; no Bandeja item.
- After 90 s: a divider "Tarea en segundo plano terminada", a new Claude turn with the
  indicator, then ALL-DONE; the task ends waiting for you as usual; history has
  `agent-resumed`.
- Variant: press "Detener" while the strip shows the command; the row ends as `detenido`
  (or Claude finishes it), and the task ends in its normal state within 30 s.

## API checks

```bash
API=http://127.0.0.1:750N
curl -s $API/api/v1/sessions/<id>/conversation | jq .progress
websocat $API/api/v1/sessions/<id>/live    # {"progress":…} messages, ≤ 4/s
curl -s -X POST $API/api/v1/tasks/<KEY>/runtime -d '{"runtime":"terminal"}' -H 'content-type: application/json'  # 409 while background runs
```

## Tests

```bash
cargo test -p delilah-agents stream transcript
cargo test -p delilah-daemon stream_events hooks live conversation runtime_api
cd app && npx tsc --noEmit && npx vitest related --run src/components/ConversationView.tsx src/components/ToolCard.tsx src/live.ts
```
