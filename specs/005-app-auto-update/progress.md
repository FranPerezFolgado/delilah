# Progress — 005-app-auto-update

Branch: feature/LIBRE-10 · Base: 011182e · Started: 2026-10-24
Mode: format=markdown commits=yes autonomy=auto

| WP | Phase | Group | Status | Attempts | Commit | Notes |
|----|-------|-------|--------|----------|--------|-------|
| WP-01 | 1 | 1A | done | 1 | 5f9c2d1 | hook log empty; implementer checks pass |
| WP-02 | 2 | 2A | done | 1 | 3af5f13 | event names + open transitions decided by implementer |
| WP-03 | 3 | 3A | verified | 1 | 250d869 | marker sanitised to plain token |
| WP-04 | 3 | 3A | verified | 1 | c7f4f28 | "Actualizar" already = "Refresh" in en.ts (open question) |
| WP-05 | 3 | 3A | verified | 1 | 064cff2 | |
| WP-06 | 4 | 4A | verified | 1 | eb234eb | run tests with `node --test scripts/release/*.test.mjs` |
| WP-07 | 4 | 4B | verified | 1 | 6f180c0 | actionlint/shellcheck not installed; workflow + dist build untried |
| WP-08 | 4 | 4B | verified | 1 | | lockfiles via cargo metadata (sidecar-free); bare-remote trial OK |
| WP-09 | 5 | 5A | pending | 0 | | |
| WP-10 | 6 | 6A | verified | 1 | c9865fc | also shows+focuses main window before emit |
| WP-11 | 6 | 6A | pending | 0 | | |
| WP-12 | 7 | 7A | pending | 0 | | |
| WP-13 | 7 | 7B | pending | 0 | | |

T005 (updater key + secrets) is maintainer-only: left to the user.

## Open findings

| Phase | Reviewer | Severity | Finding | File |
|-------|----------|----------|---------|------|
| 1 | security | LOW | package-lock.json regen dropped `libc` fields from optional native packages (npm version drift) | app/package-lock.json |
| 1 | security | LOW | `updater:default` grants check/download/install to main webview; fine while it serves only local UI | app/src-tauri/capabilities/default.json |
| 2 | spec | MEDIUM | `reduce` relies on plugin `check()` for "strictly newer" (FR-003); no guard vs `meta.installed` | app/src/updates.ts |
| 2 | spec | LOW | `appVersion()` gates on isDesktop, not canUpdate (returns version in tauri dev) — docs should agree | app/src/platform.ts |
| 2 | spec | LOW | untested: auto check in available/confirmSessions, check from upToDate/error, dismissed cleared on manual find, meta after manual failure | app/src/updates.test.ts |
| 2 | spec | LOW | manual check silently ignored in busy states — UI gives no feedback | app/src/updates.ts |
