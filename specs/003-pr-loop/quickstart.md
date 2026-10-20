# Quickstart: trying the PR loop

Always in a preview, never against the installed app (port 7420).

## Without GitHub: a fake `gh`

`scripts/fake-gh.sh` (added by this feature) answers `gh` calls from JSON files in
`$FAKE_GH_DIR` (`pr-list.json`, `pr-<n>.json`, `repo.json`, `user.json`), so every
state can be produced by editing a file:

```bash
export FAKE_GH_DIR=$PWD/fixtures/fake-gh && mkdir -p "$FAKE_GH_DIR"
DELILAH_GH_BIN=$PWD/scripts/fake-gh.sh scripts/preview.sh --fresh
```

1. Register a fixture repo project, start a task, let it commit on its branch.
2. Write `pr-list.json` with an open PR for `feature/<KEY>` → within 30 s the task gets
   the PR tab and the badge (Story 1).
3. Change `pr-<n>.json`: a failed check → one Bandeja item `pr-failed`, Dock count +1;
   leave it → nothing new; a new head sha failing → a second item (Story 2).
4. Add a review comment by another login → `pr-review`; one by the `user.json` login → nothing.
5. "Pedir al agente que lo arregle" → the agent's terminal gets the prompt.
6. Set `"state": "MERGED"` → the task is done, its worktree and branch are gone (Story 4).
   Repeat with an uncommitted file in the worktree → `pr-blocked`, nothing removed.
7. Parent project (`scripts/make-fixture-parent.sh`): two submodules, two PRs; merge one,
   then the other (Story 5).
8. `user.json` missing / `gh` exiting with "not logged in" → the PR tab explains it,
   one notice only.

## With GitHub (optional, personal account)

Confirm the account first (`gh auth status` → FranPerezFolgado), then use a throwaway
personal repository as the fixture project's remote. Open a PR from the task with
"Abrir PR" (Story 3), check the Jira link with Jira sample data turned off, merge it on
github.com and watch the task archive itself.

## Checks

```bash
cargo test -p delilah-git -p delilah-daemon
cd app && npx tsc --noEmit && npx vitest run
```
