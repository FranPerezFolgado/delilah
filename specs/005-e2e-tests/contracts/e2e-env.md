# Contract: `e2e-env` and `scripts/e2e.sh`

## `e2e-env` (binary in `crates/delilah-e2e`)

Used by the UI layer (Playwright global setup) and by hand for debugging.

```text
e2e-env start [--ui-origin <url>] [--jira] [--keep]
e2e-env fixture --into <dir>
```

**`start`** builds an `Env` (data-model.md), then prints **one line** of JSON to stdout and
keeps running until its stdin closes or it gets SIGTERM/SIGINT, then drops the Env
(stops the daemon by handle, removes `root` unless `--keep`) and exits 0. The exit code
is 3 if the tripwire fired.

```json
{ "api": "http://127.0.0.1:52811",
  "root": "/var/folders/…/delilah-e2e-x1y2",
  "home": "/var/folders/…/delilah-e2e-x1y2/delilah",
  "log": "/var/folders/…/delilah-e2e-x1y2/delilahd.log",
  "fake_dir": "/var/folders/…/delilah-e2e-x1y2/fake",
  "gh_dir": "/var/folders/…/delilah-e2e-x1y2/gh",
  "jira": "http://127.0.0.1:52812",
  "fixtures": { "repo": "…/fixtures/signal", "parent": "…/fixtures/parent-env/parent" } }
```

`jira` is `null` without `--jira`. Errors go to stderr, exit 1.

**`fixture --into <dir>`** makes a new fixture set there and prints
`{"repo": …, "parent": …}`.

## `scripts/e2e.sh`

```text
scripts/e2e.sh                 # both layers
scripts/e2e.sh api             # API/CLI layer only
scripts/e2e.sh ui              # UI layer only
scripts/e2e.sh --real-claude   # only the real-Claude smoke test (costs the subscription)
scripts/e2e.sh api -- <filter> # passes a test name filter on
```

Steps:
1. `cargo build -p delilah-daemon -p delilah-cli -p delilah-e2e`.
2. API: `cargo test -p delilah-e2e --features e2e -- <filter>`.
3. UI: `npm --prefix app ci` if `app/node_modules` is missing, `npx playwright install
   chromium` if missing, then `npx --prefix app playwright test`.
4. Prints `api: PASS|FAIL (n tests, t s)` and `ui: PASS|FAIL (…)`; exits non-zero if
   any layer failed.

Environment: `DELILAH_E2E_KEEP=1` keeps the temp roots of failed tests and prints their
paths. The script never reads or exports `DELILAH_URL`, `DELILAH_HOME` or
`DELILAH_ADDR` from the caller (an agent inside Delilah may have them set); it unsets
them before running anything.
