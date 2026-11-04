## WP-14 — done
Tasks:    T032 ✓, T035 ✓
Files:    app/src/components/OpenPrDialog.tsx — option for "." renders t("Repositorio padre")
          app/src/components/OpenPrDialog.test.tsx — new test
          app/src/i18n/en/OpenPrDialog.ts — new, EN: "Parent repository"
Tests:    "offers the parent repo, labelled, next to the submodules" (failed first, now passes).
          cd app && npx tsc --noEmit && npx vitest run --fsModuleCache=false src/components/OpenPrDialog.test.tsx src/i18n: pass (71 tests)
Notes:    The select already showed when repos.length > 1, so no visibility change was needed. The symlinked node_modules shares the main checkout's vitest fs module cache, which served a stale en.ts glob; use --fsModuleCache=false when running with the symlink. Symlink removed.
