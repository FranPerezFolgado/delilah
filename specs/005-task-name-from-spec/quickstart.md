# Quickstart: trying it in a preview

```bash
scripts/preview.sh --fresh        # prints UI http://localhost:150N and API http://127.0.0.1:750N
```
Register the fixture repo and the fixture parent (`scripts/make-fixture-parent.sh`) as
projects; both need Spec Kit with its git extension (`specify init . --integration claude`)
for scenario 1, and one without the extension for scenario 2.

1. **Hook creates the branch (repo, worktree mode)**: new free task "cosas de ramas",
   worktree. Before specify: `git -C <preview-home>/workspaces/LIBRE-1 branch --show-current`
   is empty and `git branch` in the repo has no `feature/LIBRE-1`. Let `/feature` run
   specify. Within seconds: the worktree is on `NNN-<name>`, the task's title is the
   spec's, the task screen shows "Antes: cosas de ramas" and "Restaurar".
2. **No hook (repo, checkout mode)**: same in the project without the git extension:
   the branch appears named like the spec folder; the checkout's original branch got
   no commits.
3. **Guard**: new free task, tell the agent to edit a README line and commit without
   specifying: the branch `<slug-of-title>` exists before the commit; `main` unchanged.
4. **Parent**: fixture parent, worktree mode, one submodule: after specify, the
   parent's worktree and the submodule are on the same `NNN-<name>`.
5. **Restart**: stop the preview after the spec is written but before the turn ends
   (or remove the branch record by hand), start it again: within one tick the task has
   its branch and title.
6. **PR and archive**: open a PR on task 1 (fixture remote), archive it; delete a
   branchless task.
7. **Title**: edit the title of a running task in the header; write another spec in it:
   the title doesn't change.

Native bits (none here) need the Tauri app.
