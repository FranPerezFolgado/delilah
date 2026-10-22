# Data model: End-to-end tests

No database changes. The entities are files and in-memory structs of the test harness
(`crates/delilah-e2e`).

## Env (one isolated daemon)

Created by `Env::start(EnvOptions)`, owned by one test (API layer) or one Playwright run
(UI layer). Dropping it stops everything it started.

| Field | Meaning |
|---|---|
| `root` | temp dir (`tempfile::TempDir`); kept on failure only when `DELILAH_E2E_KEEP=1` |
| `api` | `http://127.0.0.1:<port>` |
| `daemon` | `std::process::Child` of `delilahd`, killed by handle on drop (never by name) |
| `jira` | the fake Jira server's URL and its thread's shutdown handle |
| `fixtures` | `Fixtures { repo, parent, remotes }`, this Env's own copies |
| `log` | `<root>/delilahd.log`; its tail is printed when a wait or the test fails |

Layout of `root`:

```text
<root>/
├── home/            HOME for the daemon and agents (.zshrc, .gitconfig, empty .claude/)
├── delilah/         DELILAH_HOME
├── bin/             claude-fake, gh-fake wrappers (absolute paths baked in)
├── tripwire/        claude, gh → append to ../tripwire.log, exit 1
├── fake/
│   ├── scenarios/   <name>.json, <task>.json, default.json
│   └── records/     <session>.json, one per fake session, rewritten as it runs
├── gh/              FAKE_GH_DIR: repo.json, user.json, pr-list.json, pr-<n>.json
├── fixtures/        parent-env/ (make-fixture-parent.sh) and signal/ (clone)
└── delilahd.log
```

`EnvOptions`: `default_scenario` (a `Scenario`, default "stop then wait for input"),
`ui_origin: Option<String>`, `real_claude: bool` (smoke test only: no fake, no
tripwire, user's HOME), `jira: bool`.

**Rules**
- The port is never 7420 nor 7501–7509 (re-picked if the OS hands one out).
- On drop: kill the daemon, wait for it, stop the Jira thread; then fail the test if
  `tripwire.log` exists (unless `real_claude`).

## Scenario (what a fake session does)

`<root>/fake/scenarios/<name>.json`:

```json
{ "steps": [
  { "trust_dialog": true },
  { "print": "Fake Claude ready" },
  { "hook": "UserPromptSubmit" },
  { "sleep_ms": 200 },
  { "ask": { "question": "¿Pagino con cursor u offset?", "header": "Paginación",
             "options": ["Cursor", "Offset"] } },
  // an option may be an object; `Nueva tarea` + `Crear` with a `preview` is the
  // task-from-idea card the daemon turns into a proposal
  { "ask": { "question": "Añadir búsqueda", "header": "Nueva tarea",
             "options": [{ "label": "Crear", "description": "…", "preview": "brief" }, "Descartar"] } },
  { "stop": "Listo, he terminado." },
  { "wait_input": true },
  { "fail": "boom" },
  { "exit": 0 }
]}
```

| Step | Effect |
|---|---|
| `trust_dialog` | prints the folder-trust dialog (title, `❯ 1. Yes, proceed`, `2. No, exit`) and waits for Enter; on "No" exits 1 |
| `print` | writes text plus newline to the terminal |
| `hook` | runs the settings' hook for that event with a minimal payload (`hook_event_name`, `session_id`, `cwd`) |
| `sleep_ms` | waits (at most 5000) |
| `ask` | runs the `PreToolUse` / `AskUserQuestion` hook with a `tool_input.questions` payload; reads the hook's stdout; records `updatedInput.answers`; then a `PostToolUse` hook |
| `stop` | runs the `Stop` hook with `last_assistant_message` |
| `wait_input` | reads one line from the terminal, records it, runs `UserPromptSubmit`, goes on |
| `fail` | runs `StopFailure`, prints the message, exits 1 |
| `exit` | runs `SessionEnd`, exits with the code |

Before the first step the fake always runs `SessionStart`. After the last step (if it
didn't exit), it waits for input forever, like Claude idle: every line typed is
recorded and answered with `UserPromptSubmit` + `Stop`. On EOF or SIGHUP it runs
`SessionEnd` and exits 0.

**Validation**: unknown step → the fake prints the error, runs `StopFailure`, exits 2
(the test sees a failed session, never a hang).

**Selection** (R8): `[fake:<name>]` in the prompt → `<task>.json` → `default.json`.

## Record (what a fake session saw)

`<root>/fake/records/<session>.json`, rewritten after each step:

| Field | Meaning |
|---|---|
| `args` | the full argv |
| `mode` | `new` or `resume` |
| `session_id` | from `--session-id` / `--resume` |
| `task` | from the settings' hook URL |
| `cwd` | working directory |
| `env` | the environment it got (names and values) |
| `settings` | path of the `--settings` file |
| `prompt` | the text after `--`, if any |
| `scenario` | the name chosen |
| `steps_done` | how many steps ran |
| `hooks` | `[{ event, exit_code }]` in order |
| `answers` | answers received by `ask` steps, in order |
| `inputs` | lines read from the terminal, in order |
| `exited` | `null` while running, then the exit code |

## Fixtures

`Fixtures::make(into)` runs `scripts/make-fixture-parent.sh <into>/parent-env` and clones
`remotes/svc-orders.git` into `<into>/signal`, as `preview.sh` does. One set per `Env`;
UI tests ask for more with `e2e-env fixture --into <dir>`.

Fake `gh` data starts as `repo.json` and `user.json` only (no PRs); helpers write
`pr-list.json` / `pr-<n>.json` to open, fail, review and merge a PR.

The fake Jira server answers the routes the client calls (`jira.rs`):
`/rest/api/3/myself`, `/rest/api/3/field`, `/rest/api/3/search/jql` and
`/rest/api/3/issue/<key>`, from `jira-search.json` (an issue is picked out of it by key;
`field` is a small fixed list naming the sprint field `customfield_10020`).
