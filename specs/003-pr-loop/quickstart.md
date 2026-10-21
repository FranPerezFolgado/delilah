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

## Results (2026-10-21, preview slot 2, fake gh)

Checked through the daemon API and its log (the verifier had no browser tools, so the UI rendering is not covered here):

| Step | Result |
|---|---|
| 1–2 PR linked, snapshot and summary | ✅ API shows #7, open, base main, checks, `checked_at`; `/prs` summary for the badge |
| 3 failing head → one `pr-failed` item | ✅ exactly one, "CI failed: build" |
| 3 unchanged → nothing new | ✅ still one after ~50 s |
| 3 later green → item resolves | ✅ on a new head; same-head re-run fixed afterwards (WP-21) |
| 4 comment by another login / by me | ✅ one `pr-review` ("from alice"); own comment ignored |
| 5 "Pedir al agente" without a session | ✅ 409 with the message, item kept (button stays enabled per InboxView tests) |
| dismiss | ✅ 204, item gone |
| unlink + proposal without workspace | ✅ 204; 409 `no-workspace` |
| 8 signed out | ⚠️ the fake's GraphQL needs no auth; fixed so a failed account lookup shows the problem (WP-21) |
| 6–7 archive on merge, "Abrir PR" success | not checked here (need a workspace / real agent); covered by daemon tests |
