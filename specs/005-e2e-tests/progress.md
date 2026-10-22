# Progress — 005-e2e-tests

Branch: feature/LIBRE-8 · Base: 90d8fbe · Started: 2026-10-22
Mode: format=markdown commits=yes

| WP | Phase | Group | Status | Attempts | Commit | Notes |
|----|-------|-------|--------|----------|--------|-------|
| WP-01 | 1 | 1A | done | 1 | 17b3ef2 | runs with 2A |
| WP-02 | 2 | 2A | done | 1 | f813aca | runs with 1A |
| WP-03 | 2 | 2B | done | 1 | de1f28e | |
| WP-04 | 2 | 2C | done | 1 | 1f16da4 | |
| WP-05 | 3 | 3A | done | 1 | e83fa1a | |
| WP-06 | 3 | 3B | done | 1 | 444fb02 | |
| WP-07 | 3 | 3C | done | 1 | 9dcd9d6 | |
| WP-08 | 3 | 3D | done | 1 | 80edf04 | |
| WP-09 | 4 | 4A | done | 1 | 832ebdf | Phase 4 ∥ Phase 5 |
| WP-10 | 4 | 4A | done | 1 | ff652a3 | |
| WP-11 | 4 | 4A | done | 1 | 151cd84 | |
| WP-12 | 4 | 4B | done | 0 | | no change: CLI calls already in WP-09/10 (project add, new, ls, show, done, archive, rm) |
| WP-13 | 5 | 5A | done | 1 | f276089 | |
| WP-14 | 5 | 5B | done | 1 | 493dff7 | |
| WP-15 | 5 | 5B | done | 1 | 453beb1 | |
| WP-16 | 6 | 6A | verified | 1 | ac6020c | |
| WP-17 | 6 | 6A | verified | 1 | 15b33c5 | |
| WP-18 | 6 | 6B | verified | 1 | | manual parts (T049, T056) |
| WP-19 | 6 | 6C | pending | 0 | | |
| WP-01F | 1 | 1A-fix | done | 1 | 5bfff99 | Phase 1 review fixes |
| WP-02F | 2 | 2-fix | done | 1 | ea390bd | Phase 2 review fixes |
| WP-08F | 3 | 3-fix | done | 1 | eb8cdcb | Phase 3 review fixes |
| WP-45F | 4+5 | 45-fix | done | 1 | 781d7e5 | Phase 4/5 review fixes |
| WP-17F | 6 | 6-fix | done | 1 | 3b518c6 | doc example fixes |

## Verification (WP-18)

**US4-3 / US4-4 (T049)**
- Installed daemon: PID 70524 on 7420 before, PID 70524 after all runs (api, ui, 10 full runs, Ctrl-C tests); `/api/v1/health` answered OK (version 0.4.0, /Applications/Delilah.app) before and after.
- Ctrl-C mid-API-layer: `scripts/e2e.sh api` started in its own process group, `kill -INT -- -PGID` once an e2e delilahd was listening (4 attempts, 3 of them with a `conversation` test binary and its delilahd alive). After each: `pgrep -fl 'e2e-env|fake-claude'` empty, only the installed delilahd (70524) listening, process group empty. Leftovers: none. Fix applied: none needed (daemons share the group and receive the SIGINT; the harness Drop/children exit). Note: an INT during the first lib-test binary made 3 lib tests fail (fixture git commands interrupted), expected for an interrupted run.

**Checks (T055)**
| Check | Result |
|---|---|
| `cargo test --workspace` | pass (515 passed, 0 failed) |
| `cargo clippy --workspace --all-targets` | exit 0, 8 warning lines (collapsible if x3, MutexGuard across await x2, needless borrow x1, plus summaries), all in delilah-daemon api.rs/handoff.rs code not touched by this branch (branch only adds test lines there): no new warnings |
| `cargo test --manifest-path app/src-tauri/Cargo.toml` | pass after `scripts/build-sidecar.sh debug` (fails before it: sidecar binary missing) |
| `cd app && npx tsc --noEmit && npx vitest run` | pass (22 files, 168 tests) |
| `scripts/e2e.sh` | pass (all 10 runs below) |

**Timing (T056)**
- `scripts/e2e.sh api`: 57 s including build (limit 3 min). `scripts/e2e.sh ui`: 30 s (limit 5 min).
- 10 consecutive `scripts/e2e.sh` runs: all PASS. Seconds: 80, 82, 87, 79, 78, 105, 87, 86, 81, 79.
- Flaky tests: none observed; no test changed.
