# Quickstart: trying parent tasks and subtasks in a preview

```bash
scripts/preview.sh --fresh          # prints UI http://localhost:150N and API http://127.0.0.1:750N
export DELILAH_URL=http://127.0.0.1:750N
D=target/debug/delilah
```

Register a fixture repo with a GitHub-like remote (the preview prints them). Then:

1. **Parent task**: `$D new --free --title "Checkout revamp" --parent-task --project <name>` → `LIBRE-1`.
   `$D run --worktree --no-prompt LIBRE-1`: a worktree appears, branch pushed to `origin`, no agent session;
   the UI shows "0/0 subtasks merged" instead of "Running".
2. **Subtasks**: `$D new --free --title "Cart API" --parent LIBRE-1` → `LIBRE-2`, and `LIBRE-3`.
   Starting `LIBRE-2` before step 1 is refused with `parent-task-not-started`.
   `$D run --worktree --no-prompt LIBRE-2 LIBRE-3`: two worktrees branched from `origin/<LIBRE-1 branch>`
   (`git -C <ws> merge-base --is-ancestor origin/<LIBRE-1 branch> HEAD`).
3. **Subtask PR**: open it from the UI; `gh pr view --json baseRefName` shows the parent
   task's branch. Merge shows "Squash"; merge it.
4. **Progress**: within one PR poll (set `DELILAH_PR_POLL_SECS=5` for the preview), the
   parent task shows "1/2"; `LIBRE-3` shows "1 commit behind" against the parent task's
   branch and "Update branch" brings it in. `LIBRE-2` is archived.
5. **Parent task PR**: "Open PR" on `LIBRE-1` → opened as draft (offer of an empty commit
   if the branch had no commits). Merge `LIBRE-3` → the screen and the Inbox suggest "Mark
   ready"; the button runs `gh pr ready`. Merge shows "Merge commit" (project setting).
6. **Archive rules**: with `LIBRE-3` still live, `$D archive LIBRE-1` and `$D rm LIBRE-1`
   are refused, naming `LIBRE-3`.
7. **Convert / attach**: `$D new --free --title "Old idea" --project <name>` → `LIBRE-4`; `$D make-parent-task LIBRE-4`
   works; `$D new --free --title "Loose" --project <name>` → `LIBRE-5`; `$D attach LIBRE-5 --parent LIBRE-4` works;
   attaching a started task is refused with `already-started`.

GitHub operations need a real remote: the e2e tests use the fake `gh` of spec 005; by hand,
use a scratch repo on the account `gh auth status` shows (confirm it first).
