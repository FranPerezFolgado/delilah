## WP-15 — done
Tasks:    T042 ✓, T043 ✓
Files:    app/src/components/ProjectSettings.tsx — "Parent task merge method" select (merge/squash/rebase) with fallback note
          app/src/components/PrView.tsx/.css — draft badge, Mark ready hint, empty-commit open on no-commits, live-subtasks confirm + retry, base banner (subtask_commits via baseStatus)
          app/src/i18n/en/{PrView,ProjectSettings}.ts — English strings
          tests: ProjectSettings.test.tsx, PrView.test.tsx
Tests:    saves the parent task merge method; parent task: draft badge + mark ready + banner; empty commit; live-subtasks confirm. tsc + vitest related (1036) pass.
Notes:    Empty-commit open is a button in PrView (not OpenPrDialog, out of scope): openPr {repo:".", title: task.title, draft:true, empty_commit:true}. Draft badge shows for any open draft PR. Unrelated modified files (actions.rs, pr_loop/merge.rs) are not mine.
