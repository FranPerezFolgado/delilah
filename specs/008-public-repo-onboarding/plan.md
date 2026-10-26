# Implementation Plan: Public repo onboarding

**Branch**: `008-public-repo-onboarding` | **Date**: 2026-10-26 | **Spec**: [spec.md](spec.md)
**Input**: Feature specification from `specs/008-public-repo-onboarding/spec.md`

## Summary

Make the public repo readable and usable by outsiders. That means a short README for
newcomers, with the full reference moved to `docs/usage.md`; all current docs in English
with English UI labels; LICENSE and the community files; history labelled or archived; and
no personal notes, private links or work names. Code behaviour changes in exactly two
places. First, the UI language falls back to English, in the web UI (`app/src/i18n.ts`,
`app/index.html`) and the native menu (`app/src-tauri/src/daemon.rs`). Second, the release
workflow publishes `SHA256SUMS` and a build-provenance attestation, and a new
`scripts/install.sh` installs the latest release without quarantine and checks it against
those checksums. A full-history secret scan is reported outside the repo. Delivered as one
PR with one commit per block and a minor bump (1.0.0 → 1.1.0). See [research.md](research.md).

## Technical Context

**Language/Version**: Markdown; Bash (macOS `/bin/bash` 3.2 compatible) for `install.sh`;
TypeScript (React, Vite) for the web UI; Rust (stable) for the desktop shell; GitHub Actions YAML.
**Primary Dependencies**: git-cliff (changelog; maintainer-only), gitleaks (one-off scan),
`actions/attest-build-provenance`, Dependabot, Playwright (existing, for the screenshot).
**Storage**: N/A. The `language` preference already lives in the daemon; no schema change.
**Testing**: vitest (`app/src/i18n.test.ts`, new), `cargo test --manifest-path app/src-tauri/Cargo.toml`
(the `english_for` unit tests), `scripts/test-install.sh` (new, runs install.sh against a local
fixture release), `cargo test --workspace` (renamed fixtures), a link check over the docs (lychee
offline mode, or a small script), `shellcheck`.
**Target Platform**: macOS 13+ on Apple Silicon (aarch64). The install script refuses anything else.
**Project Type**: Desktop app (Rust daemon + CLI + Tauri shell + React UI) and its repo.
**Performance Goals**: The install script finishes in under 2 minutes on a normal connection (SC-003).
**Constraints**: No API or DB change. Never touch the installed app (7420): screenshots and
tries go through the e2e battery or `scripts/preview.sh`. No history rewrite or force-push.
Remote repo settings change only after confirming the gh account. Spec Kit paths (`specs/`,
`.specify/`) stay.
**Scale/Scope**: About 15 new files, about 20 edited docs, 6 edited code/config files, and 1
workflow change.

## Constitution Check

`.specify/memory/constitution.md` is still the unfilled template, so the gates are the
project rules in `CLAUDE.md`:

| Gate | Status |
|---|---|
| Version bump on the branch, level justified in the PR | ✅ minor, last commit (R14) |
| `docs/status.md` (and README) updated when a feature lands; decisions logged | ✅ Phase F tasks |
| Try changes in a preview, never on the installed app | ✅ screenshots via the isolated e2e battery; no daemon restarts |
| Never reference work repos; use fixtures | ✅ `plataforma` removed; screenshots from fixtures |
| Agent hooks outside the repo | ✅ not touched |
| Confirm the git/gh account before any remote operation | ✅ R13: `gh auth status` before `gh repo edit` / `gh api` |
| Never edit a shipped migration | ✅ no migration |
| Commits and PR in English; no AI attribution | ✅ |

Post-design re-check: still passes. The design adds no module, API, migration or setting.

## Project Structure

### Documentation (this feature)

```text
specs/008-public-repo-onboarding/
├── spec.md
├── plan.md              # this file
├── research.md          # R1–R14
├── data-model.md        # language resolution, release assets
├── quickstart.md        # how to verify each story
├── contracts/
│   ├── install-script.md    # CLI contract of scripts/install.sh
│   ├── release-assets.md    # what every release publishes
│   └── language.md          # resolution rule shared by UI and shell
└── tasks.md             # /speckit-tasks
```

### Files touched (repository root)

