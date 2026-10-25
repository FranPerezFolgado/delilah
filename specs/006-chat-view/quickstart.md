# Quickstart: trying the chat view and chat mode

Always in a preview, never on 7420:

```bash
scripts/preview.sh --fresh     # prints the UI (http://localhost:150N) and API (http://127.0.0.1:750N)
```

Register a fixture project (`scripts/make-fixture-parent.sh` or the fixture repos the
preview prints).

## Chat view (US1, US2)

1. Create a free task (view "Chat", the default) and start it with a prompt that runs a
   command, reads a file and edits it.
2. Open the session. Bubbles and tool cards appear as Claude works. Expand the Bash card
   (output), the Edit card (diff, "Abrir en el editor"), and the Read card.
3. Click "Terminal": the live terminal. Click "Chat": back.
4. Set the task's view to "Terminal" and reopen the session: the terminal shows first.
5. While Claude works, send a message: it shows "en cola" and is delivered at turn end.
   Press "Detener" mid-turn: the turn stops.
6. Restart the preview (Ctrl-C, run it again): the conversation is still there.
7. An idea conversation ("Nueva tarea con Claude") shows tool cards too, and "Crear"
   still works.

API checks:
```bash
curl -s $API/api/v1/sessions/<id>/conversation | jq '.entries | map(.kind) | group_by(.) | map({(.[0]): length}) | add'
curl -s "$API/api/v1/sessions/<id>/conversation?cursor=<c>" | jq '.entries | length'
```

## Chat mode (US3, US4), after the spike

1. New task with "Modo chat (experimental)" on, and start it. The text appears while
   Claude writes. "Terminal" is disabled with the reason.
2. `DELILAH_URL=$API target/debug/delilah attach <task>` → the chat-mode message, exit 1.
3. Ask Claude to run something outside the allow list (e.g. `npm install`). A
   "¿Permitir?" card appears in the chat and in the Bandeja. Try "Permitir una vez",
   then "Permitir siempre en esta tarea" (it does not ask again), then "Denegar" with a
   reason (Claude gets it).
4. "Detener" mid-turn; send another message: it works.
5. Between turns: "Pasar a terminal". The terminal opens on the same conversation, and
   `delilah attach` works. Turn chat mode back on: the next message streams.
6. Restart the preview mid-idle: the task comes back in chat mode.

Native-only behaviour (notifications, Dock badge) needs the Tauri app: not part of this
check.
