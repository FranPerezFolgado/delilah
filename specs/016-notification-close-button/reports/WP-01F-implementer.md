## WP-01F — done
Tasks:    1 ✓, 2 ✓, 3 ✓, 4 ✓
Files:    TaskScreen.tsx (visible list first, i===0 on visible; id-less id = local:index:text); TaskScreen.test.tsx (+hidden-first test, RED then GREEN); SetupBanner.css (X absolute top-right, panel relative + padding-right); ui/Toast.test.tsx (setLanguage es, exact "Cerrar"); ui/Notice.tsx (focusAfter skips [inert]/[hidden]); ui/Notice.test.tsx (+inert skip, className merge, minimal children)
Tests:    cd app && npx tsc --noEmit && npx vitest run: pass (1408 tests)
Notes:    The inert test passed on first run since I edited Notice.tsx before running; the TaskScreen test was seen RED first. Index-based id for id-less notices means a dismissal follows position if earlier ones vanish (signature check still applies).
