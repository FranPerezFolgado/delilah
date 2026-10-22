# Decisions

A log of the decisions behind Delilah, newest concerns last. Each one says what
was chosen, why, and what it implies. The original sketch, with ideas not built
yet, is summarised in [vision.md](vision.md). How things work today is in
[architecture.md](architecture.md).

## Product

**1. A local desktop app, one per person.** Each developer installs it and uses
their own agent subscriptions and accounts. It is not a cloud service, not a
replacement for the company's own agent, and not a full IDE: the editor is for
reviewing and touching up.

**2. Orca as the model, lighter and for submodules.** Delilah copies Orca's ideas:
- agents as CLIs in terminals;
- state from hooks;
- a daemon that outlives the window;
- the board, Bandeja and tabs.

What it adds: workspaces of a parent repo with submodules, automatic resume after
a usage limit, and starting from a Jira ticket. Forking Orca (about 2M lines of
Electron) was ruled out.

**3. Delilah wraps the user's own Claude.** It launches the `claude` the user
already has configured, with their skills, MCPs and status line. Delilah only
adds its hooks through `--settings`, and its status line script runs the user's
own one after forwarding the usage data. (The user asked for this explicitly:
"Delilah has to be a wrapper of my Claude".)

## Stack

**4. Rust core + Tauri 2 shell + React UI.**
- Tauri uses the Mac's WebKit, so the app is around 10 MB instead of Electron's 150 MB.
- The code is written by Claude, so the stack favours well-documented crates and many tests.
- Electron stays as plan B: the UI only uses the HTTP/WS API (decision 6), so moving would take days.
- GPUI was ruled out: building the editor and terminal from scratch costs too much.

**5. Git through the `git` CLI, SQLite through sqlx, axum for the API.** The git
CLI handles worktrees and submodules more reliably than the libraries do.
`portable-pty` gives one PTY per agent inside the daemon.

## Architecture

**6. API-first daemon.** `delilahd` does all the work. The app, the CLI and a
future mobile client are equal clients of `127.0.0.1:7420`. The UI never calls
Tauri commands for data, and `platform.ts` holds the only Tauri imports.

**7. The daemon is a LaunchAgent run from the app bundle**, as Orca does. It
starts at login, restarts after a crash, and keeps agents working with the window
closed. Opening a newer app restarts it with the new binary.

**8. Agent state comes from hooks, not from reading the screen.** Claude's hooks
`curl` JSON to the daemon, and the daemon is the only source of truth. The
terminal is still there for the user to talk to the agent directly. Reading the
TUI's text was rejected as fragile; the one exception is accepting the
folder-trust dialog.

**9. Hooks live only in a settings file outside the repo** (`tasks/<KEY>/`, passed
with `claude --settings`). They never go in the worktree or the global
`~/.claude`, so nothing leaks into commits or into the user's other sessions.

**10. Agents run with a clean environment.** A Claude started with another Claude
session's variables (`CLAUDE_CODE_CHILD_SESSION`…) does not save its transcript,
so `--resume` fails. The environment keeps only the login shell's PATH and proxy
or CA variables.

**11. Live updates announce topics only.** The WebSocket says what changed
(`{"type":"inbox"}`) and the UI refetches that endpoint. Data has one shape and
one place, and a client that missed messages just refetches.

**12. Security without authentication (for now).**
- The daemon accepts loopback connections only.
- An `Origin` guard allows only the app (plus `DELILAH_UI_ORIGIN` for previews).
- A `Host` check stops DNS rebinding.
- Ticket keys are validated before they name folders or go into hook commands.
- File access is confined to the workspace, symlinks included.
- The Jira token lives in the Keychain.

Mobile access (via Tailscale) will need pairing tokens first.

## Agents

**13. Questions with options go through AskUserQuestion and a PreToolUse hook.**
The sketch planned a Delilah `ask_user` MCP tool. Instead, Claude's own
AskUserQuestion is intercepted: the hook holds the question (timeout 86400 s) and
returns the user's choice as `updatedInput.answers`. That works with the user's
own skills unchanged, and if the daemon is away the question just shows in the
terminal.

**14. Usage limits are read from the status line's `rate_limits`.** A turn that
ends at 100 % pauses the task until the reset, and the scheduler resumes it with
a "carry on" prompt. Because the subscription is shared, a limit hit by any
session pauses the task. The real `StopFailure` payload for a limit has not been
observed yet (see the spike).

