# Quickstart: trying the "Ideas" row

Never on the installed daemon (7420): use a preview of this branch.

```bash
scripts/preview.sh --fresh      # prints UI http://localhost:150N and API http://127.0.0.1:750N
```

1. Register a fixture repo as a project (the script prints them), open the UI.
2. Press the project's 💡 button three times: three idea conversations start.
3. Sidebar: under the project, one row "Ideas 3", no row per idea. A "Preguntar a Claude" (+) session still has its own row.
4. Let one idea ask a question (or wait for its proposal card): the count turns to the waiting colour.
5. Click "Ideas": the project's screen opens on the waiting idea; the three ideas are tabs at the top.
6. Answer it, switch to another idea tab, go to another view, click "Ideas" again: it opens on that tab.
7. Fold the project: the row hides; the project count still includes the ideas.

Checks: `cd app && npx tsc --noEmit && npx vitest related --run src/components/Sidebar.tsx src/components/ideas.ts src/App.tsx`.
