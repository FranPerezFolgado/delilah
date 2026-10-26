# Implementation Plan: Chat mode asks for permission as often as terminal mode

**Branch**: `009-chat-mode-permissions` | **Date**: 2026-10-26 | **Spec**: [spec.md](spec.md)

**Input**: Feature specification from `specs/009-chat-mode-permissions/spec.md`

## Summary

Chat mode starts Claude in print mode. Print mode ignores the user's interactive default
(auto mode), so every tool outside the allow list becomes a `can_use_tool` ask, and
autopilot pauses on each one.

A probe during planning confirmed this. It ran five test actions with the exact chat-mode
startup:

- no flag: 5 asks;
- `--permission-mode auto`: 0 asks, and all five actions ran (research.md R1).

The fix makes the mode explicit and the same in both runtimes, as the user decided:

- a new preference `agent-permissions` (`auto`, the default, or `default`);
- a new `claude::Permission::Auto`;
- `run::launch_flags` uses them for task and project sessions.

The work is test-first:

1. extend the existing stream spike and record the findings;
2. write failing unit tests and an e2e test, using a fake agent that now models auto mode;
3. make the change;
4. add a setting in Ajustes, and a start-failure hint that names it.

Autopilot is unchanged: asks that still arrive keep pausing it (#43).

## Technical Context

**Language/Version**: Rust (workspace edition as in `Cargo.toml`) and TypeScript/React (`app/`)

**Primary Dependencies**:
- the `claude` CLI (2.1.292 at the time of the probe): `--permission-mode auto`,
  `--permission-prompt-tool stdio`;
- axum, sqlx (SQLite);
- Vite + vitest.

**Storage**: the existing `settings` table, one row `pref:agent-permissions`. No migration.

**Testing**:
- `cargo test -p delilah-agents`, `-p delilah-daemon`, `-p delilah-e2e` (fake claude);
- `npx vitest` for `SettingsView`;
- the real-CLI spike as a manual example run.

**Target Platform**: macOS (daemon + Tauri app)

**Project Type**: desktop app with a local daemon and CLI

**Performance Goals**: n/a. One more flag at spawn.

**Constraints**:
- never more permissive than the user chooses: no `bypassPermissions`, no allow-list
  changes;
- idea conversations stay `dontAsk` and read-only sessions stay `plan`;
- try changes only in `scripts/preview.sh`, never on the installed daemon.

**Scale/Scope**:
- about 6 Rust files and 2 TS files, plus the spike example and docs;
- no API route changes.

## Constitution Check

`.specify/memory/constitution.md` is still the unfilled template, so there are no
project gates. The repo's own rules in CLAUDE.md apply instead:

- previews only;
- agent hooks stay in the `--settings` file;
- docs updated when behaviour changes;
- no version bump.

The plan follows them. **PASS** before and after design.

## Project Structure

### Documentation (this feature)

```text
specs/009-chat-mode-permissions/
├── spec.md, plan.md, research.md, data-model.md, quickstart.md
├── contracts/agent-permissions.md
├── checklists/requirements.md, checklists/permissions.md
└── tasks.md            # /speckit-tasks
```

### Source Code (repository root)

```text
crates/delilah-agents/
├── src/claude.rs                       # Permission::Auto → --permission-mode auto (both runtimes)
└── examples/claude_stream_spike.rs     # `routine` scenario + `--permission-mode <m>` option (spike)

crates/delilah-daemon/src/
├── state.rs          # enum AgentPermissions { Auto, Default } in Arc<AtomicU8>; agent_permissions(), load_agent_permissions()
├── preferences.rs    # `put`: if-chain → `match key` (reload + optional validator); validation for `agent-permissions`
├── api.rs            # doc comment on the preference route: 400 for an invalid `agent-permissions`
└── run.rs            # launch_flags(session, config, AgentPermissions) stays pure → Auto | Default;
                      # one start-failure hint function next to spawn-failure handling (both runtimes)

crates/delilah-e2e/
├── src/fake/scenario.rs   # Tool.auto: allow | ask (mirrors the spike's table)
├── src/fake/stream.rs     # honour it when launched with --permission-mode auto
├── src/fake/args.rs       # (parses --permission-mode already)
└── tests/chat_permissions.rs   # new e2e file (routine: 0 cards; ask: card + autopilot pause; default: 4 cards)

app/src/
├── components/SettingsView.tsx   # "Permisos de los agentes": Auto / Por defecto de Claude
└── components/SettingsView.test.tsx

docs/
├── spikes/2026-10-26-chat-permission-mode.md   # spike record (FR-001)
├── status.md, architecture.md, decisions.md (new decision), README.md
```

**Structure Decision**:
- Use the existing preference mechanism (`preferences.rs` and `AppState`, as
  `remote-control` does) and the existing `Permission` enum. No new module.
- The fake agent learns permission modes so that the e2e test checks the behaviour, not
  only the flag.

## Phases

1. **Spike (gate, FR-001).**
   - Extend `claude_stream_spike.rs`:
     - a `routine` scenario: Write, `sed -i`, a repo script, `git commit`;
     - a control action;
     - a `--permission-mode <m>` option.
   - Run it with no mode and with `auto`.
   - Find an action that auto mode escalates, and record whether it comes as
     `can_use_tool` or `permission_denied`.
   - Record what the CLI says when the mode is rejected.
   - Confirm that a PTY session with no flag reports auto.
   - Write `docs/spikes/2026-10-26-chat-permission-mode.md` with the CLI version.
   - **If the spike contradicts R1, stop and revise the plan.**
2. **Failing tests (FR-002, FR-007).**
   - Unit tests for `Permission::Auto` argv (both runtimes).
   - Unit tests for `launch_flags` under both preference values.
   - Unit tests for the preference PUT validation and reload.
   - The fake `Tool.auto` with its own unit tests.
   - The e2e scenarios (routine → 0 cards; `auto: "ask"` → card, and the autopilot pause on a
     `feature`-workflow task; with `default` → 4 cards).
   - Run them and record that they fail.
3. **Fix.** Also add a test that changing the preference does not restart running
   sessions, and that the next relaunch (runtime switch) gets the new mode.
   - `Permission::Auto`;
   - `AppState::agent_permissions` and loading it;
   - preference validation;
   - `launch_flags` uses it at its two call sites (`spawn_session` and its tests).
4. **Start failure hint (FR-008), its own task after the spike.**
   - One function in `run.rs` matches the CLI's rejection text (from the spike) and adds
     the hint naming the setting, in the app's language.
   - Both runtimes use it: the chat start check, and a PTY exit before any work.
   - It is tested with the fake agent exiting with that stderr, in each runtime.
5. **UI (FR-010).**
   - `SettingsView` gets a two-option control, in both languages, saved through
     `setPreference`.
   - Vitest covers that it reads, saves and shows the default.
6. **Docs (FR-009).**
   - `status.md`: update "Chat mode, known problems", add the setting, and list what is
     still pending.
   - `architecture.md`: launch flags per session kind, and the preference.
   - `decisions.md`: a new decision covering why a setting, why auto is the default (and
     that it widens the terminal for users whose Claude default is not auto), what was
     ruled out, and how it relates to #43.
   - README: the setting.
7. **Verify in a preview** (quickstart.md).

## Complexity Tracking

None. No new modules, no migration, no new route.

## Plan review (architecture-reviewer, 2026-10-26)

- **HIGH, parity wording against FR-003/FR-004.** Fixed in the spec: parity is by
  construction through the setting; the upgrade widening for users whose default is not
  auto is stated; the edge cases are rewritten.
- **HIGH, the change also reaches PTY sessions.** This is the user's explicit choice
  ("Setting, both runtimes"). Its risks are stated:
  - an account without auto fails every session, so FR-008 now covers both runtimes and
    the PTY path is specified;
  - the default is recorded in `decisions.md`.

  A pre-flight check of the CLI was not added. The spike records the rejection text, and
  the failure names the setting.
- **MEDIUM, folded in:**
  - an `AgentPermissions` enum passed into `launch_flags` (it stays pure);
  - an `Arc<AtomicU8>` instead of the `remote_control` `AtomicBool`;
  - `preferences::put` becomes a `match key`;
  - the 400 is documented in `api.rs` and `api.ts`;
  - the data-model wording;
  - the fake's `auto` limited to `allow`/`ask`, mirroring the spike;
  - the start hint has one owner and its own task;
  - a test that running sessions are not restarted.
