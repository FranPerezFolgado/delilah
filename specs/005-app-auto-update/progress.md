# Progress — 005-app-auto-update

Branch: feature/LIBRE-10 · Base: 011182e · Started: 2026-10-24
Mode: format=markdown commits=yes autonomy=auto

| WP | Phase | Group | Status | Attempts | Commit | Notes |
|----|-------|-------|--------|----------|--------|-------|
| WP-01 | 1 | 1A | done | 1 | 5f9c2d1 | hook log empty; implementer checks pass |
| WP-02 | 2 | 2A | done | 1 | 3af5f13 | event names + open transitions decided by implementer |
| WP-03 | 3 | 3A | done | 1 | 250d869 | marker sanitised to plain token |
| WP-04 | 3 | 3A | done | 1 | c7f4f28 | "Actualizar" already = "Refresh" in en.ts (open question) |
| WP-05 | 3 | 3A | done | 1 | 064cff2 | |
| WP-06 | 4 | 4A | done | 1 | eb234eb | run tests with `node --test scripts/release/*.test.mjs` |
| WP-07 | 4 | 4B | done | 1 | 6f180c0 | actionlint/shellcheck not installed; workflow + dist build untried |
| WP-08 | 4 | 4B | done | 1 | bc81cfb | lockfiles via cargo metadata (sidecar-free); bare-remote trial OK |
| WP-09 | 5 | 5A | done | 1 | ecd1ee8 | + phase 3 MEDIUM fixes; i18n option 2 (refresh buttons → "Refrescar", since "Recargar" = Reload exists) |
| WP-10 | 6 | 6A | verified | 1 | c9865fc | also shows+focuses main window before emit |
| WP-11 | 6 | 6A | verified | 1 | | + phase 5 P1/P2 (sessionsFailed, unavailable events) |
| WP-12 | 7 | 7A | implementing | 0 | | |
| WP-08F | 4 | fix | done | 1 | 0751083 | phase 4 HIGH fixes + F5–F7; --target skipped |
| WP-08F2 | 4 | fix | done | 1 | 768623e | tauri-action draft writes untagged-* URLs → own latest.json |
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
| 4 | security | MEDIUM | third-party actions on mutable tags while holding signing key; pin to SHAs / protected `release` environment | .github/workflows/release.yml |
| 4 | security | MEDIUM | tag commit not checked to be on main before signing | .github/workflows/release.yml |
| 4 | security | LOW | fixed `NOTES_EOF` heredoc delimiter in $GITHUB_OUTPUT | .github/workflows/release.yml |
| 4 | security | LOW | job-wide contents:write (could split build/publish) | .github/workflows/release.yml |
| 4 | security | LOW | rollback after ambiguous push failure may diverge from a remote that accepted it | scripts/release/cut.mjs |
| 4 | security | LOW | MESSAGE interpolated into shell by make; export vars instead | Makefile |
| 4 | security | LOW | signing key exported to all children incl. npm scripts in dry run | scripts/release.sh |
| 3 | security | LOW | backup file created with default umask (dir is 0700); could chmod 0600 | crates/delilah-daemon/src/upgrade.rs |
| 3 | security | LOW | existing `delilah-<prev>.db` never refreshed after downgrade→upgrade | crates/delilah-daemon/src/upgrade.rs |
| 4 | contract | HIGH→T005 | placeholder pubkey in tauri.conf.json until maintainer sets the real key (T005); check.mjs blocks releases with it | app/src-tauri/tauri.conf.json |
| 4 | contract | MEDIUM | no explicit `--target aarch64-apple-darwin`; relies on macos-14 arch | .github/workflows/release.yml |
| 4 | contract | MEDIUM | latest.json notes trailing newline differs from manifest.mjs (cosmetic) | .github/workflows/release.yml |
| 4 | contract | LOW | re-run after a published release adds assets to it (untested) | .github/workflows/release.yml |
| 3 | architecture | MEDIUM | hook mirrors reducer to decide when to start install (hardcoded event list) → folded into WP-09 | app/src/useUpdates.ts |
| 3 | architecture | MEDIUM | state in useState + ref; mount bypasses reducer; no unmounted guard → folded into WP-09 | app/src/useUpdates.ts |
| 3 | architecture | MEDIUM | UpdateNotice hardcodes `running: 0` → WP-09 (US3) moves session count into the hook | app/src/components/UpdateNotice.tsx |
| 3 | architecture | LOW | two gates (cfg debug_assertions vs import.meta.env.DEV) can disagree; catch "plugin missing" as unavailable | app/src/platform.ts |
| 3 | architecture | LOW | before_open/after_open pair; downgrade treated as upgrade; mtime pruning — document | crates/delilah-daemon/src/upgrade.rs |
| 3 | architecture | LOW | docs: upgrade.rs, version marker, backups/ → WP-12 | docs/architecture.md |
| 3 | spec | MEDIUM | failed manual check drops reason; "No se pudo verificar la actualización" used for check failure with wrong English → folded into WP-09 | app/src/components/UpdateNotice.tsx |
| 3 | spec | LOW | bad signature shows generic "No se pudo actualizar: {reason}" | app/src/components/UpdateNotice.tsx |
| 3 | spec | LOW | "installing" only shown after downloadAndInstall returns | app/src/useUpdates.ts |
| 4 | spec | HIGH | dry run fails: notes.mjs on a not-yet-existing tag → fix WP-08F | scripts/release/notes.mjs |
| 4 | spec | HIGH | lightweight/checkout-rewritten tag yields commit message as notes → fix WP-08F | scripts/release/notes.mjs |
| 4 | contract | HIGH | CI ships tauri-action latest.json unvalidated (draft URLs?) → fix WP-08F | .github/workflows/release.yml |
| 4 | spec | MEDIUM | no tests for CLI/git side → notes CLI test in WP-08F | scripts/release/ |
| 4 | spec | LOW | pickNotes test passes an ignored 3rd arg; gh release list default limit 30 | scripts/release/notes.test.mjs |
| 5 | spec | MEDIUM | api.sessions() failure on clicking Update is silent → folded into WP-11 | app/src/useUpdates.ts |
| 5 | spec | LOW | failed manual check has no "Reintentar" → folded into WP-11 | app/src/components/UpdateNotice.tsx |
| 5 | spec | LOW | shell terminals counted as running sessions (deliberate: restart stops them) → record in decisions (WP-12) | app/src/useUpdates.ts |
| 5 | spec | LOW | untested: install run-once guard on re-render, retry re-installs | app/src/useUpdates.test.ts |
