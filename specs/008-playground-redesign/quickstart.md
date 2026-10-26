# Quickstart: trying the playground redesign

```bash
scripts/preview.sh --fresh        # this worktree's daemon + UI; prints UI and API URLs
```

1. Register a fixture repo as a project, create a task and start it (the agent gets the
   hooks from the preview's settings file).
2. Open the task → **Oficina** tab: the lead desk appears; ask the agent to read a file,
   edit one and run `ls` — the monitor goes cyan, blue, orange; the caption follows.
3. Ask it to launch a helper (`Agent` tool): a helper takes a desk; when it finishes it
   walks to the lounge and leaves.
4. Toggle **Flujo**: agent → tool nodes and the helper with its own calls; the timeline
   lists them; click a node → details; click a timeline row → the node highlights.
5. Sidebar **Playground**: one floor per live task; click it → task's Oficina tab.
   "Abrir en ventana" → the window shows the building; click a floor → it opens in the
   window, Back returns; clicking the lead → the main window shows its terminal.
6. After a while the lead desk shows a paper stack; hover it for the token count.
   Check against the API: `curl $API/api/v1/activity | jq .tokens`.
7. History: `curl "$API/api/v1/activity/history?owner=<KEY>" | jq '.sessions[0].entries | length'`.
8. macOS reduce motion on: no walking or typing animation; things appear in place.

Not checkable in the browser preview: the native separate window (needs the Tauri app).
