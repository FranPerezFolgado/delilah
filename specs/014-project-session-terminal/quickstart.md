# Quickstart: trying project sessions in the terminal

```bash
cd app && npx tsc --noEmit && npx vitest related --run src/components/SessionViewSwitch.tsx src/components/TaskScreen.tsx
scripts/preview.sh --fresh    # in the background; note the UI URL (http://localhost:150N)
```

In the preview UI (register a fixture repo as a project if it's empty):

1. On the project in the sidebar, press "…" (Ask Claude). The new tab shows **the terminal**.
2. The header shows **Chat | Terminal** with Terminal selected. Pick Chat: the conversation shows, and
   no dialog appears. Pick Terminal: the same terminal is back, with its scrollback.
3. Send Claude a prompt and, while it works, flip the switch. It stays enabled and nothing restarts.
4. Open a second project session. Each tab keeps its own view.
5. Start an idea conversation ("New task with Claude" from an idea). It opens in **Chat**, and the
   switch offers Terminal.
6. Open a task in terminal mode. Its switch still asks for confirmation before changing the mode.
7. Open a plain terminal ("+ terminal"). There is no switch.
