# Quickstart: verifying the feature

## Automated
```bash
cargo test -p delilah-integrations jira
cargo test -p delilah-daemon jira
cd app && npx tsc --noEmit
```

New tests:
- `map_issue`: a sub-task with a story `parent` → `parent` set, `subtask = true`, `epic = None` (inherited later).
- Sprint parsing: Cloud array of objects, DC array of strings (`state=ACTIVE,name=Sprint 42`), active > future > closed.
- `Filter::jql` with `sprint=current` in the 4 who/progress combinations and with a saved `ORDER BY`.
- `JiraClient::Sample.list`: `Assigned+Open` includes Ana's sub-task under PROJ-1242 and its epic is PROJ-1200; `sprint=current` drops the backlog and closed-sprint tickets.
- Daemon: `GET /jira/issues?sprint=current` → 200 and only rows with an `active` sprint; `sprint=bogus` → 400.

## Manual (sample data)
1. `Conexión` → "Datos de ejemplo".
2. Group by **Épica**: the sub-tasks hang under PROJ-1242 inside "Pagos sin sobresaltos"; none of them in "Sin épica".
3. Ana's sub-task shows up with avatar "A".
4. The orphan sub-task shows "↳ PROJ-xxxx …" as its own row.
5. **Sprint → Actual**: only the "Sprint 42" tickets remain; combine it with Reportados/Hechos.
6. Detail panel of a sub-task: Padre, Épica and Sprint rows; "Analizar con Claude" works.

## Manual (real Jira, work Mac)
- Repeat 2–6 against Cloud / DC. Check that a DC site without Jira Software shows a readable error with "Actual" and recovers with "Todos".