**34. The idea conversation runs with `dontAsk` and an explicit allow list.**
`--permission-mode dontAsk` plus `permissions.allow`/`deny` in the session's
settings file lets Claude read code without a terminal prompt. Allowed: `Read(./**)`
(the checkout) plus `Read(//<dir>/**)` for each `--add-dir`, AskUserQuestion,
`Skill(delilah:task-from-idea)`, TodoWrite, `git status` and `ls`, each Bash rule also
in its RTK form (`rtk git status:*`, ...): user-level PreToolUse hooks such as RTK
rewrite commands and allow rules match the rewritten command. Denied: Edit,
MultiEdit, Write, NotebookEdit and reading secrets (`~/.ssh`, `~/.aws`, `~/.gnupg`,
`~/.config/gh`, `.env` files), so a prompt injection in the repo cannot copy them
into a brief. `--setting-sources user` drops the repo's `.claude/settings*.json`, so
a cloned repo cannot widen the list; the user's own settings and hooks stay, so
broad allow rules in `~/.claude/settings.json` still apply. `--strict-mcp-config`
keeps project and user MCP servers out. `rg` and `git log/show/diff` were dropped:
`rg --pre` runs programs and `--output` writes files. Checked against claude 2.1.292
(which has no separate Grep/Glob tools). Every session's prompt goes after `--`, so
a prompt starting with `-` is never read as an option.

