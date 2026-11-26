## WP-03 — done
Tasks:    T007 ✓, T008 ✓
Files:    app/src/components/UpdateNotice.tsx — every state via <Notice>; X: upToDate/error close, available later, confirmSessions cancel; checking(manual)/downloading/installing/waiting hide via useDismissal(`<kind>:<version>`); text "Cerrar" buttons removed
          app/src/components/UpdateNotice.test.tsx — line-52 test uses getByRole; new tests for each X, hide-only kinds, progress-while-hidden, kind change
Tests:    cd app && npx tsc --noEmit && npx vitest related --run src/components/UpdateNotice.tsx: pass (31 tests, 4 files)
Notes:    Tests and implementation were written in one pass, so RED was not run separately. Sig for checking is "checking:" (no version). Hooks run before the switch (hidden check first). updates.ts untouched.
