## WP-02 — done
Tasks:    T005 ✓, T006 ✓
Files:    app/src/components/GlobalNotices.tsx — usage, pause and "y N más" bars are now <Notice> with useDismissals (ids usage:<name>, pause:<key>, more; signatures hot/full, reason|since, overflow count)
          app/src/components/GlobalNotices.test.tsx — 3 new tests (usage, pause, overflow close/re-show); tooltip test adjusted
Tests:    cd app && npx tsc --noEmit && npx vitest related --run src/components/GlobalNotices.tsx: pass (31 tests)
Notes:    Notice has no title prop, so the usage tooltip moved from the bar div to the text span; existing tooltip test now reads the span's title. Did not run a separate RED pass (tests and code written together). CSS untouched.
