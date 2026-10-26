# Quickstart: Agent playground

Always in a preview: never port 7420, `scripts/dev-daemon.sh`, or a `pkill`.

```bash
scripts/preview.sh --fresh      # prints UI http://localhost:150N and API http://127.0.0.1:750N
```

1. Register two fixture repos as projects (printed by the preview script). Create one
   task in each, plus a second task in the first project, and start all three.
2. Open **Playground** in the sidebar. Check:
   - there are two project areas, with desks labelled with key, title and phase;
   - the characters change as the agents start and work.
3. In one agent, ask for something that edits a file, then runs `cargo test` or a shell
   command. Watch the character go typing → terminal, with captions such as
   "Edit · api.rs" and "Bash · cargo test…".
4. Ask an agent to "use two subagents in parallel to explore X and Y". Two helpers
   appear around its desk and the main character shows *supervising*; the helpers fade
   out when they finish.
5. Make an agent ask for permission. Its desk is highlighted and the counter reads 1.
   Click the character: the task screen opens on its terminal.
6. Click **Open in window**. In the preview, a new tab opens with only the office; a
   click there brings the main tab to the task.
7. Stop the preview's daemon (Ctrl-C on the preview process only), then start it again.
   The office dims under a "Reconnecting…" banner and comes back.
8. Turn on *Reduce motion* in System Settings → Accessibility → Display: the characters
   hold still.

To inspect what the daemon sees:

```bash
curl -s http://127.0.0.1:750N/api/v1/sessions | jq '.[] | select(.live)'   # live sessions
curl -s http://127.0.0.1:750N/api/v1/activity | jq
```

The native separate window (Tauri `WebviewWindow`, focusing the main window) needs the
desktop app; try it in the installed build after release.
