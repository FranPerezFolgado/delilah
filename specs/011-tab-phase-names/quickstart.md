# Quickstart: try session tabs named after their phase

## Checks

```bash
cd app && npx tsc --noEmit && npx vitest related --run src/chain.ts src/components/TaskScreen.tsx src/components/TaskView.tsx
```

## In a preview (never the installed app)

1. `scripts/preview.sh --fresh` (in the background) and open the printed UI URL.
2. Register a fixture repo as a project and create a task with «Modo ahorro» checked.
3. Start it. The first session's tab reads «Spec y plan» (or «Spec and plan» in English,
   Ajustes → language), not «Claude principal».
4. Once the handoff to implementation happens, the new tab reads «Implementación» /
   «Implement» and the old one keeps «Spec y plan». The task tab's «Sesiones» list shows
   the same names.
5. Create a task without saving mode: its primary tab still reads «Claude principal» /
   «Main Claude».
6. Hover a phase tab: the tooltip shows the stored label.

Stop the preview with Ctrl-C (or by stopping that process), never by name.
