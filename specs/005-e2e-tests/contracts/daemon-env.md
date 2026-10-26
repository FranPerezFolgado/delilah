# Contract: new daemon environment variables

All are read once in `Config::from_env` (`crates/delilah-daemon/src/config.rs`). Unset,
the daemon behaves exactly as today. None changes the HTTP API (`api.rs` ↔ `api.ts`
untouched).

| Variable | Values | Effect | Unset |
|---|---|---|---|
| `DELILAH_AGENT_BIN` | absolute path to an executable file | `Config.agent_program`: launched instead of `claude` for every session (tasks, project sessions, conversations, resume), with the same args, env and cwd | `claude` from the agent's PATH |
| `DELILAH_SECRETS` | `memory` | `Secrets::Memory` instead of the Keychain | Keychain |
| `DELILAH_JIRA_ALLOW_HTTP_LOOPBACK` | `1` | a Jira site `http://127.0.0.1:<port>` or `http://localhost:<port>` is accepted; any other `http://` is still rejected | only `https://` |
| `DELILAH_PR_POLL_SECS` | integer ≥ 1 | the PR loop's tick (`Config.pr_poll`, default `DEFAULT_PR_POLL`) | 30 |

`DELILAH_GH_BIN` (older, from the PR loop) goes through `Config` too since the review
of this feature: `Config.gh_program`, the same check and error as `DELILAH_AGENT_BIN`,
noted at start as `gh program overridden`.

**Errors at start** (the daemon logs them and exits non-zero, like an invalid
`DELILAH_ADDR` today):
- `DELILAH_AGENT_BIN` relative, missing, or not executable:
  `DELILAH_AGENT_BIN must be an absolute path to an executable, got <value>`.
- `DELILAH_SECRETS` other than `memory`: `DELILAH_SECRETS must be "memory" or unset`.
- `DELILAH_PR_POLL_SECS` not an integer ≥ 1.

**Warned at start** when on (built from the parsed `Config` by
`Config::override_notes`, as `test-only switch on: …`): `agent program overridden`,
`gh program overridden`,
`secrets in memory`, `jira over http on loopback allowed`, `pr poll every N s` (only
when it differs from 30), so a daemon log shows at a glance that it is a test daemon.
