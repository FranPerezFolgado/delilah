# Research: Chat mode asks for permission as often as terminal mode

## R1. Why chat mode asks for everything (probe, 2026-10-26, Claude Code 2.1.292)

Two probes ran during planning, in throwaway `git init` folders in the session scratchpad.
They did not touch the repo or the installed app. Each run asked the model to do five
things:

1. write `notes.txt`;
2. edit it with `sed -i`;
3. run `./run.sh`;
4. `git add -A && git commit`;
5. `curl -X POST -d @notes.txt https://example.com/upload`.

The first four are the spec's routine actions. The fifth was a candidate "needs
permission" control.

**Probe A: `claude -p` with plain stream-json output and no prompt tool.**

| Flag | `system/init` `permissionMode` | Result |
|---|---|---|
| none | `default` | Write refused (`system/permission_denied`), nothing done |
| `--permission-mode auto` | `auto` | all done, 0 denials, commit made |

**Probe B: the exact chat-mode startup** (`-p --input-format stream-json
--output-format stream-json --verbose --permission-prompt-tool stdio`). The harness
answered every `can_use_tool` with deny and counted them.

| Flag | `permissionMode` | `can_use_tool` asks | Result |
|---|---|---|---|
| none (today) | `default` | **5**: Write, Bash ×4 | all refused |
| `--permission-mode auto` | `auto` | **0** | all five ran, commit made |

- **Decision**: the hypothesis holds. Print mode does not apply the user's interactive
  default. The `--permission-mode` flag does, and auto mode works together with
  `--permission-prompt-tool stdio`.
- **Open for the spike (T001)**: auto mode let the `curl` upload through without asking,
  so it is no use as a control. The spike must find an action that auto mode escalates
  and see whether it arrives as a `can_use_tool` (an ask, which pauses autopilot) or as a
  `permission_denied` (a refusal, no ask). The spike also records:
  - what the CLI does with `--permission-mode auto` when auto mode is unavailable (exit
    code and stderr), which FR-008 depends on;
  - that a terminal session with no flag reports auto, as the parity baseline.

## R2. Where the terminal's "auto" comes from

- `~/.claude/settings.json` has no `permissions.defaultMode`.
- The interactive auto default comes from Claude Code's opt-in state in `~/.claude.json`
  (`hasResetAutoModeOptInForDefaultOffer`, `hasSeenAutoDefaultNotice`). That is
  undocumented internal state.
- **Decision**: do not read it. Parity becomes explicit through a Delilah setting
  (R3).
- **Alternatives**:
  - always pass auto in chat mode: more permissive than the terminal for a user whose
    default is not auto;
  - detect the user's default from `~/.claude.json`: fragile.

## R3. A Delilah setting for agent permissions, used by both runtimes (user decision)

- **Decision**:
  - A new app preference `agent-permissions` with two values:
    - `"auto"` (the default when unset): pass `--permission-mode auto`;
    - `"default"`: pass no flag, as today.
  - It applies to every Claude session that today gets `Permission::Default`, in both
    runtimes: task sessions and project sessions. Idea conversations stay `dontAsk` and
    read-only sessions stay `plan`.
  - It is stored like `remote-control`:
    - the `settings` row `pref:agent-permissions`;
    - loaded into `AppState` at start and on `PUT /api/v1/preferences/agent-permissions`;
    - read by `run::launch_flags`.
  - The PUT validates the value: only `"auto"` or `"default"`, else 400.
- **Rationale**:
  - The same mode in both runtimes is parity by construction. Today the terminal follows
    Claude's hidden default and chat does not.
  - The user's choice is explicit, and "Claude's default" keeps today's behaviour for
    anyone who wants it.
  - The default `auto` matches what this machine runs today, so on this machine it
    changes nothing in the terminal. For a user whose Claude default is not auto, terminal
    sessions become more permissive after upgrading. That is accepted, recorded in
    `decisions.md` and the release notes, and "Claude's default" restores the old
    behaviour.
- **Risks**:
  - Auto mode may be unavailable on some accounts. In that case every session fails at
    start. The failure must name the setting (FR-008, the spike records the CLI's
    message).
  - Fresh installs (008 public onboarding) default to auto. That is acceptable because
    the setting is visible in Ajustes with an explanation.
- **Alternatives**:
  - a per-task or per-project setting: no need yet (YAGNI);
  - a new DB column: the generic `settings` table is enough.

## R4. Testing the behaviour, not the flag

- **Decision**: three levels.
  1. **Unit** (`delilah-agents::claude`): `Permission::Auto` maps to `--permission-mode
     auto` in both `command` and `stream_command`. The `--setting-sources` and
     `--strict-mcp-config` flags stay `dontAsk`-only.
  2. **Unit** (`run::launch_flags`): with the preference unset or `auto`, a task or
     project session gets `Auto`, and with `default` it gets `Default`. Conversations
     and read-only sessions are unchanged under both values.
  3. **E2E** (`delilah-e2e`, fake claude): the fake models permission modes. A scenario
     tool gets `auto: "allow" | "ask"` (default `"allow"`), which is used only
     when the fake was launched with `--permission-mode auto`; otherwise it asks as
     today.
     - The routine scenario (Write, `sed -i`, a script, `git commit`) in a chat-mode task
       must record **0** permission asks and raise no card. The fake's `auto` semantics
     mirror the spike's table, so they cannot drift silently.
     - A tool marked `auto: "ask"` still raises a card, and on autopilot it pauses with
       "needs permission".
     - With the preference `default`, the same routine scenario raises 4 cards. That is
       the regression check for today's behaviour.
- All three are written first and fail on the current code: there is no `Auto`, and
  the fake ignores the mode.
- **Rationale**: the e2e test drives the daemon end to end. It checks that the mode the
  daemon passes changes what the agent asks, so a test that only checks a flag string
  would not be enough on its own.

## R5. Autopilot needs no change

- `autopilot::pause::on_permission` keeps pausing on every ask that arrives (#43).
- Removing the asks that should never happen is enough for US1.
- A refusal from auto mode reaches the agent as a tool error. It is shown in the
  conversation as a normal tool result: no card, no pause, no history event.
