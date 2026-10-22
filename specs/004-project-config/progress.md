# Progress — 004-project-config

Branch: feature/LIBRE-2 · Base: d13d825 · Started: 2026-10-21
Mode: format=markdown commits=yes

| WP | Phase | Group | Status | Attempts | Commit | Notes |
|----|-------|-------|--------|----------|--------|-------|
| WP-01 | 1 | 1A | done | 1 | d2ef468 | |
| WP-02 | 2 | 2A | done | 2 | 3aea005 | +core Cargo.toml dev-dep; FieldError has code |
| WP-03 | 2 | 2A | done | 1 | d58e628 | |
| WP-04 | 2 | 2B | done | 1 | 6ec349e | |
| WP-05 | 2 | 2A | done | 1 | b3fe227 | |
| WP-06 | 2 | 2A | done | 1 | 41936c8 | +Sidebar.test.tsx fixture |
| WP-07 | 3 | 3A | done | 1 | 4ff756f | +R1-R3 review follow-ups; cargo fmt of run.rs, store_projects.rs |
| WP-08 | 3 | 3A | done | 1 | f446162 | |
| WP-09 | 3 | 3A | done | 1 | 17cebbd | |
| WP-10 | 4 | 4A | done | 2 | c29d87f | |
| WP-11 | 4 | 4B | done | 1 | 1fcdaa7 | +app/tsconfig.json resolveJsonModule |
| WP-12 | 4 | 4B | done | 1 | 8cb6fce | |
| WP-13 | 5 | 5A | done | 1 | 0647a04 | +S1-S3 symlink/.git guards |
| WP-14 | 5 | 5A | done | 1 | 91a288f | |
| WP-15 | 5 | 5B | done | 1 | 3c056ae | +kind arms in run/actions/store/terminal, lib.rs mod; launch.json for retry |
| WP-16 | 5 | 5C | done | 1 | 25aec7a | share notes in task_dir/workspace.log; setup routes return 202 + task |
| WP-17 | 5 | 5D | done | 1 | 29ea27f | |
| WP-18 | 5 | 5D | done | 1 | 25b53bc | +setup-kind arms in ProjectView/TaskView/Sidebar/TerminalPane/sessionLabels |
| WP-19 | 6 | 6A | done | 1 | 27a45f2 | |
| WP-20 | 6 | 6A | done | 1 | 6a1d630 | |
| WP-21 | 7 | 7A | pending | 0 | | |
| WP-22 | 7 | 7A | pending | 0 | | |
| WP-23 | 8 | 8A | pending | 0 | | |
| WP-24 | 8 | 8B | pending | 0 | | |
| WP-09F | 3 | 3F | done | 1 | a8e72ce | review fixes: picker paths, unplaced errors, stale rules, save confirm, docs, CLI old-daemon msg |
| WP-10F | 4 | 4F | done | 1 | e235a95 | review fixes: `/` separator around empty slug, free-origin preview |
| WP-15F | 5 | 5F | done | 1 | 67b0246 | review fixes (daemon): pgroup kill, single launch seam, launch persistence, cancel flag, recovery order, retry/skip rollback, can_archive, notify |
| WP-18F | 5 | 5F | done | 1 | b4adc52 | review fixes (app): archive setup-failed, reason labels, setup-running helper, Jira/Sidebar status |
| WP-19F | 6 | 6F | done | 1 | c0fd04c | review fix: settings pickers need never submodules (?all=true) |
| WP-02F | 2 | 2F | done | 1 | dfcf408 | review fixes: pty exit outcome, FieldError.code in TS + api.md |

## Review notes

Phases 1–2 (d13d825..53c4cb0): spec ✅ compliant · contract ⚠️ needs care · migration ✅ safe · architecture ✅ approve.
Carried into later briefs:
- WP-07: unknown-key PUT body must return 422 `{error, fields}`; PATCH changing kind must re-validate or clear submodule rules; trim blank `setup` to null on save.
- WP-15: `launch_agent` returns a domain error (not ApiError) and is `pub(crate)`; AgentLaunch prompt as an enum if convenient.
Open LOW (not fixed): migration test on pre-existing rows; SharedMode→ShareMode TS rename; resolved_mode helper; shared Rust/TS fixture test; key.to_string() repeats.

Phase 3 security ✅ (no exploitable). Carried into WP-13: canonicalise shared source under the project root, never write through an existing symlink in the worktree, reject a `.git` component (and `\`, NUL, leading `~` in validate if cheap); test the symlink case. Setup execution (WP-15) gets a second security look.
Phase 3 (353a293..HEAD): spec ⚠️ gaps → fixed in WP-09F · contract ⚠️ needs care → fixed in WP-09F · security ✅ · UI ⚠️ partial (no browser; API/CLI pass, screen untested in a browser).
Open LOW: no test for "Ajustes guardados" row confirmation; PATCH kind-change does two non-atomic writes; Spanish submodules.unknown message omits the path; CLI 422 output untested end-to-end.
Phase 4 (21862f6..cd91b76): spec ⚠️ minor gaps (TS≡Rust on 41 inputs) → WP-10F: `/` as separator around an empty {slug}; free-origin preview uses LIBRE-?; shared cases for `{slug}` twice / non-ASCII template. UI not checked in a browser. Open LOW: ß/Æ become hyphens (per research.md).
WP-15 notes for WP-16: call setup::recover_interrupted at daemon start before accepting runs; setup::cancel returns before the transition (API should return 202 or poll); retry with no setup configured → Unsupported (409). Known limit: kill reaches only the shell, not grandchildren (no process-group access without unsafe).
Phase 5 (cd91b76..4449dd7): spec ⚠️ gaps · security ✅ (MEDIUM pgroup kill) · architecture ✅ merge with fixes · contract ✅ compatible → WP-15F + WP-18F. UI not checked in a browser.
Phase 6 (0148332..567cd13): spec ✅ compliant · contract ❌ breaking (ProjectSettings pickers lost never submodules) → WP-19F.
