# Vision: where Delilah is going

This page is the public summary of the original design sketch
of 2026-10-19, as far as it still applies. It covers what was planned
but **is not built yet**. Where the sketch and the code disagree, the code and
[decisions.md](decisions.md) win: hooks go in `--settings` rather than
`.claude/settings.local.json`, and questions go through AskUserQuestion rather
than an `ask_user` MCP tool.

## The goal

A task moves on its own from a Jira ticket to pull requests. It stops only when
the agent asks something or runs out of quota, and in both cases the user gets a
notification. On a parent repo with submodules, a task is a **changeset**: one
workspace with only the submodules it touches, the same branch in each, and one
PR per submodule, linked to each other and to the ticket.

## Planned, not built

**More agents.** One `AgentAdapter` trait, with the methods `launch`, `send`,
`resume`, `install_hooks`, `sync_context` and `read_usage`, and implementations
for Claude Code and other coding agents.
- **Connecting:** a structured protocol where the agent offers one (ACP, Claude's `stream-json`, other agents' JSON streams), with the PTY as the universal fallback.
- **Agents without hooks:** their state would be read from OSC 7501 terminal sequences, as Orca does.
- **Compatibility:** keep a matrix of agent versions, because these CLIs change every month.

**Portable context.** One source of skills, instructions, MCP servers and commands,
translated into each agent's format when the workspace is prepared:
- `SKILL.md` is shared across agents;
- `CLAUDE.md` maps to `AGENTS.md`;
- MCP config is written per agent.

The idea is to turn today's Claude subagents (reviewers of quality, security and
tests) into workflow steps, so any agent can do them.

**Declarative workflows.** `[workflow.<name>]` with phases, each phase saying:
- which agent runs it, and which command;
- the artifact it produces;
- whether it needs a person;
- fan-out (`tasks.md` items marked `[P]` run in parallel, e.g. `parallel = 3`).

Candidate workflows:
- SpecKit feature (the `/feature` wrapper);
- bugfix (analyse → reproduce → fix → test);
- PR review;
- analysis only.

Phase detection would be by artifacts by default, or reported by the skill
(`report_phase`). Today only the `feature` workflow exists, and its phases are
tracked (see `phases.rs`), not driven.

**Per-project settings (mostly built).** Not a `delilah.toml`: they are stored by
the daemon and edited from the app and the CLI (see decision 50 and
`specs/004-project-config`). Built: the workspace mode, the branch template, shared
folders, a setup command, and submodules to always or never include. Still pending:
- PR options (link the PRs of a changeset to each other and to Jira);
- sharing settings through the repo, as a base the stored ones override.

**Closing the loop:**
- Delilah opens the PRs, or links the ones the agent opened.
- It notifies on CI failures and on new PR comments.
- When every PR of a changeset is merged, it archives the workspace, deletes the local branches and saves what was learned to QMD.

**Gates.** Configurable stops per workflow (e.g. between "plan ready" and "build
workspace"). The state machine was meant to leave room for them.

**Concurrency and budget.** A maximum number of agents at once (e.g. 3), a budget
per task in tokens or time with a warning at 80 %, and optionally resuming with
another agent or account when a task is urgent.

**Mobile, through Tailscale.** The daemon is already API-first, so a phone is just
one more client:
- **Exposure:** `tailscale serve` for HTTPS, never the public internet.
- **Pairing:** with a token, which today's loopback-only security model does not have yet.
- **Client:** a PWA served by the daemon, with web push.
- **Scope:** see, answer and approve. Destructive actions stay on the desktop.

Proposed but not decided:
- an Inbox-first mobile scope;
- QR pairing.

The user still has to say:
- which Mac it would run on, and whether IT allows Tailscale;
- iPhone or Android;
- answer-only, or also launching work.

## Open questions from the sketch

- Linux and Windows too, or macOS only? (This decides notifications and installers.)
- Which other workflows come after SpecKit?
- Is it worth integrating the company's own autonomous agent as one more agent?
- Does RTK work with other agents, or only with Claude?
