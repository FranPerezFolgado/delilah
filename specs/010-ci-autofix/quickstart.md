# Quickstart: CI autofix

Try it in a preview, never on the installed app (port 7420).

1. `scripts/preview.sh --fresh` (background); note the API `http://127.0.0.1:750N`
   and UI `http://localhost:150N`. Use `DELILAH_URL=<API> target/debug/delilah …`.
2. Register a fixture project whose remote is a test GitHub repo with a CI workflow
   that fails on demand (e.g. a test reading a file the agent can fix).
3. `delilah project show <name>`: `ci-autofix: autopilot` (default).
4. Create a task without autopilot: `GET /tasks/<KEY>` → `"ci_autofix": false`.
   `delilah autofix <KEY> on` → on; the PR tab shows "Autofix: watching".
5. Push a commit that makes CI fail and open the PR (or "Open PR"). When every check
   has finished: within ~30 s the agent receives "Delilah's CI autofix (attempt 1 of 3)",
   the `ci-failed` Inbox item disappears, no "CI failed" notification arrives, the PR tab
   shows "Fixing CI: attempt 1 of 3", the history `autofix-sent`.
6. The agent pushes a fix; CI goes green → PR tab "watching", history `autofix-fixed`.
7. Make the failure unfixable (e.g. a check that always fails): after 3 attempts one
   `pr-autofix` Inbox item "gave up after 3 attempts" and one notification; no 4th
   message. "Retry autofix" sends the current failure as attempt 1.
8. Ask the agent (by hand) to stop without pushing during an attempt → item "finished
   without pushing".
9. Project settings → "CI autofix: every task" → a new task starts with it on.

Automated: `cargo test -p delilah-daemon autofix`, `cargo test -p delilah-e2e --test autofix`,
`cd app && npx vitest related --run src/components/PrView.tsx src/components/AutofixControl.tsx`.
