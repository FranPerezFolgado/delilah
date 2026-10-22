# Quickstart: trying saving mode

Never on the installed daemon (7420): in a preview of this branch.

```bash
scripts/preview.sh --fresh        # prints the UI (http://localhost:150N) and the API (http://127.0.0.1:750N)
```

1. In Ajustes, turn on "Modo ahorro por defecto" (and leave "Guardar notas antes de cambiar de sesión" on).
2. Register a fixture project and create a free task: the form shows "Modo ahorro" ticked.
3. The task header shows "Modo ahorro"; the chain reads Spec y plan ● → Implementación ○ → Revisión ○.
4. Take the first session through the spec, plan and tasks. When its turn ends with `tasks.md` written, the Bandeja offers "Implementar en sesión nueva".
5. Confirm: the first session gets `/save-session`; when that turn ends, an "Implementación" tab opens with `/orchestrate specs/<dir>` and becomes the active tab; "Spec y plan" shows ✓ and "Reabrir".
6. When every task is checked, "Revisión" starts on its own and a notification says so.

Quick checks without a real feature, with the API:

```bash
API=http://127.0.0.1:750N
curl -s -XPOST $API/api/v1/tasks/LIBRE-1/saving-mode -d '{"on":true}' -H 'content-type: application/json'
curl -s -XPOST $API/api/v1/tasks/LIBRE-1/handoff -d '{"action":"confirm"}' -H 'content-type: application/json'
```

Measuring the saving (SC-001): sum the context per turn of every session of the task.

```bash
python3 -I - ~/.claude/projects/<workspace-dir>/*.jsonl <<'EOF'
import json,sys
tot=0
for p in sys.argv[1:]:
    seen=set()
    for l in open(p):
        o=json.loads(l); m=o.get('message') or {}; u=m.get('usage')
        if o.get('type')!='assistant' or o.get('isSidechain') or not u or m.get('id') in seen: continue
        seen.add(m['id']); tot+=u.get('input_tokens',0)+u.get('cache_read_input_tokens',0)+u.get('cache_creation_input_tokens',0)
print(tot)
EOF
```

Checks: `cargo test --workspace`, `cargo test --manifest-path app/src-tauri/Cargo.toml`, `cd app && npx tsc --noEmit && npx vitest run`.
