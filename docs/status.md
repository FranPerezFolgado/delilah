# Status

What Delilah does today, what is pending, and the known problems. Update this
page when a feature lands or a decision is taken. Last updated: 2026-10-22
(version 0.5.0: autopilot and task previews, on `feature/LIBRE-9`).

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
- The task header shows the phase (and done/total while implementing) before the status; hidden for backlog and finished tasks.
- A task screen in Orca's style: tabs (task, sessions, Cambios, files), the project tree, ⌘P, and tabs remembered per task.
- Cambios (diffs), a CodeMirror editor that reloads on disk changes, and a Markdown preview.
- The Bandeja, with replies, AskUserQuestion answers (with a configurable panel position) and review suggestions.
- Live updates over WebSocket, the Dock badge, and notifications.

**Work modes:**
- Tasks in the user's checkout (the default) or in a worktree.
- Free tasks (`LIBRE-n`), and a backlog with notes.
- Deleting tasks.
- Project sessions (read-only Claude in the checkout), which can be turned into a task.
- "Nueva tarea con Claude": an idea conversation per project, shown as a chat view (no terminal) with Claude's replies, question cards, a text field, queued messages and "Detener". Claude ends with a `Nueva tarea` confirmation card; `Crear` makes a backlog task with the brief as notes and the history event "Creada desde la conversación «label»". Project sessions can create tasks the same way, task sessions cannot. The history survives restarts (`--resume`). See [architecture.md](architecture.md) and decisions [#34](decisions.md), [#35](decisions.md), [#36](decisions.md).

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

**Saving mode** (a fresh Claude session per phase, opt-in per task):
- Option on new tasks, backlog items and running tasks (also `--saving-mode` / `--no-saving-mode` in the CLI), with its default and "save notes first" in Settings.
- The session chain on the task screen; the planning → implementation handoff is offered (task screen and Bandeja), the review starts on its own and is announced.
- Recovery of an interrupted handoff on the scheduler tick; retry after a failure.

**Autopilot** (a task runs plan → implement → review → draft PR on its own; opt-in per task, spec workflow only, implies saving mode):
- Option on new tasks, backlog items and running tasks (`--autopilot` / `--no-autopilot`, `delilah autopilot <KEY> on|off|resume`), a mark in the task header, and the chain's preview and PR steps.
- The implementation starts without asking; a stalled step gets one nudge; questions with one "(Recommended)" option are answered in the hook; permission prompts, other questions and a blocked review pause it into a Bandeja item (*Reanudar autopiloto* / *Seguir a mano*).
- The review writes an outcome file; if ready, the project's preview starts and a draft PR opens per repo (title and body from the review, a loopback preview URL appended). Interrupted finishes recover on the scheduler tick.
- "Crear y lanzar en autopiloto" on the idea card.
- See [architecture.md](architecture.md) and decisions [#37](decisions.md) to [#47](decisions.md).

**Previews of a task** (a project's `preview_command`, a field in the project view): started by the daemon in the task's workspace, URL detected from its output, stopped with the task. "Lanzar" / "Parar" and the URL in the task header. For this repo set it to `scripts/preview.sh`.

## Pending and known problems

- **Idea conversations, still missing:**
  - streaming of Claude's text and a chat view for other sessions (only the idea conversation has one; the rest show the terminal);
  - deleting conversations (the history is kept for good);
  - the usage-limit notice does not show in the conversation view;
  - the "Creada desde la conversación" label is not shown in the backlog view, only once the task starts;
  - `/hooks/claude` has no authentication, so a local process that knows a project session id could forge a `Nueva tarea` card (the user still has to click `Crear`).
- **Saving mode, pending:** one session per orchestrate phase (cutting inside implementation).
- **Saving mode, known problems:**
  - the old live session keeps `primary = true` in memory until it exits (~3 s);
  - `PATCH` of `saving_mode` is not atomic with the other backlog fields;
  - an unparsable `handoff` JSON fails the whole task list;
  - the English history says "Moved to Implement" (it reuses the phase-name keys);
  - a CLI older than the daemon fails on `step-started` events: ship them together;
  - rolling the daemon back after using saving mode needs the `step-started` rows tolerated.
- **The real usage-limit payload** has never been observed. When it first happens on the work Mac, check `hooks.jsonl` against `delilah-agents::limits` (see the [spike](spikes/2026-10-19-claude-pty.md)).
- **SpecKit dirties the checkout.** Each `/speckit-plan` writes its block into CLAUDE.md, and a dirty checkout blocks new checkout tasks. Proposed: ignore that block in the clean-checkout check.
- **The archive refusal message** (`workspace: . has uncommitted changes…`) names neither the task nor the files. It should list them.
- **Moved state:** the installed app's state was copied from `fixtures/dev-home` on 2026-10-20. Those tasks' worktrees still live in `fixtures/dev-home/workspaces/` (absolute paths in the DB): do not delete that folder. The fixture `PROJ-*` tasks need `DELILAH_ALLOW_FILE_PROTOCOL`, which the installed daemon lacks.
- **Live sessions lost:** FEAT-4 and FEAT-5 lost their live sessions when the dev daemon was killed. Their conversations can be resumed from the task.
- **Autopilot, known limits:**
  - previews are lost on a daemon restart (memory only, not adopted again);
  - no time or token caps: a running autopilot is bounded only by one nudge per step and the usage limit;
  - an autopilot stored before `review_session` existed does not auto-recover a finish;
  - a destructive option the agent marks "(Recommended)" would be picked automatically ([#42](decisions.md));
  - the end-to-end run with a real Claude (quickstart) is tried by hand in a preview.
- **Optional next steps for previews:**
  - a "Delilah Preview" Tauri build (another identifier and port) for native-only features;
  - a browser MCP so the agent inside a task can see the preview UI.
- **PR loop follow-ups:**
  - cross-linked PRs of a changeset (each PR names the others);
  - saving lessons to QMD when a task merges;
  - "Abrir PR" against a base other than the default branch.
- **Mobile access:** design only, waiting for the user's answers (see [vision.md](vision.md)).
- **Everything else in [vision.md](vision.md):** more agents, portable context, declarative workflows, `delilah.toml`, gates and budgets.
- **Testing:** test the whole flow on the work Mac with a real ticket.