**35. Tasks from ideas: the daemon creates them on the confirmation card.**
Claude does not create the task. It shows a `Nueva tarea` AskUserQuestion card
(title and brief in the `Crear` option's preview); when the user answers `Crear`, the
daemon creates the backlog task and then returns the answer, with the key, to Claude.
So a task exists only after a click, and the same code serves the idea
conversation and project sessions. Rejected: Claude calling the `delilah` CLI or
`curl` (needs Bash permissions, and nothing stops it from creating tasks unasked)
and a Delilah MCP tool (a new server to run and configure, for what the existing
ask hook already does). The brief is written by Claude, so it can be edited before
the task starts.

**36. Idea conversations: a chat view over the PTY fed by hooks; `dontAsk` instead
of plan mode; a plugin passed to project-owned sessions.**
- **Chat view over the PTY.** The conversation is still an interactive Claude in a PTY, so `--resume`, hooks and the terminal fallback keep working. The view is built from hook events (`UserPromptSubmit`, `Stop`, the ask). Left for later: `stream-json`, which would give streaming text but replaces the PTY.
- **`dontAsk`, not plan mode.** Plan mode asks to leave the plan, and read-only commands prompt. See [#34](decisions.md) for the allow list.
- **A session-only plugin.** The `task-from-idea` skill ships in `<home>/plugins/delilah`, passed with `--plugin-dir` to project-owned sessions only (task sessions never get it), so nothing is installed in the user's Claude.
- **Spike findings** (`specs/004-task-from-idea/research.md`, claude 2.1.292):
  - Edit and Write in `deny` are removed from the session, and `npm install` is refused without a prompt;
  - a user-level hook that rewrites commands (RTK) breaks allow matching, hence the RTK forms in #34;
  - AskUserQuestion reaches PreToolUse in `dontAsk`, with the preview intact, and the answer returns verbatim (accents and dashes included);
  - **no `Stop` hook fires after ESC**, so the daemon ends the turn itself on "Detener";
  - the plugin's skill is listed and runs, both typed (`/delilah:task-from-idea`) and through Claude's `Skill` tool.

## Workflows and tasks

**15. SpecKit is the first workflow, through the user's `/feature` wrapper.**
Delilah starts `/feature <ticket or text>` and follows the spec files. It never
drives SpecKit's phases itself, and the repo stays the source of truth.

**16. Work in the checkout by default; worktree as an option; project sessions to
ask.**
- **(A) Checkout.** Most work happens in the developer's own folder, as they would by hand: a branch there, the checkout clean to start, one task at a time.
- **(B) Worktree.** For parallel work.
- **(C) Project sessions.** Questions about the code are not tasks: a project session (`@project`) runs Claude read-only (`--permission-mode plan`) in the checkout. It can be allowed to edit, or turned into a free task, keeping the conversation.

**17. Phases: the user decides; files and signals only move forward.** The user
sets any phase (the phase bar, or asking for a review).
- **Forward only:** SpecKit files, the first code edit (implement) and `gh pr create` or `git push` (review) only advance the phase, so they never undo the user's choice.
- **Suggest, don't move:** when the agent stops with commits on its branch, the Bandeja suggests review instead of moving the task. Commits alone do not mean "done".

**18. Archiving never traps a task in the checkout, but a worktree asks before
losing work.**
- **Checkout:** the user's own folder is never deleted, so archiving always lets go of it. Uncommitted files simply stay.
- **Worktree:** removing it deletes unpushed work, so it needs an explicit force ("Archivar igualmente", `--force`).

**19. The backlog is the `Queued` status.** A task can be saved with notes and a
chosen place (checkout or worktree) and started later. A failed start goes back
to the backlog (`StartAborted`).

## UI

**20. Hand-rolled i18n.** `t("texto en español")`, with English in `i18n/en.ts`
keyed by the Spanish text. It needs no library, and a missing translation shows
the Spanish text instead of breaking. What Delilah writes to Claude is translated
too, so the agent answers in the user's language. The shell's menu language is
read from the daemon at start.

**21. Notifications come from the app only, never Telegram or similar.** The
daemon notifies while the app is closed, and the UI notifies (with the agent's
words) while it is open but in the background. The Dock badge counts what blocks
an agent.

## Working on Delilah

**22. Delilah is built with Delilah, so changes are tried in previews.** The
installed app (7420, `~/.delilah`) runs the agents working on this repo.
`scripts/preview.sh` gives each checkout its own daemon (750N), UI (150N) and
state. Nobody restarts, stops or `pkill`s a `delilahd` to try something; one such
`pkill` once killed live agent sessions.

**23. Specs live in the repo.** `specs/` and `.specify/` are committed and pushed;
they used to be local-only. The block SpecKit writes into CLAUDE.md is not kept:
it points at whichever feature is current.

**24. Develop on the personal Mac, test on the work Mac.** Nothing from the work
repos is ever referenced or copied. Development uses
`scripts/make-fixture-parent.sh` (a parent with four fake submodules over
`file://`) and Jira sample data.

**25. Git account and pushing.** Git and gh here run as the personal account
(FranPerezFolgado). The machine also has work credentials, so confirm the account
before any remote operation. Agents commit finished work without asking, and the
user pushes. Commit messages carry no `Co-Authored-By: Claude` trailer.

## Pull requests

**26. GitHub through the user's `gh`, with no token in the daemon (2026-10-20).**
Delilah runs the `gh` the user already has, on the login shell's PATH, behind a
`Gh` trait so tests and previews can swap it (`DELILAH_GH_BIN`). `gh` already
holds the tokens, honours `gh repo set-default` and talks to Enterprise hosts
like the agent's own `gh pr create`. Storing a GitHub token in the daemon
(Keychain or settings) and calling the API with `reqwest` or octocrab was ruled
out: it copies a credential and adds a client. When `gh` is missing, signed out
or without access, the PR tab says so and the Bandeja notifies once.

**27. PRs are polled every 30 s, not pushed by webhooks (2026-10-20).** The daemon
is on loopback behind no public address, so GitHub cannot reach it; a tunnel or
a GitHub App would be new infrastructure and a new credential. One GraphQL call
per open PR every 30 s is far under the rate limit, with back-off after a
failure and an immediate check when a task is opened or the agent pushes.

**28. PR notices are rows, not derived state (2026-10-20).** The inbox items for
CI, reviews and a blocked archive live in `pr_notices`, unique per cause and
resolved rather than deleted. Unlike session attention they cannot be recomputed
from the snapshot: one already sent to the agent or dismissed must stay gone
after a restart.

**29. Delilah never forwards PR feedback to the agent by itself (2026-10-20).**
A failed CI or a review only reaches the Bandeja; "Pedir al agente que lo
arregle" sends it, as comments to consider, not as instructions from the user.
Auto-forwarding was ruled out: it would spend the subscription and let a
reviewer's text steer the agent unseen.

**30. Notify for comments from anyone but the signed-in account (2026-10-20).**
Other people and bots count; only the user's own comments are skipped. Filtering
bots was ruled out, since a CI bot's review is often the thing to read.

**31. A merged task archives without asking, unless that would lose work
(2026-10-21).** When every linked PR is merged the task is marked done and
archived at once. Asking first was ruled out: the merge is the confirmation. The
guard stays: every repo with a PR must be clean and its local branch contained
in the PR's head (so squash and rebase merges pass), else an `archive-blocked`
item offers a forced archive. The task's local branches are deleted at the
verified sha, never the parent's and never a remote one. A branch left behind
raises `archive-partial`, and archiving by hand finishes it. This overrides decision 18's "a checkout never refuses" for the
merge archive only.

**32. Repos without a PR never lose work; the merge archive checks every repo
(2026-10-21).** A merge in one repo must not archive away another that has no PR:
the check covers every repo of the workspace. One without a PR blocks the archive
if it is dirty or has unpushed commits, and keeps its branch. A failed fetch blocks
too, and the workspace is released without force. Deleting a branch compares it to
the verified sha first, so a commit made after the check survives.

## Saving mode

**33. Saving mode: a fresh session per phase.** It relaxes decision 15, which said
Delilah never drives SpecKit's phases: Delilah still never drives what happens inside a
phase, it only starts a new session between phases. With the option on, each phase (spec and
plan, implementation, review) runs in its own Claude session, started from the spec
files and git, which already hold everything the next phase needs.
- **Why:** every turn re-reads the whole conversation. On LIBRE-3 (153 turns) the context grew from ~44k tokens to ~364k, ~33M re-read against 130k written. Estimated saving by cuts: ~37 % with one (planning → implementation), ~49 % with two (+ before the review), ~60 % cutting between implementation phases too. Each session pays its own base (~45-60k), so the returns diminish. Measure it per task from the sessions' transcripts (research R8, [quickstart](../specs/004-saving-mode/quickstart.md)); target: 30 % fewer tokens.
- **Opt-in per task**, default from a preference: on a high usage limit it only adds handoffs.
- **Planning → implementation asks the user** (it also approves the tasks); the review starts on its own. Notes are saved with `/save-session` first (preference).
- **Ruled out:** returning to the parent session (it carries the context we are shedding); cutting inside the orchestrate phase, one session per implementation phase, for now (pending in [status](status.md)).

## Autopilot

**37. Autopilot is a policy over saving mode, not a second engine (2026-10-22).** A task
with autopilot runs plan → implement → review → draft PR by itself. `handoff.rs` keeps
owning the step sessions, the persisted handoff and recovery; `autopilot/` only changes
who confirms a handoff (`shape`), what each session is told, and what happens after the
review. State is one nullable JSON column, `tasks.autopilot`, written before each side
effect, so a restart repeats nothing; it implies saving mode.
- **Ruled out:** a parallel state machine on the same hooks (two owners of the primary session, double starts); editing the user's global skills with an `--autopilot` argument (outside this repo; the session prompt already overrides their stops, and the skills can learn it later).

**38. One nudge per step, and recommended answers inside the hook (2026-10-22).** This
stretches #15 and #33 (Delilah never acts inside a phase): after a turn end that reaches
no boundary, the primary session gets one typed nudge per step, and a second stall pauses.
The workflow's approval stops are plain text, so without it nearly every plan step would
pause; unlimited nudges could loop. A question whose every item has exactly one option
labelled "(Recommended)" is answered in the AskUserQuestion hook (no inbox item, history
`autopilot-answered`); any other question pauses. Delilah still never drives the
workflow's own phases.

**39. Autopilot pushes and opens a draft PR with the user's own `gh` (2026-10-22).** It
does so without asking at that moment, like "Abrir PR" (#26): the user opted in per task,
the PR is a draft, and nothing is merged. The stage `opening_pr` is persisted and PRs are
re-detected first, so a restart never opens a second one.

**40. The daemon owns previews, not the agent (2026-10-22).** A preview an agent starts
outlives its session with nobody to stop it. The daemon runs the project's
`preview_command` in the task's workspace, stops it with the task (done, failed, archived,
deleted) and at shutdown, and keeps it in memory only: a restart does not adopt it. It
runs in a login shell because launchd's PATH lacks cargo and npm, with the agents' clean
environment, so the daemon's own variables (tokens) do not reach the dev server. The
command is trusted user configuration: like an agent, it runs whatever it says, and what
guards it is the API's Origin and loopback `Host` checks, not the route.

**41. The PR body keeps loopback preview URLs, without userinfo, query or fragment
(2026-10-22).** The spec wants the preview URL in the PR, and for this repo it is
`localhost`. Credentials and tokens in a URL must not reach a possibly public PR, and
neither must a tunnel or LAN URL: a non-loopback host leaves the line out.

**42. The agent's own "(Recommended)" label is trusted; permission prompts are never
answered (2026-10-22).** Automatic answers pick the option the agent marked. Accepted
risk: a destructive option the agent marks as recommended would be picked. A permission
prompt always pauses the autopilot instead.

**43. Process groups are signalled with `/bin/kill -- -<pgid>` (2026-10-22).** The
workspace forbids `unsafe`, so no `libc::killpg`. The waiter task owns the child; a
delayed KILL is sent only if the leader has not been reaped, so a reused pid is never
hit.

**44. Autopilot state changes are compare-and-set (2026-10-22).** `Store::update_autopilot`
writes only if the stored value is still the one the closure saw, so a pause or "off"
that came in during a finish (a preview wait takes minutes) is never overwritten. The
finish's writes also check they still belong to the same review session.

**45. "Can have autopilot" means a task on the `feature` (spec) workflow (2026-10-22).**
Not a project session, not another workflow: without spec files there are no handoffs.
Enforced on every entry point (task create, backlog, `PATCH`, the autopilot endpoint, the
proposal card) with a 409 `not-spec-workflow`.

**46. Recovering an interrupted finish needs `Autopilot.review_session` (2026-10-22).** It
names the outcome file to read. Autopilots stored before the field existed do not
auto-recover a finish (the PR can still be opened by hand with "Abrir PR").

**47. Proposal cards reject only closed sessions (2026-10-22).** Spec 004 FR-013 lets plain
project sessions create tasks, so a card is not tied to an idea conversation; only a
closed session's card is refused.
