# Quickstart: trying fresh task bases

Never on the installed app (7420); in a preview of this branch.

```bash
scripts/preview.sh --fresh      # prints UI http://localhost:150N, API http://127.0.0.1:750N
export DELILAH_URL=http://127.0.0.1:750N
F=fixtures/preview-home-N/env   # the fixture it prints
```

1. **Stale local main** — register `$F/parent` (parent) and a clone of
   `$F/remotes/svc-orders.git` (repo). Push a commit to the remote from another
   clone:
   ```bash
   git clone -q $F/remotes/svc-orders.git /tmp/other && cd /tmp/other \
     && echo x > new.txt && git add . && git commit -qm "merged meanwhile" && git push -q
   ```
   Start a worktree task and a checkout task on the repo project: both branches
   contain "merged meanwhile" (`git log --oneline -1 feature/<KEY>`), the project's
   `main` does not, and `git -C <ws> rev-parse --abbrev-ref @{u}` has no upstream.
2. **Parent** — same push to `svc-payments`'s remote; start a task with
   `svc-payments` chosen: its branch contains the commit.
3. **Offline** — `git -C <repo> remote set-url origin /nonexistent`, start a task:
   it starts; the Bandeja shows "Rama posiblemente atrasada" naming the repo and
   the base; the task screen shows the same banner; "Descartar" removes both.
4. **Timeout** — `git -C <repo> remote set-url origin ssh://example.invalid/x` with
   `core.sshCommand="sleep 60"`: the start returns within ~30 s with the warning.
5. **Existing branch** — archive a checkout task (its branch stays), then start a
   new checkout task with `branch` set to that branch: it switches to it and the
   branch is not moved (same sha as before).

Checks: `cargo test -p delilah-git`, `cargo test -p delilah-daemon`,
`cd app && npx tsc --noEmit && npx vitest run`.
