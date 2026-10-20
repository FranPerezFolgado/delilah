# Quickstart: trying the phase in the task header

Always in a preview, never against the daemon on 7420:

```bash
scripts/preview.sh            # prints UI http://localhost:150N, API http://127.0.0.1:750N
export DELILAH_URL=http://127.0.0.1:750N
```

Register one of the fixture repos it prints as a project and start a task on it.

## Automated

```bash
cd app && npx tsc --noEmit && npx vitest related --run src/phases.ts src/components/PhaseBadge.tsx
cd app && npx vitest run
```

## By hand

1. **Phase (US1)**: open the running task. The header shows "1 Especificación" before
   the status pill, and the board column and the spec panel's stepper match it.
2. **Live**: with the terminal open, move the task to Plan
   (`curl -X POST $DELILAH_URL/api/v1/tasks/<KEY>/phase -d '{"phase":"plan"}' -H 'content-type: application/json'`
   or the spec panel's stepper). The header changes to "2 Plan" without a reload.
3. **Progress (US2)**: move it to Implementación in a workspace whose spec has a
   `tasks.md`. The header reads "3 Implementación · n/N".
4. **Hidden**: a backlog task (its screen is the backlog view) and a finished task show
   no phase. The project screen header is unchanged.
5. **Narrow**: shrink the window to 1024 px with a long title. The title truncates, and
   the phase and the status stay whole.