```text
LICENSE                                  # new, MIT
README.md                                # rewritten, < 200 lines
CONTRIBUTING.md  SECURITY.md  CODE_OF_CONDUCT.md  CHANGELOG.md   # new
cliff.toml                               # new, git-cliff config
CLAUDE.md                                # personal bits out, private links replaced
CLAUDE.local.md                          # new, git-ignored (personal rules) — not committed
.gitignore                               # + CLAUDE.local.md
.github/
├── ISSUE_TEMPLATE/{bug_report.yml,feature_request.yml,config.yml}   # new
├── pull_request_template.md             # new
├── dependabot.yml                       # new
└── workflows/release.yml                # + checksums, attestation, permissions
docs/
├── README.md                            # new: index of current docs + archive
├── usage.md                             # new: CLI, settings, Jira, app, sessions, updates, resources
├── images/board-{light,dark}.png        # new: English hero screenshots
├── development.md                       # releasing: changelog, checksums, attestation; install test
├── architecture.md  status.md  decisions.md  vision.md   # English UI terms, links, new decisions
├── redesign/README.md                   # rewritten in English
└── archive/{README.md,spikes/,next-moves.md,ui-redesign.md}   # moved here
specs/README.md                          # new: what specs/ and .specify/ are
scripts/
├── install.sh                           # new
├── test-install.sh                      # new: runs install.sh against a local fake release
├── bump-version.sh                      # + regenerate CHANGELOG.md
└── release/check.mjs                    # + CHANGELOG has the version
app/
├── index.html                           # lang="en"
├── package.json                         # "license": "MIT"
├── src/i18n.ts                          # normalised detect(), lang at import
├── src/i18n.test.ts                     # new
├── src/App.tsx                          # (no change expected: setLanguage now normalises)
├── src-tauri/src/daemon.rs              # english_for() + tests
└── e2e/screens.spec.ts                  # DELILAH_SCREENS_LANG=en, hero filter
crates/delilah-daemon/src/{session_api.rs,store_projects.rs}   # plataforma → platform (tests)
.github/workflows/ci.yml                 # + scripts/test-install.sh and shellcheck on macOS job
```

**Structure Decision**: No new crate or module. Everything lives in the existing layout.
New user-facing docs go to the repo root (where GitHub looks for them) and to `docs/`.

## Design by block

### Block A: Basics (Story 1, FR-001…FR-012)

1. **LICENSE** (MIT, "Copyright (c) 2026 Francisco Pérez Folgado", the maintainer's public
   GitHub name) and `"license": "MIT"` in `app/package.json`. The workspace and shell
   `Cargo.toml` already say MIT.
2. **docs/usage.md**: move README lines 23–565 (Usage, Desktop app, Project settings, Jira,
   sessions, Resource usage, Installing on another Mac → "Building from source", Updates in
   English) with the substance unchanged. UI labels use their English text from
   `app/src/i18n/en*.ts` (e.g. "Te espera" → its `EN` value, "Bandeja" → "Inbox"; the
   dictionary is the source of truth, never a guess). Add a short table of contents.
3. **README.md** (< 200 lines), in this order: title + pitch ("A lightweight macOS app that
   runs Claude Code agents in parallel, each in its own git worktree, and pauses and resumes
   them around usage limits."), badges (CI workflow, MIT, latest release, `platform-macOS
   (Apple Silicon)`), hero `<picture>` (light/dark), **Why** (isolated workspaces;
   auto pause/resume on usage limit; inbox of what needs you; lightweight, ~195 MB with 12
   agents → `docs/usage.md#resource-usage`), **Requirements**, **Install** (script, manual
   with the two Gatekeeper routes, verify, from source; see the install-script contract),
   **Quickstart** (≤ 5 commands), **Documentation** (links to `docs/README.md` entries),
   **Contributing**, **Security**, **License**, the "not affiliated with Anthropic" note.
4. **Stale and Spanish content**: delete "Status: MVP-0", "private/deploy key" and the
   Codex/Pi claims. The "Actualizaciones" section becomes "Updates" in usage.md. Sweep the
   current docs (`docs/*.md` outside archive, README, CLAUDE.md, CONTRIBUTING, SECURITY) for
   Spanish prose and UI labels. `docs/adr/` is current: ADR-001 keeps its content, with
   the quoted Spanish UI label replaced by its English one. Code identifiers and quoted Spanish source strings in
   architecture notes become their English labels where they name a UI element.
