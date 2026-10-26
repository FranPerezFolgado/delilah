# Quickstart: trying configurable workflow commands

Everything in a preview (never the installed daemon on 7420):

```bash
scripts/preview.sh --fresh          # prints UI http://localhost:150N and API http://127.0.0.1:750N
export DELILAH_URL=http://127.0.0.1:750N
```

## 1. New project → built-in prompt (US1)

1. Register a fixture repo from the preview's output (`delilah project add …`).
2. `curl $DELILAH_URL/api/v1/projects/<name>` → `settings.workflow` absent.
3. Start a free task "Arreglar el login" with notes. The agent's first message (task transcript /
   `hooks.jsonl`) is "Trabaja en la tarea LIBRE-1: Arreglar el login." + notes pointer; one
   "Principal" session, no saving-mode note even with the saving-mode preference on.

## 2. Migration keeps the author's setup (SC-002)

1. Copy a pre-upgrade state (`fixtures/preview-home-N` from `main`) and start the preview on it.
2. Every project's `settings.workflow` is the phased `/feature {task}` preset.
3. A task started there gets `/feature …` + the saving-mode note, and hands off with
   `/orchestrate specs/<dir>` as before.

## 3. Pick a skill (US2)

1. Add `.claude/skills/fix/SKILL.md` (frontmatter `name: fix`, `description: …`) to the fixture repo.
2. `curl $DELILAH_URL/api/v1/projects/<name>/skills` lists `fix` with `source: project`.
3. In the UI: Projects → the project → settings → "Flujo de trabajo" → pick `/fix`, the field reads
   `/fix {ticket}`; save. Start task PROJ-12 → first message `/fix PROJ-12`.
4. Type `/fix {foo}` → the field shows the unknown placeholder error; save is refused.
5. Choose "Nada"; start a task → agent idle, task waiting for the user.

## 4. Phased with the official preset (US3)

1. Apply "Spec Kit (oficial)"; save. The new-task form now offers saving mode and autopilot.
2. Switch back to "Prompt integrado": the form hides them; `POST /api/v1/runs` with
   `autopilot: true` → 409 `not-spec-workflow`.

## 5. Per-task override and backlog (US4)

1. In a phased project, create a backlog task with start "Investiga {title}".
2. Change the project's workflow; start the backlog task → "Investiga <title>", no saving mode.
3. `delilah run PROJ-13 --no-prompt` → starts idle.

## Checks

```bash
cargo test -p delilah-core -p delilah-daemon -p delilah-cli
cd app && npx tsc --noEmit && npx vitest run
```
