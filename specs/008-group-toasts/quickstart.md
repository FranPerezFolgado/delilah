# Quickstart: trying grouped toasts

```bash
cd app && npx tsc --noEmit && npx vitest related --run src/components/ui/Toast.tsx src/components/GlobalNotices.tsx src/components/ProposalCard.tsx
scripts/preview.sh --fresh      # prints UI http://localhost:150N and API http://127.0.0.1:750N
```

In the preview UI:

1. **Merge**: in a project's setup banner press «Copiar» three times → one «Comando copiado ×3».
2. **Collapse**: trigger two different errors (e.g. change a preference with the preview daemon
   stopped… or delete a task with a bad key via the API) and copy a command → newest shown, «+2»;
   hover → all three; leave → collapsed; «Cerrar todo» → none.
3. **Pause**: copy a command, hover the stack for 10 s, leave → it stays ~its remaining time.
4. **Waiting group**: start two tasks on the fixture repos and open a third screen; when both
   ask (status «Te espera») → one toast «2 tareas te esperan · Ver» → «Ver» opens the Inbox;
   answer one → «KEY te espera · Responder»; answer the other → toast gone.
5. **Undo**: «Limpiar avisos» → «Deshacer» works; discard a proposal → «Deshacer» within 6 s works,
   and the toast goes away when the discard happens even if hovered.
