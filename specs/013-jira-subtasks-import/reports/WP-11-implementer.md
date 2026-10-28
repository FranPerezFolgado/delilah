## WP-11 — done
Tasks:    T030 ✓, T032 ✓
Files:    app/src/components/JiraImportDialog.tsx — convert/existing parent lines, project picker only for create parent, attach badge, conflict reason
          app/src/components/JiraImportDialog.test.tsx — 3 new tests
          app/src/i18n/en/JiraImportDialog.ts — 3 new strings (no es file: source strings are Spanish)
Tests:    convert/existing parent lines; refused disables confirm; attach/conflict rows. `npx tsc --noEmit && npx vitest run src/components/JiraImportDialog.test.tsx src/i18n` pass (70).
Notes:    needsProject now only applies when parent action is create. Conflict rows already disabled via canTick.
