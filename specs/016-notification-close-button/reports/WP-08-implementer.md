## WP-08 — done
Tasks:    T025 ✓, T026 ✓
Files:    app/src/components/DisconnectedBanner.tsx — onClose prop, renders via Notice (dcb, alert)
          app/src/components/DisconnectedBanner.test.tsx — onClose passed; new close-button test
          app/src/App.tsx — useDismissal(offline ? "offline" : null); banner only when offline && !hidden; shell-content still driven by offline
          app/src/App.test.tsx — close keeps content inert; up then down again brings banner back
Tests:    npx tsc --noEmit + vitest run on both test files: pass (9 tests)
Notes:    RED step not run separately (tests and implementation written in one pass).
