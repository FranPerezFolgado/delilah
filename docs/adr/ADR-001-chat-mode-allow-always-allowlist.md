# ADR-001: "Permitir siempre" in chat mode remembers only allowlisted or exact rules

- **Status**: Accepted
- **Date**: 2026-10-25

## Context

In chat mode Delilah runs `claude -p --input-format stream-json --permission-prompt-tool stdio`
and answers every `can_use_tool` request with a "¿Permitir?" card. "Permitir siempre en esta
tarea" stores a rule in `tasks.allowed_tools`; that rule goes into `permissions.allow` of every
later session of the task (autopilot included) and the daemon answers matching requests by
itself. The rule shape comes from Claude's `permission_suggestions`, which the model writes and
repo content can steer. Two security reviews showed that a bare `Bash` rule, broad prefixes
(`git:*` → `git -c alias.x=!sh`, `npm run:*`, `find:*` → `-exec`, `rm:*`, `curl:*`) and file
globs (`Edit(~/**)`, `Read(~/.ssh/**)`) turn one click into unattended code execution or data
exfiltration for the rest of the task.

## Decision

A remembered Bash prefix rule is accepted only for an allowlist of bounded commands
(`SAFE_BASH_PREFIXES`); anything else is remembered as the exact command, file rules as the
exact approved file inside the workspace, WebFetch rules as the exact host, and when no safe
rule exists only "Permitir una vez" is offered.

A prefix rule is applied by the daemon only, never written into the session settings: Claude's
own matcher would accept any arguments after the prefix. The daemon matches it when the
arguments after the prefix pass `claude::is_safe_prefix_tail`: no quoting, globbing, chaining
or redirection, no flag that runs a program (`-exec`, `-toolexec`, `-vettool`, `--config`,
`--script-shell`, `--ext-diff`, `--textconv`, `-Z…`, `+toolchain`), and no path leaving the
workspace (leading `/` or `~`, `..`). Exact, file and WebFetch rules still go into the
settings.

## Alternatives considered

**Keep Claude's suggestion as is (bare tool name as fallback)** — *Rejected*
One click on a harmless `npm --version` granted the whole shell to every later session of the
task, including autopilot runs nobody watches.

**Denylist of dangerous command heads** — *Rejected*
It was the first fix (interpreters and wrappers: `sh`, `env`, `sudo`, `node -e`…), but the
re-review found that ordinary tools (`git`, `npm`, `find`, `make`, `cargo`, `ssh`, `tar`)
execute or write arbitrary things through their arguments; a denylist can't be complete.

**Exact command only, never prefixes** — *Rejected*
Safest, but `cargo test`, `npm test` or `git diff` with varying arguments would ask on every
run, which defeats "always" for the most common, harmless requests.

**Allowlist of bounded prefixes, exact rule otherwise** — *Accepted*

## Consequences

✅ A remembered rule can't grant a shell, an interpreter or a write outside the workspace:
the same check (`claude::is_safe_allow_rule`) filters what is stored, what is written into the
session settings, and what the daemon auto-answers (`permissions::rule_matches`), and the
daemon also bounds the arguments after a prefix (`go test -exec sh`, `cat ~/.ssh/id_rsa`,
`git diff --output=../x` ask again).
✅ The card shows the exact rule before the click, and the rule is stored only after the answer
reached the agent.
❌ More prompts: `npm install lodash` is remembered only as that exact command; a different
package asks again.
❌ Allowlisted prefixes are not harmless in every case: `cargo/npm/go/make test|build` run the
repo's own code (build scripts, tests the agent may have edited — though those edits need their
own permission). The flag check is a list of known escapes, so a runner flag it misses would
still pass.
❌ Prefix rules only work in chat-mode sessions (the daemon answers their requests); a terminal
session of the same task asks again in the terminal. Quoted arguments (`grep "a b"`) and
absolute paths ask again too.
❌ The allowlist is a list in code (`SAFE_BASH_PREFIXES` in `crates/delilah-agents/src/claude.rs`)
that needs maintenance as new tools show up; rules stored before this change that no longer
pass are silently ignored.
❌ There is no UI yet to review or revoke a task's remembered rules.

## When to reconsider

When a "review / revoke rules" view exists in the task screen, or if after a month of real
chat-mode use most "Permitir siempre" clicks end up as exact-command rules that never match
again (users asking for the same tool with new arguments), revisit the allowlist: add the
recurring safe prefixes, or replace the flag denylist with a per-entry list of allowed flags.
