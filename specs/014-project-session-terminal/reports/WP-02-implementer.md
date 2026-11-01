## WP-02 — blocked on running checks (code done)
Tasks:    T004 written (not run), T005 written (not run)
Files:    app/src/components/TaskScreen.test.tsx — replaced the "opens in chat" test with 3 tests: project session opens in terminal; read-only one does; non-live one does and shows "Reabrir"
          app/src/components/TaskScreen.tsx — viewOf project fallback `views.get(s.id) ?? (s.conversation ? "chat" : "terminal")`; defensive comment on the stream branch; updated the comment above `views`
Tests:    NOT RUN. The worktree has no app/node_modules (vitest fails: cannot find 'vite'). I tried to symlink node_modules from /Users/franpf/dev/projects/delilah/app/node_modules; the permission classifier denied it. Neither tsc nor vitest ran, and the tests-fail-first step was not observed.
Notes:    To verify: run `npm ci` in app/ (or allow the symlink), then `cd app && npx tsc --noEmit && npx vitest related --run src/components/TaskScreen.tsx`.
          The existing test "offers no Chat / Terminal switch on a project conversation" uses the `conv` fixture (conversation: true) and is left for T006.
