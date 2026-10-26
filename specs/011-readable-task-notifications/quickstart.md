# Quickstart: try readable background-task notifications

Always in a preview of the branch (never the installed app on 7420).

```bash
scripts/preview.sh --fresh       # prints UI http://localhost:150N and API http://127.0.0.1:750N
```

## Automated checks

```bash
cargo test -p delilah-agents transcript
cargo test -p delilah-daemon hooks
cd app && npx tsc --noEmit && npx vitest related --run \
  src/backgroundItems.ts src/components/BackgroundDoneItem.tsx src/components/ConversationView.tsx
```

## US1 — one clear item per finished background task

1. In the preview UI, create a task on a fixture repo with chat mode on.
2. Prompt: "Run `sleep 20` in the background, start a background sub-agent that lists the
   files, and tell me when both finish."
3. When each finishes: one item per task, e.g. "Comando terminado · sleep 20 · código 0"
   and "Subagente terminado · List files · 14 s · 3 herramientas · 18.2K tokens". No XML,
   no user bubble.
4. Prompt: "Run `false` in the background." → item styled as a warning: "falló · código 1".
5. Prompt: "Start `sleep 120` in the background", then stop it from Claude ("stop that
   background task") → "detenida".

## US2 — read the report

1. Click the sub-agent item from US1 → the report renders as Markdown; click again → collapses.
2. A hand-back case: an orchestrated task (or a `/feature` run) whose sub-agents report
   through hand-backs. Each sub-agent appears once with its report; no "delivered to you as
   a message" text anywhere.

## US3 — no leaks

1. Unit tests cover: two notifications in one message, user text + notification, missing
   closing tags, entity-escaped and raw `<` in reports.
2. In the task list and inbox, after a background task finishes, the task's last-message
   line is still the user's own last prompt or Claude's last answer, never the notification.

## Compatibility

Point the UI from `main` at the preview API (`VITE_DELILAH_URL=<API>`): it shows the old
one-line markers, no errors in the console.
