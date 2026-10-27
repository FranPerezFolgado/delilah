# T056 quickstart in a preview (UI 1501 / API 7501, CLI only)

No browser tool was available, so every UI claim is NOT CHECKED. CLI/API only.
Quickstart syntax differs from the real CLI: `new --free --title "..." --project P`, `run` (not `start`), `--worktree --no-prompt`.
Fixture remotes are local bare repos, no GitHub; the preview ran without the fake gh.

1. Parent task: PASS (CLI). `new --parent-task` -> LIBRE-1; `run` created worktree + branch feature/LIBRE-1-checkout-revamp, pushed to origin (ls-remote), progress "0/2 subtasks merged". Status shows "running" in CLI/API (the UI "0/0 merged" label NOT CHECKED).
   Note: no agent session was verified (status `running`, "Claude is working on it" printed).
2. Subtasks: PASS. `new --parent LIBRE-1` -> LIBRE-2/3; `run LIBRE-2` before parent start refused ("Start the parent task LIBRE-1 first"). After start, both worktrees have origin/<parent branch> as ancestor.
3. Subtask PR (base = parent branch, "Squash"): not checked, needs real GitHub / fake gh.
4. Progress 1/2, "1 commit behind", Update branch, auto-archive: not checked (depends on step 3).
5. Parent PR draft, Mark ready, "Merge commit": not checked (GitHub).
6. Archive rules: PASS. `archive LIBRE-1` and `rm LIBRE-1` both refused: "The parent task has live subtasks: LIBRE-2, LIBRE-3."
7. Convert/attach: PASS. make-parent-task LIBRE-4 ok; `attach LIBRE-5 --parent LIBRE-4` ok (progress 0/1). Attaching started LIBRE-6 refused: "The task has already run; only a never-started task can be attached" (message text; error code `already-started` not inspected). Attaching a subtask to a parent refused ("Subtasks do not nest").
Console/network errors: not checked (no browser).
