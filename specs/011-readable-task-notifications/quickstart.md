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

## Results

Run on 2026-10-26 against `scripts/preview.sh --fresh --no-ui` (slot 1, fixture repo
`signal`, task SIG-1 in chat mode, real Claude Code session), through the HTTP API: no
browser was available to the verifying agent, so the UI rendering is covered by the vitest
suites (`BackgroundDoneItem`, `backgroundItems`, `ConversationView`) only.

| Scenario | Result | Notes |
|---|---|---|
| US1 1–3 | Pass | Command marker `kind: command, exit_code: 0`; sub-agent notification with `duration_ms`, `tool_uses`, `tokens`, `report_in_handback` plus a hand-back marker with the same `task_id` (folded by the UI). No user entry contains notification or agent-message markup. |
| US1 4 | Fail → fixed (WP-12F) | Two bugs: a notification absorbed mid-turn is written as an `attachment` (`queued_command`, `commandMode: task-notification`) and gave no marker; failed commands' summary `failed with exit code N` gave no `exit_code`. Both parsed now, with tests. |
| US1 5 | Not reproducible | Claude Code wrote no notification for a task stopped with TaskStop. Covered by unit tests. |
| US2 1 | Not checked in a browser | Expand/collapse covered by `BackgroundDoneItem.test.tsx`. |
| US2 2 | Partly | Hand-back marker produced and paired by `task_id` in real data; the fold is unit-tested. |
| US3 2 | Pass | `last_message` on the sessions, `/api/v1/sessions` and the inbox stayed Claude's answer after every notification. |
| Compatibility with `main` UI | Not checked | Change is additive (optional `background` field). |
