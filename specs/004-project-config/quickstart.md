# Quickstart: trying project settings

Always in a preview, never against the installed app (port 7420):

```bash
scripts/preview.sh --fresh        # prints the UI (http://localhost:150N) and API (http://127.0.0.1:750N)
export DELILAH_URL=http://127.0.0.1:750N
```

Register the fixture repo and the fixture parent the preview prints, then:

1. **Settings screen**: Projects → the repo → Ajustes. Set branch
   `feature/{ticket}-{slug}`, default worktree, shared `.env` (copy), setup
   `echo ok > .setup-ran`. Save; reopen; the values are there. `git -C <repo> status`
   is clean.
2. **Validation**: shared `../x`, branch `feature/{slug}`, branch `x/{foo}`: each shows
   its error next to the field; nothing saved.
3. **Start**: write `.env` in the fixture repo; new task "Paginación por cursor" →
   the form preselects worktree and shows the branch name. Start: a "Setup" tab shows
   the output; then Claude starts. The worktree has `.env` and `.setup-ran`.
4. **Failure**: set setup `exit 3`; start a task → "Setup falló" with the output and
   "Reintentar setup" / "Lanzar sin setup". Skip → Claude starts in the worktree.
5. **Submodules**: on the parent, always `a`, never `c`; the form shows `a` locked and no
   `c`; the workspace has `a`.
6. **CLI**: `target/debug/delilah project show <repo>`, `… project set <repo> setup
   'true'`, `… project unset <repo> setup`.
7. **No settings**: a project with nothing set starts tasks in the checkout on
   `feature/<KEY>`, as before.
