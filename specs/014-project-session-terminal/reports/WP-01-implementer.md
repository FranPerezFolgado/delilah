## WP-01 — done
Tasks:    T001 ✓, T002 ✓ (5 new tests failed first), T003 ✓
Files:    app/src/components/SessionViewSwitch.tsx — props union (view | runtime arm), shared Segmented, view mode calls chooseView with no dialog/disabled
          app/src/components/SessionViewSwitch.test.tsx — dropped view/chooseView from runtime renders; added 5 view-mode tests
Tests:    view mode: pressed / Chat calls chooseView only / pressed click no-op / view=chat / never disabled. vitest related: 125 pass; tsc --noEmit clean (via node_modules/.bin/tsc).
Notes:    app/node_modules was missing in this worktree; ran `npm ci` (untracked). `npx tsc` resolved to a wrong package, use node_modules/.bin/tsc.