5. **Links**: replace the claude.ai links (R11) and update every link into the moved README
   sections and archived docs. Check with a link checker over `README.md` and `docs/**/*.md`,
   excluding `docs/archive`, `docs/redesign` and `specs`.
6. **Names and personal notes**: `plataforma` → `platform` (R12) wherever `git grep`
   finds it outside `specs/` (today: `session_api.rs`, `store_projects.rs`, README); run
   `cargo test --workspace`. Move the personal rules to `CLAUDE.local.md` (R9), and reword "work
   Mac" in `docs/decisions.md` #24 and `docs/status.md`.
7. **History** (R10): `git mv` spikes, next-moves and ui-redesign to `docs/archive/`, then
   add `docs/archive/README.md`, `specs/README.md`, `docs/README.md`, and the English
   `docs/redesign/README.md`.
8. **Screenshot** (R8): extend `screens.spec.ts`, then run it once to produce
   `docs/images/board-{light,dark}.png`.
9. **Secret scan** (R6, FR-029): gitleaks over the full history. The redacted, triaged
   report goes to `~/.delilah/tasks/LIBRE-15/secret-scan.md`, which is not committed. If there
   is a real finding, stop and report; never rewrite.
10. **Repo metadata** (FR-003, R13): after `gh auth status` confirms FranPerezFolgado,
    `gh repo edit` the description and topics, and enable private vulnerability reporting.

### Block B: Community files (Story 3, FR-013…FR-018)

CONTRIBUTING, SECURITY, CODE_OF_CONDUCT, the issue forms and PR template, and dependabot,
as in R13. CHANGELOG (R5): add `cliff.toml`, generate `CHANGELOG.md` over all tags, extend
`scripts/bump-version.sh` (regenerate it with `--tag v$new`, add it to the commit; fail
early with an install hint if `git cliff` is missing), and make `scripts/release/check.mjs`
require a `## [X.Y.Z]` section. Its failure message says to run `scripts/bump-version.sh`
(or `git cliff --tag vX.Y.Z -o CHANGELOG.md`) on a branch. `check.mjs` also runs locally in
`make release` before the tag is pushed, so a manual bump fails there, not late in CI. Document both in `docs/development.md#releasing`.

### Block C: English fallback (Story 4, FR-019…FR-022)

Follows [contracts/language.md](contracts/language.md) and R7:
- `i18n.ts`: `detect(setting: unknown)` returns `"es"`/`"en"` for those exact values, and
  otherwise `systemLang()` (`es*` → `es`, else `en`; no `navigator` → `en`). `setLanguage`
  accepts `unknown`, and the module calls it at import so `document.documentElement.lang` is
  right from the first paint. Keep the `LangSetting` type for the Settings control.
- `index.html`: `lang="en"`.
- `daemon.rs`: `pub fn english_for(pref: Option<&str>, apple_languages: &str) -> bool`.
  Explicit `"en"` → true, explicit `"es"` → false; otherwise parse the first quoted entry of
  `AppleLanguages`, and it is Spanish only if that entry starts with `es`. Empty or unparsable
  → English. `english()` calls it.
- Tests first: `app/src/i18n.test.ts` (6 system languages × auto/unset/unknown/explicit) and
  Rust unit tests for `english_for`. Both use the **same** table, copied row for row and with a
  comment in each pointing to the other and to contracts/language.md, so the two can't drift.
- Already true today: `i18n.ts` runs `detect("auto")` at import. What changes is the `es` → `en`
  fallback, normalising unknown values, and writing `documentElement.lang` at import.
- Try it in a preview (`scripts/preview.sh`) with the setting on Auto. The native menu needs
  the Tauri app; say so instead of trying it (CLAUDE.md).

### Block D: Distribution and trust (Story 2, FR-023…FR-028)

