# Status

What Delilah does today, what is pending, and the known problems. Update this
page when a feature lands or a decision is taken. Last updated: 2026-10-21
(version 0.3.0).

## Built

**Core (MVP-0):**
- The Cargo workspace and the task state machine.
- Tasks in SQLite behind the HTTP API.
- Parent workspaces with selective submodules, and plain repo projects.
- Claude in a PTY, with `delilah attach`.
- Pausing on usage limits and resuming after the reset.
- macOS notifications.
- The CLI: `run`, `new`, `ls`, `attach`, `usage`, `resume`, `done`, `archive`, `rm`, `project`.

**Jira:**
- Cloud, Data Center and sample data, with the token in the Keychain.
- An Orca-style list: assigned or reported, open or done, grouped by status or epic, with sub-tasks under their parent and a current-sprint filter.
- "Analizar con Claude", and submodule suggestions from a read-only Claude pass.

**Desktop app** (Tauri + React):
- The daemon is bundled and run by launchd.
- Native window, configurable shortcuts and a folder picker.
- A board by phase, and spec tracking per task.
- A task screen in Orca's style: tabs (task, sessions, Cambios, files), the project tree, ⌘P, and tabs remembered per task.
- Cambios (diffs), a CodeMirror editor that reloads on disk changes, and a Markdown preview.
- The Bandeja, with replies, AskUserQuestion answers (with a configurable panel position) and review suggestions.
- Live updates over WebSocket, the Dock badge, and notifications.

**Work modes:**
- Tasks in the user's checkout (the default) or in a worktree.
- Free tasks (`LIBRE-n`), and a backlog with notes.
- Deleting tasks.
- Project sessions (read-only Claude in the checkout), which can be turned into a task.

**PR loop:**
- Delilah finds the PR of a task's branch with the user's `gh` (no token of its own), per repo for a parent, and the user can link one by URL or unlink it.
- A "PR" tab with the title, state, CI checks, review state and comments, and a PR badge on the board and the sidebar.
- The Bandeja shows failed CI, changes requested and new comments (from anyone but the signed-in account), and "Pedir al agente que lo arregle" sends them to the agent, only when the user chooses.
- "Abrir PR": pushes the branch and opens the PR from a proposal, per submodule for a parent; offered while any repo of the task has no open PR.
- A task whose PRs are all merged is done and archived on its own, unless that would lose work (every repo is checked, also those without a PR, which keep their branch); one notification per merge; a half-finished archive is finished by a manual archive.

**Other:**
- Spanish and English, including what Delilah tells Claude.
- Help › Ver logs.
- Optional Remote Control for Claude sessions.
- Previews (`scripts/preview.sh`) to try a branch next to the installed app.
- Phases moved by the user, by spec files and by agent signals; archiving that never blocks in the checkout, and a forced archive for worktrees.

## Pending and known problems

- **The real usage-limit payload** has never been observed. When it first happens on the work Mac, check `hooks.jsonl` against `delilah-agents::limits` (see the [spike](spikes/2026-10-19-claude-pty.md)).
- **SpecKit dirties the checkout.** Each `/speckit-plan` writes its block into CLAUDE.md, and a dirty checkout blocks new checkout tasks. Proposed: ignore that block in the clean-checkout check.
- **The archive refusal message** (`workspace: . has uncommitted changes…`) names neither the task nor the files. It should list them.
- **Moved state:** the installed app's state was copied from `fixtures/dev-home` on 2026-10-20. Those tasks' worktrees still live in `fixtures/dev-home/workspaces/` (absolute paths in the DB): do not delete that folder. The fixture `PROJ-*` tasks need `DELILAH_ALLOW_FILE_PROTOCOL`, which the installed daemon lacks.
- **Live sessions lost:** FEAT-4 and FEAT-5 lost their live sessions when the dev daemon was killed. Their conversations can be resumed from the task.
- **Optional next steps for previews:**
  - a "Delilah Preview" Tauri build (another identifier and port) for native-only features;
  - a per-task "Preview" button that runs the project's preview command next to the terminal;
  - a browser MCP so the agent inside a task can see the preview UI.
- **PR loop follow-ups:**
  - cross-linked PRs of a changeset (each PR names the others);
  - saving lessons to QMD when a task merges;
  - "Abrir PR" against a base other than the default branch.
- **Mobile access:** design only, waiting for the user's answers (see [vision.md](vision.md)).
- **Everything else in [vision.md](vision.md):** more agents, portable context, declarative workflows, `delilah.toml`, gates and budgets.
- **Testing:** test the whole flow on the work Mac with a real ticket.
