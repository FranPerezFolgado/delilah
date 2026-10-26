# Quickstart: running the end-to-end tests

Safe with the installed Delilah running: every run uses its own temp home, a free port
and fake `claude`/`gh`/Jira. Nothing reaches 7420 or `~/.delilah`.

```bash
scripts/e2e.sh            # everything: API/CLI layer, then UI layer
scripts/e2e.sh api        # Rust flows only (fast)
scripts/e2e.sh ui         # Playwright flows only
scripts/e2e.sh api -- prs # one test file / name filter
DELILAH_E2E_KEEP=1 scripts/e2e.sh api   # keep failed tests' temp roots
```

## Checking the feature by hand

1. **Normal checks unaffected**: `cargo test --workspace` doesn't run the flows (they
   need `--features e2e`), and stays about as fast as before.
2. **Fake Claude only**: `scripts/e2e.sh api`. It passes, and no
   `tripwire.log` is reported. With the installed Delilah open, its sessions keep
   running and `curl -s 127.0.0.1:7420/api/v1/health` is still OK afterwards.
3. **Tripwire works**: temporarily unset `DELILAH_AGENT_BIN` in the harness, run one flow:
   it fails with "the real claude was run".
4. **No leftovers**: after a run (also after Ctrl-C mid-run), `pgrep -fl
   'delilah-e2e|fake-claude'` lists nothing, and no `delilahd` other than the
   installed one (check its PID is unchanged, don't kill anything by name).
5. **A debug environment**: `target/debug/e2e-env start --jira --keep`, use the printed
   `api` with `DELILAH_URL=<api> target/debug/delilah ls`, then press Ctrl-D.
6. **UI**: `scripts/e2e.sh ui`. On a failing test, `app/playwright-report/` has the
   screenshot, trace and the daemon log.
7. **Smoke test (costs one short real session)**: `scripts/e2e.sh --real-claude`.

## Adding a flow

API: a new `#[test]` in `crates/delilah-e2e/tests/<area>.rs`:

```rust
let env = Env::start(EnvOptions::default());
env.scenario("asks", json!({"steps": [{"ask": {"question": "¿A o B?", "options": ["A", "B"]}}, {"stop": "ok"}]}));
let key = env.api().new_task(&env.fixtures.repo_project(), "Add X [fake:asks]");
let ask = env.wait_until("ask in inbox", || env.api().inbox().asks().first().cloned());
env.api().answer(&ask.id, "A");
env.wait_until("fake got the answer", || env.record_for(&key).answers.first().cloned());
```

UI: a new `app/e2e/<flow>.spec.ts` using the `e2e` fixture (`api`, `fixture()`,
`scenario()`), as the existing specs do.

## Chat mode (stream-json)

When the daemon runs it with `--input-format stream-json` (`claude::stream_command`),
the fake speaks the stream protocol of `docs/spikes/2026-10-24-claude-stream.md`
(`crates/delilah-e2e/src/fake/stream.rs`): each user line on stdin plays the next entry
of the scenario's `turns` (picked at the first message: a `[fake:<name>]` marker in it,
else `<task>.json`, else `default.json`; once the turns run out it answers
`Echo: <message>`). Every turn prints `system/init`, fires `UserPromptSubmit`, plays its
steps, then a `rate_limit_event`, a `result` and the `Stop` hook; the user message, the
assistant messages and the tool calls and results are appended to the transcript under
the temp home, where the daemon reads the conversation. An `interrupt` is answered as
the real CLI does. Steps of a turn:

```json
{"turns": [
  [{"say": {"text": "Uno dos tres", "delay_ms": 250}}],
  [{"tool": {"name": "Bash", "input": {"command": "ls -la"}, "output": "total 0"}},
   {"say": {"text": "Hecho."}}],
  [{"tool": {"name": "Bash", "input": {"command": "git diff HEAD"}, "ask": true,
     "suggestions": [{"type": "addRules", "behavior": "allow",
                      "rules": [{"toolName": "Bash", "ruleContent": "git diff:*"}]}]}}],
  [{"sleep_ms": 500}, {"exit": 0}]
]}
```

`say` streams the text as word deltas `delay_ms` apart, then the complete message. `tool`
sends `can_use_tool` (unless `"ask": false`; `suggestions` default to the exact command)
and waits for the `control_response`: allowed, the result is `output`; denied, the denial
message, as an error. Each answer is kept in the record's `permissions`
(`<fake_dir>/records/<session>.json`). Start a chat-mode task with `chat_mode: true` and a
`prompt` carrying the marker in `POST /api/v1/runs`.

Covered in `app/e2e/chat.spec.ts`: the chat view with live text that becomes the final
message; permission cards in the chat and the inbox (allow once, deny with a reason);
"allow always" storing the exact rule and a prefix rule (`Bash(git diff:*)`) that covers
`git diff --stat` but not `git diff --output=../x`; the runtime switch from the session's
Chat | Terminal switch (refused with a card waiting, then to the terminal and back with
"Probar modo chat"); a stopped chat session not reconnecting to its live socket. The fake's
stream mode itself: `crates/delilah-e2e/tests/fake_bin.rs` and the unit tests in
`src/fake/stream.rs`.
