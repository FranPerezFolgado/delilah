# Quickstart: trying a task from an idea

Always in a preview, never against the daemon on 7420:

```bash
scripts/preview.sh            # prints UI http://localhost:150N, API http://127.0.0.1:750N
export DELILAH_URL=http://127.0.0.1:750N
```

Register one of the fixture repos it prints as a project.

## Spike (WP-1, before building on it)

With the preview's `claude` command line for an idea conversation
(`--permission-mode dontAsk --plugin-dir … --append-system-prompt … --settings …`):
1. Ask Claude to edit a file: it is refused with no prompt in the terminal.
2. Ask it to run `npm install`: refused, no prompt. `git log` works.
3. It proposes a task with AskUserQuestion and the PreToolUse hook gets it (`hooks.jsonl`),
   with `preview` on the `Crear` option.
4. An answer string such as `Crear — creada LIBRE-9 en el backlog` reaches Claude as is.
5. `ESC` in the PTY stops a turn. Note whether `Stop` fires.

Write the results in research.md (R3, R5) before WP-2.

## Automated

```bash
cargo test -p delilah-agents && cargo test -p delilah-daemon
cd app && npx tsc --noEmit && npx vitest related --run src/components/ConversationView.tsx src/components/AskCard.tsx
cd app && npx vitest run
```

## By hand

1. **Open (US1)**: on the project, "+" → "Nueva tarea con Claude". The conversation view
   opens with the hint and no terminal. Also from "Nueva tarea" → "Con Claude", with
   the project chosen.
2. **Talk**: describe an idea. While Claude works, the view shows "Leyendo …"; its reply
   appears when the turn ends.
3. **Queue and Detener**: send a second message while it works. It shows as "en cola"
   and is delivered after the turn. Send another, remove it, and press "Detener": the
   turn stops.
4. **Cards**: Claude's questions show as cards. Answer one in the Bandeja: the view
   shows it answered.
5. **Confirm**: on the "Nueva tarea" card, the brief is readable. "Cambiar algo" →
   nothing is created. "Crear" → `LIBRE-n` appears in the backlog within 5 s, and the
   card shows the key as a link.
6. **Brief (US2)**: the task's notes start with the "Brief escrito por Claude…" line.
   Its history says "Creada desde la conversación «…»". Edit the notes and start it:
   the agent's `/feature` starts from the edited notes.
7. **Project session (US3)**: in "Preguntar a Claude", ask "haz una tarea de esto".
   The same card appears; "Crear" adds a task, and the session keeps its terminal.
8. **Boundary**: in a task's own Claude, ask it to create a task with that card shape.
   It is an ordinary question and no task appears.
9. **History**: close the conversation, restart the preview (Ctrl-C, run again), and
   reopen it. The whole history is there, and "Reabrir" resumes Claude.
10. **Ver terminal**: open it, type a message there, and go back. The message is in the
    history.
11. **Manual**: the "Nueva tarea" dialog (Jira / Libre) and `delilah new` behave as
    before.