- `release.yml`: add `id-token: write` and `attestations: write` to the job. In the same job,
  right after "Build and upload latest.json", add a step "Checksums":
  `rm -rf "$RUNNER_TEMP/dist"`, then `gh release download "$TAG" -D "$RUNNER_TEMP/dist"`,
  and fail unless `Delilah_aarch64.app.tar.gz` and `latest.json` are among the files. Then
  `(cd "$RUNNER_TEMP/dist" && shasum -a 256 * | sort -k2 > "$RUNNER_TEMP/SHA256SUMS")`
  (written outside `dist/`) and `gh release upload "$TAG" "$RUNNER_TEMP/SHA256SUMS" --clobber`.
  Then `actions/attest-build-provenance@v3` (pinned by major, like the other actions) with
  `subject-path: '${{ runner.temp }}/dist/*'`, which attests the assets and not SHA256SUMS.
  "Publish release" stays last, so any failure leaves only a draft. See
  [contracts/release-assets.md](contracts/release-assets.md).
- `scripts/install.sh` per [contracts/install-script.md](contracts/install-script.md);
  `scripts/test-install.sh` builds a fake release dir (a dummy `Delilah.app` in a
  `.app.tar.gz` plus `SHA256SUMS`, served over `file://` through `DELILAH_BASE_URL`) and
  checks: a fresh install; a reinstall; a bad checksum leaving the old app intact; a missing
  `SHA256SUMS` stopping the script; a non-arm64 check via `DELILAH_ARCH` override; and no
  quarantine attribute on the result. Add it, plus `shellcheck`, to the macOS (Rust workspace) job in `ci.yml` only, since it
  needs `ditto`, `xattr` and `shasum`.
- README install section and `docs/development.md` (verify commands). Add a decision for
  Homebrew (R1) and one for unsigned distribution with checksums and attestation.
- Verification of the workflow itself happens on the first tagged release after merge
  (SC-007). Note that in `docs/status.md` as pending until then.

### Block E: Wrap-up (FR-030, FR-031)

`docs/status.md` (what is built, what is pending: first release with attestation,
private reporting enabled), `docs/decisions.md` (new entries: public repo docs layout and
archive, English fallback, unsigned distribution with install script + checksums +
attestation, no Homebrew tap for now, changelog via git-cliff in the bump), and
`docs/architecture.md` (language resolution, release assets). Run every check, then
`scripts/bump-version.sh minor --commit` (1.0.0 → 1.1.0, CHANGELOG regenerated).

## Order and parallelism

A1–A7 (docs) and C (code) share no files, so they can run in parallel, and so can D's
workflow and script work. B needs A's README and docs layout for its links, and B's
`bump-version.sh` change must land before E. The screenshot (A8) needs only the e2e battery.
The secret scan (A9) and the repo metadata (A10) are independent and can run any time.
The README is written last within A, once usage.md, the screenshots and the install section
exist.

## Risks

- **A changelog section missing from the next release**: guarded by `check.mjs`.
- **A tauri-action asset rename breaks the install script**: the script uses the same
  stable name the updater manifest already depends on (`Delilah_aarch64.app.tar.gz`), so
  both would break together and `manifest.mjs` would catch it.
- **Attestation step permissions**: if `attest-build-provenance` fails, the release stays a
  draft (publishing is the last step). Nothing half-published.
- **Translating UI labels by guess**: always take them from `app/src/i18n/en*.ts`.

## Plan review

`architecture-reviewer` found no CRITICAL issues. Its HIGH findings were fixed above:
- The release step order and paths: `SHA256SUMS` is written outside the attested dir, the
  draft's assets are checked, and the attest action is pinned.
- The rename covers every `git grep` hit, with the full workspace tests. Its claim that
  `delilah-integrations/src/jira.rs` holds the name was checked and is false.

MEDIUM findings, also applied:
- ADR-001 is in scope of the English sweep.
- `check.mjs` has an actionable message and also runs locally.
- The install script is trimmed (contract): `--version` and `DELILAH_REPO` are dropped, and
  `DELILAH_SKIP_CHECKSUM` stays only as the deliberate override the spec allows, with a warning.
- The language test table is the same in both test files.

LOW findings, applied:
- `ci.yml` runs the install test on the macOS job only.
- Dependabot groups `tauri*` crates and `@tauri-apps/*` npm packages.
- The link check doesn't exclude `docs/redesign/`.

`docs/sessions/` is git-excluded locally (not tracked), so it is out of scope.

## Complexity Tracking

No gate violations.
