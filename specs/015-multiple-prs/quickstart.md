# Quickstart: Track every PR a task opens

Try it in a preview, never against the installed daemon (port 7420).

```bash
scripts/make-fixture-parent.sh          # parent repo with submodules svc-a, svc-b
scripts/preview.sh --fresh              # prints UI http://localhost:150N, API http://127.0.0.1:750N
```

The preview uses the fake `gh` (`DELILAH_GH_BIN`); create PRs through it.

1. **Parent + submodule PRs**: register the fixture parent, start a task touching `svc-a`,
   let it make its descriptive branch (parent on the task's branch). Create a PR from the
   task branch in `svc-a` and one in the parent repo. Within 30 s the PR tab shows both,
   grouped under `svc-a` and `.`; the badge says 2.
2. **Agent's PR from another branch**: from the task's session run
   `gh pr create --head other-branch …` in `svc-b`; the PR appears with "agent" as its source.
3. **Outside repo**: Link by URL a PR of a repo not in the workspace; it is listed under
   `host/owner/name`.
4. **Two PRs in one repo**: a second PR in `svc-a` from another branch; both stay.
5. **Closing**: merge all but one → task stays in Review. Merge the last while the agent is
   mid-turn → nothing until the turn ends; then the task is archived once.
6. **Replaced**: close one PR unmerged → archive blocked notice; open a new PR from the same
   branch in that repo → old card shows "replaced by #N"; merging the rest archives the task.
7. **Upgrade**: start the preview on a copy of an old `fixtures/preview-home-N` with linked
   PRs → the same PRs and states show.

Checks:

```bash
cargo test -p delilah-git
cargo test -p delilah-daemon
cd app && npx tsc --noEmit && npx vitest run
```
