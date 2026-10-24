# Status

What Delilah does today, what is pending, and the known problems. Update this
page when a feature lands or a decision is taken. Last updated: 2026-10-22
(version 0.5.0: autopilot and task previews, on `feature/LIBRE-9`; per-project settings, on `feature/LIBRE-2`).

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

**Branch names:**
- A task's branch is named from its title: Claude (haiku) proposes a short name, with fallbacks to a slug of the title and then to the key. Jira tasks get `feature/<KEY>-<short-name>`, free tasks `feature/<short-name>`.
- A taken name (local or remote, in any repo of the workspace) gets `-2`, `-3`…; a branch is never reused or renamed.
- A "Rama" field in the start dialog shows the proposal and accepts a name; `delilah run --branch` is normalised the same way.
- `Task.branch` (from the stored workspace) is shown on the task view.

**Project settings** (spec `004-project-config`, personal to the install, in `projects.settings`):
- A settings screen (Proyectos › Ajustes) and `delilah project show|set|unset`: default workspace mode, optional branch template, shared entries, setup command, always/never submodules.
- A project's optional branch template (null by default, then the proposal above applies) decides the branch format; the "Rama" field of the new task form shows the template's branch at once and then the daemon's answer (decision 49); `delilah new|run` take `--checkout` / `--worktree`.
- Shared entries are copied or linked into new worktrees.
- A setup command runs in the new worktree as a read-only Setup session; a failure leaves the task in `setup-failed` with retry, skip and cancel (UI banner and `delilah setup`).

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
- See [architecture.md](architecture.md) and decisions [#38](decisions.md) to [#48](decisions.md).

**Dev workflow** (for working on this repo, not a product feature): CI on every PR (`.github/workflows/`), fmt/clippy/tests gating each implementer (`.claude/verify.sh`), a hook guarding the installed app, `scripts/bump-version.sh`. Decisions [#49](decisions.md) and [#50](decisions.md).

**Previews of a task** (a project's `preview_command`, a field in the project view): started by the daemon in the task's workspace, URL detected from its output, stopped with the task. "Lanzar" / "Parar" and the URL in the task header. For this repo set it to `scripts/preview.sh`.

**End-to-end tests:**
- `scripts/e2e.sh` runs 17 flows against a fake Claude, `gh` and Jira, each with its own daemon, temp home and free port: 11 API/CLI (Rust, `crates/delilah-e2e`) and 6 UI (Playwright, `app/e2e`, plus a smoke check). Local only, no CI. See [development.md](development.md#end-to-end-tests).

**Auto-update** (0.7.0):
- Tauri updater against GitHub Releases: a notice with the open-session count, "Más tarde" / "Cuando terminen", menu item and Ajustes › Actualizaciones; release builds only.
- The daemon copies the database to `backups/` before migrating after a version change.
- `make release` / `make release-dry-run` and the tag workflow `.github/workflows/release.yml`.

## Pending and known problems

- **End-to-end tests, pending and known:**
  - the UI quick-open test covers the file quick open (the UI has no task switcher);
  - the UI layer shares one daemon per run (tests are serial and use their own repos and projects);
  - the real-Claude smoke test (`--real-claude`) is opt-in, uses the subscription and is not run by default;
  - measured: API layer 57 s (including the build), UI layer 30 s; 10 consecutive full `scripts/e2e.sh` runs all green (78–105 s each), no flaky tests; the installed Delilah stayed untouched (same PID, health OK) and an interrupted run (Ctrl-C) leaves no daemons behind.
- **Auto-update, pending:**
  - the maintainer's key and secrets (T005 of `specs/005-app-auto-update`): until the public key replaces `REPLACE_WITH_UPDATER_PUBKEY`, `check.mjs` refuses to release;
  - the first tagged release, which verifies the workflow end to end (draft lookups, asset name, relaunch into the new version, daemon and plugin version);
  - next step: `vX.Y.Z-rc.N` tags published as GitHub prereleases (no `--latest`, so `releases/latest` and installed apps ignore them) to rehearse the pipeline. Needs the workflow to accept `-rc` tags and publish with `--prerelease`, and `check.mjs`, `manifest.mjs` and `versions.mjs` to accept the suffix. No update channel in the app: one fixed endpoint;
  - third-party actions are pinned to major tags, not SHAs.
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
  - a preview's child process that ignores SIGTERM once its leader has exited is left running ([#44](decisions.md));
  - `handoff` and `autopilot` call each other (as do `autopilot::pause` and the inbox's types): when they grow, give `handoff` a small policy hook that autopilot implements;
  - no time or token caps: a running autopilot is bounded only by one nudge per step and the usage limit;
  - an autopilot stored before `review_session` existed does not auto-recover a finish;
  - a destructive option the agent marks "(Recommended)" would be picked automatically ([#43](decisions.md));
  - the end-to-end run with a real Claude (quickstart) is tried by hand in a preview.
- **Optional next steps for previews:**
  - a "Delilah Preview" Tauri build (another identifier and port) for native-only features;
  - a browser MCP so the agent inside a task can see the preview UI.
- **Project settings, open items:**
  - the settings screen, the setup banner and the new task form were never checked in a browser during that feature's reviews (API and CLI were);
  - low: no test for the "Ajustes guardados" confirmation; a PATCH that changes a project's kind does two non-atomic writes; the Spanish "unknown submodule" message omits the path; the CLI's 422 output is untested end to end; no migration test over pre-existing rows; `ß`/`Æ` become hyphens in branch slugs;
- **PR loop follow-ups:**
  - cross-linked PRs of a changeset (each PR names the others);
  - saving lessons to QMD when a task merges;
  - "Abrir PR" against a base other than the default branch.
- **Mobile access:** design only, waiting for the user's answers (see [vision.md](vision.md)).
- **Everything else in [vision.md](vision.md):** more agents, portable context, declarative workflows, `delilah.toml`, gates and budgets.
- **Testing:** test the whole flow on the work Mac with a real ticket.
