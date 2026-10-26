# Using Delilah

The reference for the command line, the desktop app, project settings, Jira and
sessions. For what Delilah is, see the [README](../README.md).

## Contents

- [Command line](#command-line)
- [Desktop app](#desktop-app)
- [Jira](#jira)
- [Sessions](#sessions)
- [Resource usage](#resource-usage)
- [Building from source](#building-from-source)
- [Uninstall](#uninstall)
- [Updates](#updates)

## Command line

The `delilah` command is a separate binary (crate `delilah-cli`). The `.dmg`, the app
tarball and `install.sh` do not put it on your PATH (the app bundles only the daemon,
`delilahd`, which it starts itself, and there is no "install CLI" menu item). To use it,
build it and link it yourself:

```bash
cargo build --release -p delilah-cli
ln -s "$PWD/target/release/delilah" /usr/local/bin/delilah   # or any folder on your PATH
```

```bash
# Terminal 1: the daemon (state in ~/.delilah, API on 127.0.0.1:7420)
cargo run -p delilah-daemon

# Terminal 2: register projects once
delilah project add ~/work/platform --kind parent   # parent repo with submodules
delilah project add ~/dev/projects/signal --kind repo # a single repo, Orca-style

# Run a task: builds the workspace and starts Claude Code on it
delilah run PROJ-1234 --project platform --submodules svc-payments,svc-orders --title "Card retries"
delilah run SIGNAL-12 --project signal --title "Better feed"

# Or save it to the project's backlog for later (no workspace, no agent), then start it
delilah new SIGNAL-13 --project signal --title "Feed cache" --notes "Look at the TTL first"
delilah new --free --project signal --title "Try a lighter JSON parser"   # named LIBRE-n
delilah run SIGNAL-13     # starts the backlog task in its project; its notes reach the agent

delilah new SIGNAL-14 --project signal --title "Feed cache" --worktree   # --checkout or --worktree; neither: the project's setting, else the checkout
delilah run SIGNAL-13 --checkout   # same flags; neither: the backlog task's choice, else the project's setting, else the checkout

# Chat mode (experimental): the agent runs without a terminal, the conversation is the only view
delilah run SIGNAL-15 --project signal --title "Feed tags" --chat   # also on `delilah new`
delilah runtime SIGNAL-15 terminal   # move a task's agent to the terminal and back (chat | terminal)
delilah runtime --session <ID> chat # only that session (no key needed); the task's setting stays the default

# Per-project settings (see "Project settings" below)
delilah project show signal                          # effective settings, defaults marked
delilah project set signal workspace worktree        # checkout | worktree
delilah project set signal branch 'feature/{ticket}-{slug}'
delilah project set signal setup 'npm ci'
delilah project set signal shared add .env --copy    # --copy | --link; neither: automatic
delilah project set signal shared rm .env
delilah project set signal submodules always add svc-common   # always|never add|rm <path>
delilah project unset signal setup                   # back to the default
delilah setup retry|skip|cancel SIGNAL-13            # when a task's setup failed or is running

delilah ls                 # tasks and their state
delilah attach PROJ-1234   # live terminal of the agent; Ctrl-] detaches
delilah usage              # subscription usage windows, as Claude reports them
delilah resume PROJ-1234   # resume a task paused by the usage limit right away
delilah done PROJ-1234     # mark it done and stop its agent
delilah reopen PROJ-1234   # bring a done task back (needs its workspace; not an archived one)
delilah archive PROJ-1234  # remove its workspace (a worktree with unpushed work needs --force)
delilah rm PROJ-1234       # delete the task for good: stops its agent, releases its workspace
                           # (the branch stays); a worktree with unpushed work needs --force
```

A task whose turn ends on a usage limit is paused until the window resets; the
daemon then resumes it on its own and tells the agent to carry on. The daemon
sends macOS notifications when a task needs you, pauses, resumes or ends
(`DELILAH_NOTIFY=0` turns them off).

To try Delilah without real repos, `scripts/make-fixture-parent.sh` creates a fake parent repo in `fixtures/parent-env/` (git-ignored) whose four modules (`svc-payments`, `svc-orders`, `svc-billing`, `common-lib`) are submodules backed by local bare repos. With it, start the daemon with `DELILAH_ALLOW_FILE_PROTOCOL=1`
(its submodules use `file://`).

## Desktop app

A Tauri 2 shell around a React UI (`app/`). The UI only talks to the daemon's HTTP
and WebSocket API, so the shell could be swapped for Electron without touching it.

```bash
# 1. The daemon: against the fake repos (fixtures/, created on first run)…
scripts/dev-daemon.sh
#    …or against your real projects
cargo run -p delilah-daemon

# 2. The app (first time: npm install)
cd app && npm run tauri dev

# Delilah.app and a .dmg in app/src-tauri/target/release/bundle/ (builds delilahd too)
cd app && npm run dist
```

`npm run dev` alone serves the same UI at http://localhost:1420 for a browser.

**The interface (UI v2).** Dark and light follow the Mac, and the terminal and the editor
change theme with it. The window has the sidebar, the main area and, at the bottom, a
statusbar: "Connected" (or "No connection to delilahd · retrying…"), "N working · N waiting for you · N paused · N failed", the Claude usage and "Logs" (opens the log view; Help ›
View logs and ⌘⇧L do too). With the daemon away the app keeps the last data under a banner
with its age ("Data from 2 min ago"), "Retry now" and the command to start it; it
reconnects by itself. The sidebar folds to a rail with ⌘\ (and by itself below 1300 px of
window); it lists each project's most urgent tasks with "Show N more", a task filter, and
"Delete task…" in a task's menu. The first time (no project) "Getting started" walks
through adding a project, connecting Jira (optional) and creating the first task. The same
words mean the same thing everywhere: a task is Backlog, Working, Waiting for you, Paused,
Done or Error; an Inbox item is a Question, a Permission or a Notice; a PR is Draft,
Open, Checks failing or Merged. Amber is only for "Waiting for you". Starting is "Start",
stopping is "Stop", going somewhere is "Open…". Narrow windows keep every function: the
task header compacts below 960 px of main area and the panels become overlays below 1200 px.
Over the app, short notices warn when a task waits for you ("Answer"), when Claude's usage is
almost out, when a limit is reached ("resumes at…") or when an autopilot pauses.

**Previews.** To try a branch while the installed Delilah keeps working (e.g. when
Delilah's own agents build Delilah), `scripts/preview.sh` runs that checkout's
daemon on 750N and its UI on `http://localhost:150N`, with state in
`fixtures/preview-home-N`; several worktrees get different slots. The daemon lets
that UI in with `DELILAH_UI_ORIGIN`, and the UI finds it through
`VITE_DELILAH_URL`. `--fresh` starts empty, `--no-ui` runs only the daemon,
`--notify` turns notifications on. Ctrl-C stops both.

**End-to-end tests.** `scripts/e2e.sh` runs the API/CLI and UI flows against a fake Claude, safe next to the installed app (see [development.md](development.md)).

The bundle carries `delilahd` inside (`Delilah.app/Contents/MacOS/delilahd`). When
the installed app opens, it installs a LaunchAgent
(`~/Library/LaunchAgents/dev.delilah.daemon.plist`) and launchd keeps the daemon
running from then on: at login, after a crash, and with the window closed, so
agents keep working. Opening a newer app restarts the daemon with the new binary.
The Delilah menu has "Restart the daemon" and "Stop the daemon"; the log is
`~/Library/Logs/Delilah/delilahd.log`. Development builds (`tauri dev`) leave
launchd alone and expect `scripts/dev-daemon.sh`.

**Board and specs.** The Tasks view is a board with one column per SpecKit phase:
Specify, Plan, Implement, Review and Done. The daemon reads the
task's spec folder in its workspace every few seconds and works out the phase from
the files: no `plan.md` yet means Specify, `plan.md` means Plan, unchecked
items in `tasks.md` mean Implement, all of them checked means Review. The
folder is `specs/<folder>/` in the workspace (or in one of its submodules), the one
whose name has the ticket key or the branch name, or failing that the one the
branch has been writing to (a `specs/` kept in `.gitignore` counts too). Inside a task you see the phases, the spec's documents
and the `tasks.md` checklist with the item the agent is on. Phase changes go to
the task's history. Delilah only reads these files; the repo stays the source of
truth.

The task screen's header shows the task's phase (step number and name) next to its status, with the progress ("Implement · 4/12") while implementing.

Without SpecKit the phase comes from what the agent does: its first edit to code
(outside `specs/` and `.specify/`) means Implement, and `gh pr create` or
`git push` means Review. When the agent stops with commits on its branch, the
Inbox asks "Move to review?". These signals and the spec files only move a
task forward; you move it anywhere by clicking a phase in the task's phase bar,
and asking for a review ("Review changes") moves it to Review.

**Saving mode.** Every turn of a Claude session re-reads the whole conversation, so
long tasks get expensive. With "Saving mode" on (a checkbox when creating a task or
saving it to the backlog, a toggle on a running task, `--saving-mode` /
`--no-saving-mode` in `delilah run` and `new`), each phase runs in a fresh session.
The first one ("Spec and plan") stops once `tasks.md` is written; the Inbox and the
task screen then offer "Implement in a new session" (which also approves the
tasks) or "Keep going here" to carry on in the same session without saving mode. The
implementation session ("Implement") starts from the spec files; when every
item is checked, a "Review" session starts on its own, reviews the branch and
commits (it never pushes). The task screen shows the chain of sessions, with the
earlier ones finished and reopenable; the newest is the main one. Before each
change the outgoing session saves its notes with `/save-session`. If a handoff
fails, the Inbox shows it with "Retry"; after a restart Delilah finishes or
re-offers it. In Settings: "Saving mode by default" (new tasks) and "Save notes
before switching session" (on by default). Needs the spec files: without a
`specs/` folder there are no handoffs.

**Autopilot.** A task can run on its own up to the PR: "Autopilot" (next to "Saving
mode", which it turns on; a toggle on a running task, `--autopilot` /
`--no-autopilot` in `delilah run` and `new`, `delilah autopilot <KEY> on|off|resume`).
Only for tasks on the spec workflow. The implementation starts without asking, the
review runs the project's checks, and when it reports the branch ready Delilah starts
the project's preview (if it has one) and opens a draft PR with your `gh`. A step that
stalls gets one nudge; questions that have an option marked "(Recommended)" are
answered for you. Anything else (a permission prompt in the task's session, a question without a
recommendation, a blocked review, a failure) pauses it: the header shows "Autopilot
paused" with the reason, and the Inbox offers "Resume autopilot" or "Carry on by
hand". Autopilot waits while the agent's background subagents run, and only nudges or
pauses when the agent has really stopped. In the terminal too, a task whose agent ended its turn while its sub-agents still work stays "Working", not "Waiting", until they report back, and no "is waiting for you" or "finished its turn" notification is sent meanwhile. On the idea card, the autopilot option creates the task and starts it.
Delilah pushes and opens the PR without asking again, so turn it on only where that is
what you want. It never merges.

**Task previews.** A project can have a preview command (field in the project view;
for Delilah itself, `scripts/preview.sh`). "Start preview" in a task's header runs it in the
task's workspace, shows the URL it prints and "Stop" stops it; it also stops when the
task ends. A restart of the daemon does not bring a preview back.

**Chat view and chat mode (experimental).** Every Claude session of a task has a Chat /
Terminal switch above its tabs; the chat shows your messages, Claude's replies, tool
cards (commands, diffs, file reads; click one for its full input and result), question
cards and a text field, and queues what you write while Claude works ("Stop"
interrupts). The switch is the only view control: what you pick is remembered as the
task's view (the new task dialog's "View" sets the first one). Each session has its own
switch, so one Claude of a task can be in chat mode and another in the terminal
(`delilah runtime --session <ID> chat|terminal`).

Chat mode goes further: with the chat-mode option in the new task dialog or
`--chat` on `delilah new` / `delilah run`, the task's Claude runs without a terminal
(`claude -p` with a stream), so its text appears as it is written. Hooks still drive
the task's state, the Inbox, usage-limit pauses, saving mode, autopilot and the PR
flow. There is no terminal in that mode (`delilah attach` says so). "Terminal" in the
switch (or `delilah runtime <KEY> terminal`) restarts the agent in a terminal with
`--resume`, keeping the conversation; it is refused, with the reason next to the switch,
while the agent works or waits for you (wait, answer, or press "Stop"). A terminal
task goes the other way with the switch (or `delilah runtime <KEY> chat`). Project sessions and
idea conversations always use the terminal.

While Claude works the chat shows what it is doing ("Working" with a clock and the current
step), sub-agents appear as cards with their latest steps (one line each, not their whole
conversation), and background commands and sub-agents are listed in an "In the
background" strip with "Stop". The task stays "Working" until that work ends; if Claude
resumes on its own afterwards the chat says "Claude continues: background task finished".
While background work runs you can type to Claude, but the Chat / Terminal switch answers
"Background work is running: wait for it to finish or press Stop".

When a chat-mode agent needs a permission it shows a permission card (also in the
Inbox) with the tool and what it will do: "Allow once", "Deny" (with an
optional reason that Claude is told) or "Always allow in this task". The last one is
remembered per task, for all its sessions (autopilot included), and only for bounded
rules: a command from a short list of safe ones (`git status`, `ls`, `cargo test`…) with
plain arguments inside the workspace (no quoting, chaining, `-exec`-style flags or paths
outside it), otherwise that exact command; the exact file for an edit; the exact host for
a web fetch. When no safe rule exists the card offers only "Allow once". Anything
else asks again.

**Playground.** A view (in the sidebar) that draws your live Claude sessions as a
building: one floor per task or project, newest on top, each with a miniature. A
floor is an office with a pixel character at a desk per session, in fixed slots, so
a character never jumps when another leaves. What the character does says what its
session does:

- *arriving*: the session is starting; *thinking*: Claude is working with no tool in flight;
- *typing* (editing or writing), *reading* (reading or searching), *terminal* (running a command), *busy* (any other tool), with a caption under the desk such as `Edit src/lib.rs`, and the monitor glows in a colour per kind of tool;
- *phone*: "Supervisando", Claude is waiting on a sub-agent it launched in the foreground (one in the background leaves it thinking);
- *waiting*: it needs you (a `?` bubble, highlighted); *paused*: the usage limit stopped it (`zzz`).

The caption is the tool plus its target, cut to 40 characters. File contents never
show and a `WebFetch` shows the host only. When Claude launches sub-agents,
**helpers** appear next to the desk, each with a role (researcher, implementer,
reviewer or generic), and walk to the lounge when they finish. A **stack of paper**
on each desk grows with the tokens the session has spent (new input and output; cache
reads and sub-agents' own spend are not counted).

Click a floor to open the task's **Oficina** tab (right after the overview, with a dot
when an agent of that task waits). It shows the floor and, with the toggle on its
right, **Flujo**: a graph of the agents, their helpers and every tool call, with
failed calls marked, and a timeline beside it. Drag and scroll to pan and zoom, or use
the +/-/fit buttons; click a node or a timeline row to find the other. The toggle
remembers your choice. The header counts the agents waiting for you ("N esperando");
**clicking a character opens that task on the session's terminal**, where you answer.
"Abrir en ventana" moves the playground to its own window to keep it on a side
screen: closing that window really closes it, a floor opens inside it with a back
button, and a click on a character brings Delilah to the front with the task. In a
plain browser it opens a named tab instead.

With reduced motion in the system the characters stay in still poses, animations
pause while the window is hidden, and while the daemon is unreachable a
"Reconectando…" banner shows the last known state. Actions come from Claude's
hooks, so **an agent started before this update shows no actions until it is
relaunched** (it still shows thinking, waiting and paused).

**Inbox.** Everything that waits on you, from every task, most urgent first:
what an agent asks at the end of its turn (its last message, with a box to answer
it right there), a permission it needs (answered in its terminal, or with the permission card for a task in chat mode), a failed turn,
tasks ready for review, paused by the usage limit (with "Resume now") or failed.
Each item opens the task on the right tab, and questions and permissions can be
answered inline. The sidebar counts what blocks an
agent; ⌘I opens it. Notices (what does not block an agent: "Finished its turn" of a
secondary session, a PR opened, review ready, paused, failed, branch behind) go away one by one
with "Remove" or all at once with "Clear notices" (a toast offers "Undo"); what waits on
an answer stays. With nothing waiting it says "Nothing is waiting for you" and the last activity. The daemon keeps what each session waits for from its hooks,
so it survives a restart (`GET /api/v1/inbox`, `POST /api/v1/sessions/{id}/reply`).

**Toasts.** Tasks that wait for you show one grouped toast, not one per task: "{key} is waiting
for you" with "Answer" for one, "{n} tasks are waiting for you" with "View" (opens the Inbox) for
several. A task leaves the group when it stops waiting or you open its screen; closing the toast
empties it. Identical repeated toasts merge into one with "×N" (up to "×99+"). With more than one
toast they collapse into stacked cards (the newest in full plus a "+N" button); hover, focus or
click expands the stack and the button becomes "Hide notices". "Close all" shows whenever there
is more than one toast, collapsed or expanded. Timers pause while it is expanded. Info toasts
last 6 s; errors and the waiting toast stay until closed.

**Pull requests.** Delilah uses your own `gh` (signed in as you; it stores no
token) and checks every 30 s. When the task's branch has a PR, it links it, per
submodule for a parent, and moves the task to Review. You can also paste a PR's URL
("Link by URL") or "Unlink" one. Pointing `DELILAH_GH_BIN` at another
binary swaps `gh` (previews use a fake one).

- **The "PR" tab** (next to Changes) shows the title, number and link, draft, open,
  merged or closed, each CI check, the review state and the comment count, the
  account `gh` uses and when it was last checked. "Update" checks at once. If `gh`
  is missing, signed out or has no access to the repo, the tab says so.
- **The PR badge** on the board cards and the sidebar shows the same state at a glance;
  for a parent with several PRs it shows the worst one.
- **Inbox items:** CI failed, changes requested and new comments on the PR, from
  anyone but you. Each opens the PR, or "Ask the agent to fix it" sends it to
  the task's agent as comments to consider (never on its own), or "Discard" it.
  They count on the Dock badge. A new failing commit raises a new item; the same one
  never twice.
- **"Open PR"** (in the PR tab, once the branch has commits, and while some repo of the task has no open PR) shows a title and
  description to edit (the Jira link and the commit subjects), "Draft", and for a
  parent a "Submodule" picker, on the first free submodule with commits. It pushes the branch and opens the PR against the
  default branch; if anything fails, nothing is linked and the error is shown.
- **"Merge"** (PR tab) merges an open, non-draft PR whose checks passed, with the
  method the repository allows (the tab shows the method and what will happen) and only
  while `gh` is still the account the tab showed. Each check shows its time and the review
  comments their file and line. The task is then archived as below; Delilah never merges
  on its own.
- **"Update branch"** appears under the task header as "main moved N commits ahead" when the
  base moved on: it merges the base into the branch (never a rebase), refuses with a
  reason if the workspace has tracked changes or the merge conflicts (then the branch is
  left as it was), and clears the "Branch may be behind" notice.
- **Archive on merge.** When all of a task's PRs are merged, Delilah marks it done and
  archives it without asking: back to your previous branches in checkout mode, the
  worktree removed otherwise, and the local task branches deleted (never the parent's
  and never on GitHub). Every repo counts, also one without a PR: if any has uncommitted changes or
  commits the PR does not contain (or, without a PR, unpushed ones), or cannot be
  fetched, nothing is removed and the Inbox says why, with "Archive anyway". A PR
  closed without merging only notifies. You get one notification per merge, "merged and archived" or "merged, not archived". If branches are left, the Inbox lists them (also once the task is
  archived) and archiving by hand finishes it.

**Questions with options.** When the agent asks with Claude Code's
`AskUserQuestion` (as the `feature` workflow does), Delilah shows the question and
its options in the Inbox and on the session's tab: one click answers, as if
chosen in the terminal, and the agent goes on. Several questions or multiple choice
send with "Answer"; you can also type your own answer, or "Answer in the
terminal" to answer there. It works through a `PreToolUse` hook in the session's
settings that waits (up to a day) for the answer. If the daemon is away, the
question simply shows in the terminal.

**macOS.** The Dock icon counts what blocks an agent. While you are in another
app, each new item of the Inbox is a notification with the agent's own words
(and a task resuming after the usage limit is one too). Coming back to Delilah
within 10 minutes (e.g. from the Dock icon) opens that item on its tab. Clicking
the notification itself does not bring Delilah forward: Tauri's notifications
report no clicks on desktop, and in development builds they are even sent as
Terminal. While the app is open the daemon stays quiet; when it is closed, the
daemon notifies as before.

**Projects.** The sidebar groups everything by project, as in Orca: the project,
its sessions, then its tasks by urgency. A project's screen works like a task's,
but on your own checkout: "Ask Claude" (or "+" on the project) starts
Claude in **read-only** mode (`--permission-mode plan`). It reads, searches and
explains, but does not edit, and nothing is created. "Permitir cambios en esta
session" restarts it in normal mode on the same conversation. "Turn into a task" turns
that Claude into a free task's primary session, keeping its conversation: the task
works in your checkout, the folder the conversation belongs to. A project session can
also propose tasks (see "New task with Claude" below).

**Project settings.** In Projects, "Settings" on a project opens its settings.
They are personal to your Delilah (stored by the daemon, nothing is written in the
repo) and the same ones can be edited with `delilah project show|set|unset`.
Whatever is not set keeps its default, which `show` marks.

- **Where it works** (workspace mode): checkout or worktree, for new tasks. Unset by
  default, and then tasks run in your checkout. A task started
  without saying where follows it (CLI, ideas, the autopilot idea option); the
  new task form preselects it, and a
  backlog task keeps the choice you saved on it. `delilah project show` prints
  `checkout (default)` when unset. Before this redesign an unset setting meant a
  worktree for everything but the form: scripts that relied on it now get the checkout
  (with its clean-folder rule) unless they pass `--worktree`.
  `--checkout` / `--worktree` on `delilah new` and `delilah run` override it.
- **Branch template** (branch, optional): empty by default, and then Delilah proposes a
  descriptive name for each task (`feature/PROJ-12-retry-payments`). With a template,
  e.g. `fix/{ticket}-{slug}`, the template decides the format: `{ticket}` is the key
  and `{slug}` is Claude's short proposal (the title made safe for a branch name if
  Claude fails: lower case, no accents, hyphens, at most 40 characters). If the slug
  comes out empty the `/`, `-`, `_` and `.` around it are dropped. The "Branch" field of
  the new task form shows the branch the task will get and you can edit it; what you
  type wins over the template. `delilah project unset <name> branch` goes back to the
  proposal.
- **Shared files**: files or folders of the checkout that a **worktree**
  needs but git does not carry (`.env`, `node_modules`). Each is copied or linked
  (a symlink to the original); "Automatic" links folders and copies files. What
  already exists in the worktree is left alone, and a missing source or an unsafe
  path (outside the project, or inside `.git`) is skipped and noted in the task's
  `workspace.log` without stopping the task.
- **Setup command**: a command (e.g. `npm ci`) that runs once in a
  new worktree, after the shared entries and before the agent starts. It runs in
  your login shell with a 10 minute limit, as a read-only "Setup" tab of the task
  with its full output (also in `tasks/<KEY>/`). If it succeeds the agent starts.
  If it fails, times out or is cancelled, the task stops as `setup-failed` with the
  end of the output, and offers "Retry setup", "Start without setup" and "Cancel setup" (a
  running setup can be cancelled too; the whole process group is stopped). The same
  from the terminal: `delilah setup retry|skip|cancel <KEY>`. If the daemon restarts
  during a setup, the task shows it as interrupted and can be retried. A failed
  setup task can be archived. The failure banner shows the command ("Copy command"), the
  exit code or "Timed out" / "Cancelled", the duration, the end of the
  output, "Open the full log" and "Open a terminal here". Checkout tasks do not run the setup.
- **Always / never included submodules** (parents): "always" ones are added to
  every run; "never" ones are not offered in the new task form's picker. A rule
  that names a submodule that no longer exists is rejected with an error pointing
  at the settings.

Errors in the form appear next to the field (and as `field: message` in the CLI).

**New tasks.** From a Jira ticket or **free** (no ticket: Delilah names it `LIBRE-n`
and starts `/feature <what you want to do>`). The agent works either **in your
checkout** (default: a branch in your folder, or in the chosen submodules of a
parent; one task at a time, with the checkout clean; archiving goes back to the
branch you were on and keeps the task's branch; files left uncommitted stay in your
folder and never block it) or **isolated in a worktree**
(several tasks in parallel without touching your folder).
The task's branch starts from `origin`'s default branch as Delilah has just fetched it
(for a parent, each chosen submodule from its own), not from what your folder had
checked out; your local `main` is not touched. A name that is already taken gets `-2`
(see below), so the task never lands on an old branch. A free task that starts without
a branch (see below) starts its worktree there too, so the branch it gets later starts
fresh; in your checkout it starts from where your folder is. If the fetch fails (offline, no access) the task starts anyway from the
last known state and warns "Branch may be behind": an item in the Inbox and a
banner on the task screen, with "Discard". It never blocks the task, and archiving
clears it.

**Finished tasks.** "Mark as done" offers "Undo" for a moment; later, "Reopen"
(or `delilah reopen`) brings a done task back while its workspace still exists. The board's
"Done" cards show the PR chip (#n and its state) and the finish date. Jira tickets show
their sub-tasks (with the count done) under the parent.

**The task's branch.** Delilah names it from the title: Claude proposes a short name
and the branch is `feature/<short-name>`, or `feature/<KEY>-<short-name>` for a Jira
ticket (a free task's `LIBRE-n` is left out). If Claude does not answer in time, the
name comes from the title, and failing that from the key. The start dialog has a
"Branch" field that fills in the proposal about a second after you stop typing the
title; type your own name there and it is kept, or empty it to let Delilah choose.
A name already used (as a local branch or on a remote, in any repo of the task) gets
`-2`, `-3`…: a branch is never reused. `delilah run --branch <name>` is cleaned up
the same way (`feature/` added once, spaces and accents turned into `-`/letters). The
branch shows on the task's screen and stays the same for the whole task. The one
exception is a free task with no branch typed in a project without a template: see
the next paragraph.

**Branch and name from the analysis.** A free task started with no branch typed and in a
project without a branch template starts without a branch of its own
(in a worktree, detached at your project's HEAD; in your checkout, nothing is
switched). Once the agent writes the spec, the task takes the spec's title (the task
screen shows "Before: ..." with "Restore", and you can edit the title in place at any
time) and a branch named like the spec folder (`005-my-feature`), also in the chosen
submodules of a parent. If the agent edits code or commits before that, Delilah makes
the branch first. If it cannot, the Inbox shows it with "Retry". Without a
branch, "Open PR" is refused. A typed branch, a project's template, and Jira tasks follow the paragraph above
(and keep the ticket's title).

**New task with Claude.** Third option in the "New task" dialog, next to Jira
and Free (after choosing the project), and on the project's screen and its "+".
Instead of a form you tell Claude your idea in a conversation view, without a
terminal: its replies, its questions as cards (like the Inbox's) and a text field.
You can write while it works (the message waits for the end of its turn) or press
"Stop". Claude reads the code (read-only, it cannot edit) and ends with a
`New task` card with a title and a brief (the idea, a summary, affected areas and
open questions). "Create" puts the task in the project's **backlog** with that brief as
its notes, and its history says "Created from the conversation «name»"; you review or
edit it and start it as any other. The conversation stays open and keeps its history,
also after restarting Delilah. Project sessions ("Ask Claude") can create
tasks the same way; a task's own sessions cannot. In the sidebar a project's ideas
appear as one "Ideas" row (with their count, highlighted when one is waiting for you)
that opens the waiting idea, else the last one shown, else the newest; on the project's
screen each idea is a tab.

**A task's screen.** As in Orca, a task opens with tabs in the middle and the
project tree on the right (⌘B shows or hides it). The tabs are the task itself
(status, actions, spec, history), its sessions, Changes when opened, and files.
Each task remembers its tabs when you switch tasks or reload the app. ⌘P finds a
file by part of its name (the workspace's files as git sees them, submodules
included) and opens it in a tab.

- **Sessions** are tabs with their live terminal: the primary Claude, extra Claudes
  and shells. "+ Claude" and "+ Terminal" in the task, or ⌘⇧T and ⌘T, start one
  as a new tab. ⌘W closes the active tab: a terminal closes at once, a working
  Claude asks first, and the primary one never closes (it ends with the task). A
  stopped session offers "Reopen". ⌃Tab and ⌃⇧Tab move between tabs. In saving
  mode each phase's session is named after its phase ("Spec y plan",
  "Implementación", "Revisión"), also once it is no longer the current one; outside
  saving mode the primary reads "Claude principal".
- **Changes** lists what the branch changed in each repo of the workspace (each
  submodule of a parent), with line counts and the diff of each file. It compares
  against where the branch started (its merge-base with `origin/HEAD`, or `main`), so
  commits, uncommitted edits and new files all count. Arrow keys or j/k move through
  the files. When every item of `tasks.md` is checked, the task offers to review the
  changes or start a "Review" session with Claude. ⌘S saves a file, ⌘F searches.
- **The tree** marks the branch's changes. **Files** open in CodeMirror 6, with the
  language picked from the file name. The agent works on the same files, so an open
  file reloads by itself when it changes on disk. If you had unsaved edits, the
  editor asks which version to keep rather than overwrite either one. Files above
  1 MB and binaries are not editable. Paths are confined to the workspace, symlinks
  included.
- **Markdown** files (`.md`, `.markdown`) switch between the source and a rendered
  preview with the button over the editor or ⌘⇧V; the preview shows unsaved edits and
  each tab keeps its mode. Specs, Inbox messages and Jira descriptions render the
  same way (GitHub-flavoured: tables, task lists, quotes, links, `https:` images),
  keeping their single line breaks. HTML in the text shows as text. Web links open in
  the browser. In a workspace file, relative links open the file in a tab and relative
  images load from the workspace; in tickets and Inbox messages a remote image waits
  for a click, so viewing them tells no third party.

**Live updates.** The app does not poll. The daemon announces what changed on
`GET /api/v1/events` (a WebSocket sending topics like `{"type":"inbox"}`) and the
app fetches only that again, so a question reaches the Inbox in well under a
second. If the connection drops (the daemon restarts or updates), the app
reconnects by itself and fetches everything again. A full refresh every 30 s
stays as a safety net.

**Agent permissions.** Settings › Agent permissions chooses how Claude decides what needs
your approval, for task and project sessions in both the terminal and chat mode: "Auto"
(the default) lets Claude's auto mode handle routine work, so it stops asking for every
command; "Claude's default" launches Claude with no mode flag. It applies to sessions you
start or reopen from then on; running ones are not restarted. Idea conversations and
read-only sessions keep their own modes. If Claude refuses the mode at start, the error
says to try "Claude's default" (checked with Claude's "invalid mode" error; the wording for
an account without auto mode has not been seen yet). Actions auto mode still escalates show
as permission cards in chat mode.

**Language.** Spanish or English, picked in Settings › Language (Automatic follows the Mac: Spanish if the Mac is in Spanish, English for any other language). It
covers the interface, the Mac menu (from the next time the app opens), the
notifications, and what Delilah writes to Claude (the message after a usage
limit, the review prompt, the ticket handed over), so the agent answers in that
language too. Texts are written in Spanish in the components, wrapped in `t()`;
the English ones are in `app/src/i18n/en/<Component>.ts` (Automatic gives English unless the Mac is in Spanish). In development the console warns of
any text without a translation.

**Help.** Help › View logs (⌘⇧L) shows the daemon's last lines live, to see what is
going on while testing.

## Jira

Connect it from the app's Jira view: Jira Cloud (email + API token),
Server/Data Center (personal access token) or built-in sample data. The token is
kept in the macOS Keychain. "Analyse with Claude" opens a new task prefilled from
the ticket; for parent projects, "Suggest with Claude" runs a quick read-only
Claude pass (haiku) that picks the submodules and says why. The agent gets the
ticket as `~/.delilah/tasks/<KEY>/ticket.md`.

## Sessions

A task's workspace can hold several sessions: its primary Claude
(whose turns drive the task's status), extra Claude sessions (e.g. "Review
changes", to review the branch) and plain terminals with your login shell. A usage
limit hit by any of them pauses the task; after the reset the primary and the
paused ones carry on. A session that stopped (the daemon restarted, the process
ended) can be reopened: Claude picks its conversation back up with `--resume`.

## Resource usage

Delilah adds little on top of the agents themselves: a small Rust daemon and a Tauri
window that uses the system WebKit, with no bundled Chromium. Measured on 2026-10-23 on
an M4 Pro (12 cores, 24 GB, macOS 26.3.1) with Claude Code 2.1.292. RSS and CPU come
from `ps`, sampled every 5 s and summed over each process tree (100% CPU = one core).
The script is [scripts/sample-resources.py](../scripts/sample-resources.py).

**Delilah with 12 agents running** (2 min, app window open, live updates flowing):

| Process | Processes | RSS avg | RSS max | CPU avg | CPU p95 |
|---|---:|---:|---:|---:|---:|
| `delilahd` (daemon) | 1 | 18 MB | 23 MB | 1.1% | 1.8% |
| `delilah-app` + its WebKit processes | 4 | 177 MB | 278 MB | 8.0% | 16.2% |
| **Delilah's own overhead** | 5 | **~195 MB** | | **~9%** | |
| The 12 agents (Claude Code + MCP servers, builds, tools) | 111 | 2.55 GB | 3.2 GB | 54% | 104% |

The agents account for over 90% of the memory, about 210 MB each. That is the same
`claude` binary every client runs, so what changes from one client to another is the
shell around it.

**Other clients' shells**, each opened on this repo and sampled for 60 s:

| Client | Engine | RSS avg | CPU avg | App size | Measured |
|---|---|---:|---:|---:|---|
| **Delilah** (daemon + app) | Rust + system WebKit | **~195 MB** | **~9%** | **26 MB** | with 12 agents running |
| Zed | Rust, native GPU UI | 180 MB | 1.6% | 175 MB | idle, no agent session |
| Orca 1.4 | Electron | ~600 MB | 1.9% | 642 MB | idle (a restored `claude` left out) |
| Claude desktop | Electron | 829 MB | 35% | 615 MB | with 4 Claude Code sessions |
| VS Code | Electron | 914 MB | 12% | 1.4 GB | idle, no Claude extension |
| Cursor | Electron | 1.29 GB | 64% | 1.0 GB | idle; indexing just after opening |

Delilah was measured under load and the others mostly idle, so this is unfavourable
to Delilah. Only Zed is in the same range, and Zed is an editor, not an orchestrator. In
Zed and VS Code an agent session adds its adapter or extension on top. Orca keeps a
background daemon (~22 MB) after you quit it, as `delilahd` does.

**Disk.** Most of the disk goes to the workspaces, not to Delilah: each task is a full
worktree with its own build output. With six Rust workspaces `~/.delilah` took 32 GB,
almost all of it `target/` dirs (4.7 to 17 GB each). Task files take 147 MB and the
database under 5 MB. Any client that isolates tasks in worktrees pays this cost;
archiving a finished task (`delilah archive`) removes its workspace and frees it.

## Building from source

Delilah is not signed, so the simplest way is to build it on the Mac that will use
it. Needs Xcode Command Line Tools (`xcode-select --install`), Rust (rustup), Node
22+, git and Claude Code logged in.

```bash
git clone https://github.com/FranPerezFolgado/delilah.git && cd delilah/app
npm install
npm run dist   # tauri build, with a .dmg that needs no Finder automation
# Install it
cp -R src-tauri/target/release/bundle/macos/Delilah.app /Applications/
open /Applications/Delilah.app
```

The `.dmg` in `src-tauri/target/release/bundle/dmg/` also works on another Apple
Silicon Mac. Copied over (AirDrop, a download), macOS will say the app is damaged
or from an unidentified developer, because it is not signed or notarized. Once it
is in /Applications, clear the quarantine flag:

```bash
xattr -dr com.apple.quarantine /Applications/Delilah.app
```

State lives in `~/.delilah` (database, workspaces, task files) and the Jira token
in the Keychain. To remove it, see [Uninstall](#uninstall).

## Uninstall

1. Stop the daemon from the app menu (Delilah → Stop the daemon), then quit the app (⌘Q).
2. Delete the app: `/Applications/Delilah.app`.
3. Delete the daemon's LaunchAgent: `~/Library/LaunchAgents/dev.delilah.daemon.plist`.
4. Optional: delete `~/.delilah` (database, workspaces, task files, backups). This
   discards your tasks and their workspaces; skip it if you may reinstall later.
   The Jira token lives in the Keychain and can be removed there.

## Updates

Delilah updates itself from the versions published on GitHub. A notice says a new
version is available and how many open sessions (terminals included) a restart would
interrupt. It never installs without your confirmation: "Update now" installs right
away, "Later" hides that version until the next manual check or the next launch, and
"When they finish" waits until no session is left open. It checks 10 s after launch
and every 6 h; you can also check from the menu (Check for updates…) and from
Settings › Updates ("Check now"). Development builds (`tauri dev`, previews) do not
update. Before migrating after a version change, a copy of the database is saved in
`~/.delilah/backups/`. How to publish a version: [development.md](development.md#releasing).
