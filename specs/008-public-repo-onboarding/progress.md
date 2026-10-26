# Progress — 008-public-repo-onboarding

Branch: 008-public-repo-onboarding · Base: df896ca · Started: 2026-10-26
Mode: format=markdown commits=yes autonomy=auto

| WP | Phase | Group | Status | Attempts | Commit | Notes |
|----|-------|-------|--------|----------|--------|-------|
| WP-01 | 1 | 1A | verified | 1 | c328ca8 | tools: git-cliff 2.14.2, gitleaks 8.30.1, shellcheck 0.11.0, lychee 0.24.2 (brew; `npx lychee` doesn't work → call `lychee` directly) |
| WP-02 | 2 | 2A | verified | 1 | ff6cbd5 | + comment paths in 6 code files |
| WP-03 | 3 | 3A | verified | 1 | 3baf382 |  |
| WP-04 | 3 | 3A | verified | 1 | 8f3b63a | Codex/Pi reworded as other agents |
| WP-05 | 3 | 3A | verified | 1 | fd20712 |  |
| WP-06 | 3 | 3A | verified | 1 | 05691c8 | English seed via tr(); seed poll flaky once |
| WP-09 | 3 | 3A | verified | 1 | — | done by orchestrator via gh (account FranPerezFolgado checked) |
| WP-07 | 3 | 3B | verified | 1 | 38b6381 | 121 lines; includes T024 |
| WP-08 | 3 | 3C | verified | 1 | 6f52dad | greps + lychee clean; CLI only from source |
| WP-10 | 4 | 4A | verified | 1 | a5df7ea | user 'no' at prompt exits 0 (contract silent) |
| WP-12 | 4 | 4A | verified | 1 | 4edfd57 | decisions #79 #80 |
| WP-11 | 4 | 4B | verified | 1 | 842a9a7 | not verifiable until next tag; actions pinned by tag like the rest |
| WP-13 | 4 | 4B | verified | 1 | 38b6381 | done in the WP-07 run (same file) |
| WP-14 | 5 | 5A | verified | 1 | e124e91 |  |
| WP-15 | 5 | 5A | verified | 1 | 8614fd9 | SECURITY.md says no checksums yet; WP-11 must update it |
| WP-16 | 5 | 5A | verified | 1 | 3b9554e | CHANGELOG v0.2.1..v1.0.0; release check requires the version's section |
| WP-17 | 5 | 5A | verified | 1 | 302a7a1 | personal notes moved to ignored CLAUDE.local.md |
| WP-18 | 5 | 5A | verified | 1 | — | done by orchestrator via gh: {"enabled":true} |
| WP-19 | 6 | 6A | verified | 1 | d7954db | T041 PASS in preview (headless Playwright, en-US/es-ES, fr→en); native menu = unit tests only; empty defaults output now → English |
| WP-20 | 6 | 6B | verified | 1 | bbc08f3 | decision #81 |
| WP-21 | 7 | 7A | verified | 1 | 8061b28 | gitleaks: 0 findings over all refs; report in ~/.delilah/tasks/LIBRE-15/secret-scan.md |
| WP-22 | 8 | 8A | verified | 1 | 41285a5 | decision #82 |
| WP-19F | 6 | 6F | verified | 1 | a688a90 | lang.rs/state.rs → English fallback; bare AppleLanguages entries; parseSetting in App |
| WP-23 | 8 | 8B | pending | 0 | | |
| WP-24 | 8 | 8C | pending | 0 | | |

## Open findings

| Phase | Reviewer | Severity | Finding | File |
|-------|----------|----------|---------|------|
| 3 | implementer WP-04 | LOW | architecture.md quotes the deny reason and "session stopped" in English, but the daemon may emit them in Spanish | docs/architecture.md |
| 4 | security | MEDIUM | release.yml grants contents/id-token/attestations write to the single job that also runs tauri-action with the signing key; actions pinned by mutable tag. Fix: pin by SHA, split build vs checksum/attest/publish jobs | .github/workflows/release.yml:13-16 |
| 4 | security | LOW | DELILAH_BASE_URL override drops https/TLS flags (http accepted, no warning); add --proto '=https,file' and a warning | scripts/install.sh:11,75 |
| 4 | security | LOW | no --proto-redir '=https', so redirects could downgrade | scripts/install.sh:73 |
| 4 | security | LOW | SHA256SUMS comes from the same origin; docs should say attestation is the provenance check (optionally verify with gh if present) | scripts/install.sh:86-101 |
| 4 | security | LOW | Delilah.app check uses -d (follows symlinks); add `! -L` (matters only with DELILAH_SKIP_CHECKSUM=1) | scripts/install.sh:105-110 |
| 4 | security | LOW | predictable staging names .new-$$/.old-$$ in install dir; use mktemp -d | scripts/install.sh:136,144 |
| 4 | security | LOW | "macOS will warn on first launch" isn't true for the script install (no quarantine) | SECURITY.md:47 |
| 4 | security | LOW | on re-run, gh release download fetches a stale SHA256SUMS into dist and hashes/attests it; rm it after download | .github/workflows/release.yml:113-122 |
| 4 | security | LOW | ci.yml has no permissions block; add contents: read | .github/workflows/ci.yml:3-12 |
| 3 | implementer WP-08 | MEDIUM | the app/DMG/install.sh ship only delilahd; the `delilah` CLI used in the Quickstart must be built from source (documented, not fixed) | README.md, app/src-tauri/tauri.conf.json |
| 4 | spec | MEDIUM | on a re-run the stale SHA256SUMS in the draft is downloaded, listed and attested (contract: covers every *other* asset, not attested) | .github/workflows/release.yml:113-116 |
| 4 | spec | MEDIUM | README one-line install exits 3 until a release with SHA256SUMS exists (v1.0.0 has none): tag 1.1.0 soon after merge | README.md:40 |
| 4 | spec | LOW | any SHA256SUMS fetch failure → exit 3; contract says 404 → 3, download failures → 9 | scripts/install.sh:84 |
| 4 | spec | LOW | mktemp failure exits 1 (usage code) | scripts/install.sh:54 |
| 4 | spec | LOW | README advertises DELILAH_INSTALL_DIR; contract says env vars aren't advertised | README.md:36 |
| 4 | spec | LOW | README build-from-source shows no clone / copy to /Applications (linked to usage.md) | README.md:72-76 |
| 4 | spec | LOW | test-install.sh has no cases for exit 6/7/9; the no-quarantine assertion can't fail | scripts/test-install.sh:69 |
| 2 | spec | LOW | history line sits above the title | docs/redesign/README.md:1 |
| 1 | spec | LOW | tasks/quickstart still say `npx --yes lychee`; lychee is called directly | specs/008-public-repo-onboarding/tasks.md, quickstart.md |
| 6 | architecture | MEDIUM | shell re-implements the rule instead of reading the daemon's resolved language (e.g. GET /api/v1/language); otherwise record the duplication | app/src-tauri/src/daemon.rs:219-246 |
| 6 | architecture | MEDIUM | setLanguage("auto") at import mutates the DOM; could move to main.tsx; double navigator guard | app/src/i18n.ts:26 |
| 6 | architecture | MEDIUM | App stores the raw preference ("fr", 42) in state; Settings then has no matching option. Normalise with parseSetting() | app/src/App.tsx:161-170 |
| 6 | architecture | LOW | contract table mirrored by hand across TS/Rust; a shared JSON fixture would remove drift | app/src/i18n.test.ts, daemon.rs tests |
| 3 | spec | MEDIUM | Quickstart needs a hidden build from source for the CLI (Rust not in Requirements) → in WP-04F | README.md:83-95 |
| 3 | spec | LOW | proposal protocol strings `Nueva tarea`/`Crear` quoted without English label → in WP-04F | docs/*.md |
| 5 | spec | MEDIUM | CLAUDE.md still points to README as "every feature" → in WP-04F | CLAUDE.md:24,26 |
| 7 | spec | MEDIUM | secret-scan report predates 23 later commits (re-run: 650 commits, no leaks) → refresh at the end | ~/.delilah/tasks/LIBRE-15/secret-scan.md |
| 5 | spec | LOW | committed CHANGELOG.md isn't reproducible from cliff.toml (omits 0.2.0); next bump rewrites it | CHANGELOG.md |
| 5 | spec | LOW | CHANGELOG entries keep Spanish labels from old commit subjects (history) | CHANGELOG.md:41,47,53 |
| 5 | spec | LOW | no automated test for the git-cliff guard in bump-version.sh | scripts/bump-version.sh |
| 3 | spec | HIGH | Spanish UI labels left in current docs (Abrir PR, Rama, Restaurar, …) → WP-04F | docs/decisions.md, status.md, architecture.md, usage.md |
| 6 | implementer WP-19F | LOW | AppState::new defaults to Es under cfg!(test) only (≈800 tests assert Spanish), so production (En) and test defaults differ | crates/delilah-daemon/src/state.rs |
| 6 | architecture (re-review) | LOW | first_language duplicated between daemon lang.rs and shell daemon.rs (guarded by mirrored tests) | crates/delilah-daemon/src/lang.rs:52 |
