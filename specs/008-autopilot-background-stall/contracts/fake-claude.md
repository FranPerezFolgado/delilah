# Contract delta: `fake-claude` (amends specs/005-e2e-tests/contracts/fake-claude.md)

All additions are backwards compatible: existing scenarios parse and play as before.

## Scenario lookup

Order: `[fake:<name>]` in the prompt → **`<task>.<command>.json`** → `<task>.json` →
`default.json`.

`<command>` is the prompt's leading slash command without the slash, up to the first
space (`/feature LIBRE-1 …` → `feature`, `/orchestrate specs/x` → `orchestrate`). It must
pass `valid_name` once joined (`<task>.<command>`); a prompt without a leading `/word`
skips this lookup. So a task's plan and implement sessions can play different scripts.

## `stop` step, object form

```json
{"stop": {"message": "Waiting on the reviewer.",
          "background": [{"id": "a1", "type": "subagent", "agent_type": "architecture-reviewer",
                          "description": "Review plan", "status": "running"}]}}
```

- The string form `{"stop": "text"}` is unchanged (payload without `background_tasks`).
- The object form sends `Stop` with `last_assistant_message = message` and
  `background_tasks = background` (each entry as given; `[]` allowed).
- `background` left out in the object form: no `background_tasks` field.

## `write` step

```json
{"write": {"path": "specs/libre-1-x/tasks.md", "content": "- [ ] T001 …\n"}}
```

- `path` is relative to the fake's cwd (the task's workspace). Absolute paths, `..`
  components or an empty path: the fake prints the error and exits 2 (a broken scenario,
  like an unknown step).
- Parents are created; the file is overwritten.
- Sends no hook (the daemon reads the spec from disk at the next turn end).
