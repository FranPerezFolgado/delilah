## WP-07 — done
Tasks:    T021 ✓, T022 ✓, T023 ✓, T024 ✓
Files:    app/src/components/PrView.tsx — problem, subtask-commits (top-level hooks), wrong-base and ready hint (hooks inside PrCard) rendered via Notice + useDismissal
          app/src/components/PrView.test.tsx — "dismissable notices" describe (3 tests)
          app/src/components/JiraView.tsx — expired banner via Notice + useDismissal("expired"); jv-body stale unchanged
          app/src/components/JiraView.test.tsx — "JiraView expired banner" test
Tests:    pass: cd app && npx tsc --noEmit && npx vitest related --run src/components/PrView.tsx src/components/JiraView.tsx (145 passed)
Notes:    Sigs: problem=kind; commits=String(n); wrong base=`base→expected`; ready hint=`pr.id` (per plan table). Tests were written together with the implementation, not run RED first.
