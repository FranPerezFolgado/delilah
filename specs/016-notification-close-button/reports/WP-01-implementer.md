## WP-01 — done
Tasks:    T001 ✓ (seen failing: module missing), T002 ✓, T003 ✓, T004 ✓
Files:    app/src/components/ui/Notice.tsx (CloseButton, Notice, useDismissals, useDismissal); Notice.css (.ntc, .ntc-x 28px); Notice.test.tsx; index.ts (exports; prop types are inline, none exported)
Tests:    CloseButton, Notice (structure/className/role/onClose, focus move), useDismissals (hide, A->B->A), useDismissal(null). cd app && npx tsc --noEmit && npx vitest related --run src/components/ui/Notice.tsx: pass (694 tests)
Notes:    Tests call setLanguage("es") (default locale is English). Toast.css not edited (out of allowed files; .toast is flex with gap 8px, 28px X fits). Focus move: first focusable after the notice in document order.
