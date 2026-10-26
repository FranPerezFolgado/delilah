# Research: Public repo onboarding

Each item: **Decision**, **Rationale**, **Alternatives considered**.

## R1. Homebrew for an unsigned app

- **Decision**: No Homebrew distribution in this feature. Record a decision (FR-028): the
  official tap is out, and an own tap stays a possible follow-up, not built now.
- **Rationale**: Homebrew deprecated `--no-quarantine`, removes it in Homebrew 6, and ends
  support for casks that fail Gatekeeper on 2026-09-01 (Homebrew/brew#20755). The only way left
  is an own tap whose cask clears the quarantine flag in a `postflight` step. That means a
  second repo to maintain, a cask to bump on every release (or a workflow that does it), and
  a cask that quietly bypasses Gatekeeper, which is the same trust trade as `install.sh`
  with more moving parts. The install script already gives a one-line install.
- **Alternatives**: own tap with `postflight` xattr (deferred: extra repo and release step,
  and Homebrew may restrict it further); official homebrew-cask (not accepted for unsigned apps).
- **Sources**: https://github.com/Homebrew/brew/issues/20755,
  https://workbrew.com/blog/homebrew-5-0-0

## R2. How the install script avoids quarantine

- **Decision**: `curl` downloads the release's `Delilah_aarch64.app.tar.gz` (the stable name
  the updater already uses, see `scripts/release/manifest.mjs`) and `SHA256SUMS` from
  `https://github.com/<repo>/releases/latest/download/`. It checks the hash, extracts with
  `tar` into a temp dir, and moves the app into `/Applications`.
- **Rationale**: Only apps that go through LaunchServices-aware downloaders (browsers, Mail,
  AirDrop) get `com.apple.quarantine`. `curl` and `tar` don't set it, so Gatekeeper never
  checks the app, the same way the Tauri updater's downloads aren't checked. The `.app.tar.gz`
  avoids mounting a DMG (`hdiutil`) and has a version-free name.
- **Alternatives**: the DMG plus `hdiutil attach` (slower, versioned name, mount
  clean-up); downloading and then running `xattr -dr` (pointless, since `curl` sets no flag).

## R3. Checksums and provenance in the release workflow

- **Decision**: After tauri-action uploads the draft and `latest.json` is in place, a new step
  downloads the draft's assets (`gh release download "$TAG" --dir dist`), writes
  `SHA256SUMS` with `shasum -a 256` (every asset except itself), uploads it, and runs
  `actions/attest-build-provenance` (current major) with `subject-path: dist/*`. Only after that
  does "Publish release" run. The job gains `id-token: write` and `attestations: write`.
- **Rationale**: Hashing the uploaded files, not the local build outputs, guarantees the
  names and bytes in `SHA256SUMS` match what users download. tauri-action renames assets when
  it uploads them. Attesting before publishing keeps the rule that a release goes live only
  once every step has passed. Attestations are free for public repos. Users check with
  `gh attestation verify <file> --repo FranPerezFolgado/delilah`.
- **Alternatives**: hashing `app/src-tauri/target/.../bundle` (names don't match the
  assets); signing with minisign or GPG (another key to manage, and fewer users can verify it).

## R4. Ad-hoc signing and updater signature

- **Decision**: No change. The bundle keeps the signature it gets today (Apple Silicon
  binaries are always at least ad-hoc signed by the linker and the bundler), and the updater
  `.sig` stays as is.
- **Rationale**: FR-027. The notes ask to keep it, and nothing in this feature needs a change.

## R5. Changelog generation

- **Decision**: git-cliff with a `cliff.toml` at the root (Conventional Commits groups;
  commits that don't parse are skipped; merge commits and `chore: bump version` are
  skipped). `CHANGELOG.md` is generated once now over every tag. From then on
  `scripts/bump-version.sh` regenerates it with `git cliff --tag vX.Y.Z -o CHANGELOG.md` and
  adds it to the bump commit. It fails with an install hint (`brew install git-cliff`) if
  git-cliff is missing. `scripts/release/check.mjs` fails a release whose version has no
  `## [X.Y.Z]` section in `CHANGELOG.md`.
- **Rationale**: The bump commit is already the last step on a feature branch, so the
  changelog goes through review with the version. The release check makes sure every
  published release is covered (FR-016). git-cliff isn't installed locally today; only
  whoever bumps the version needs it.
- **Alternatives**: generating it in the release workflow (it can't commit to `main`
  without extra permissions or a bot); release-please (another workflow and PR flow on top
  of the existing `make release`).

## R6. Secret scan

- **Decision**: gitleaks (`brew install gitleaks`, or its Docker image), run as
  `gitleaks git --redact -v --report-format json` over the full history (`--log-opts=--all`).
  The redacted report goes outside the repo, to
  `~/.delilah/tasks/LIBRE-15/secret-scan.md`, with each finding triaged as real or false
  positive. Nothing is committed and no history is rewritten.
- **Rationale**: gitleaks is a single binary, fast, and scans every commit. Redacting and
  keeping the report out of the public repo avoids republishing a real secret. trufflehog
  can be added as a second pass (`trufflehog git file://. --only-verified`) if gitleaks finds
  anything, since it can confirm whether a credential is live.
- **Alternatives**: trufflehog alone (heavier; its verification calls outside services);
  GitHub secret scanning (already on for public repos, but it only covers known provider
  patterns and doesn't produce a report in the task).

## R7. Language fallback: where Spanish is the default today

Findings:
1. `app/src/i18n.ts` `detect()`: `navigator` missing → `"es"`; an explicit setting is
   returned as is, so a stored unknown value (`"fr"`) becomes `current = "fr"` and `t()`
   renders Spanish (it only translates when `current === "en"`).
2. `app/src/App.tsx:166-171` passes whatever the daemon stored straight to `setLanguage`.
3. `app/index.html:2` declares `lang="es"`. `i18n.ts` sets `current` at import, but only
   `setLanguage` writes `document.documentElement.lang`, so the page says `es` until the
   preference arrives.
4. `app/src-tauri/src/daemon.rs:219-231` `english()`: when `AppleLanguages` can't be read,
   `unwrap_or("es")` gives Spanish menus.

- **Decision**: (a) `detect()` normalises: `"es"`/`"en"` are explicit; anything else
  (including `"auto"`, `null` and unknown values) follows the system; the system counts as
  Spanish only if its tag starts with `es` (case-insensitive); everything else, including
  no system language, is English. (b) `i18n.ts` applies the detected language to
  `document.documentElement.lang` at import. (c) `index.html` declares `lang="en"`.
  (d) In the shell, extract a pure `english_for(pref: Option<&str>, apple_languages: &str) -> bool`
  with the same rule, unit-tested, and keep `english()` as the I/O wrapper.
- **Rationale**: A single rule, applied in both places, covers FR-019 to FR-022 with no new
  setting and no API change. The `LangSetting` type stays `"auto" | "es" | "en"`.
- **Alternatives**: making English the default even for Spanish systems (rejected: the
  spec keeps Spanish for Spanish systems); a daemon-side normalisation of the stored value
  (it would change the API contract for no gain).

## R8. README screenshot

- **Decision**: Regenerate a hero screenshot with the UI in English and keep it in
  `docs/images/` (board, dark and light, 1440×900, under 300 KB each). Add an English option
  to the existing Playwright battery `app/e2e/screens.spec.ts` (`DELILAH_SCREENS_LANG=en`,
  which sets the `language` preference to `en` before capturing) and a filter for the
  hero screens. The README shows the light/dark pair with a `<picture>` element.
- **Rationale**: Every capture in `docs/redesign/despues` is in Spanish (its README says
  "UI en español"), and the spec forbids Spanish UI labels in current docs (SC-004). The
  battery already runs against an isolated daemon with fixtures, so no personal data shows.
- **Alternatives**: reusing `despues/Tablero-*.png` (Spanish UI); a GIF (heavier, and it
  needs a recording tool in CI or by hand; could come later).

## R9. Where personal notes go

- **Decision**: Move the maintainer-specific rules out of `CLAUDE.md` into a git-ignored
  `CLAUDE.local.md`, which Claude Code also loads: the two Macs, the personal gh account and
  the work credentials. `CLAUDE.md` keeps the generic versions ("never copy code or data from
  private repos; use the fixtures"; "if several GitHub accounts are configured, confirm the
  one gh uses before any remote operation"). Add `CLAUDE.local.md` to `.gitignore`. In
  current docs (`docs/decisions.md` #24, `docs/status.md`), reword "the work Mac" as "a real
  project".
- **Rationale**: The maintainer keeps their agent guard rails; outsiders see only the rules
  that apply to them.
- **Alternatives**: deleting the notes (the maintainer loses guard rails that prevent
  pushes from the wrong account).

## R10. Historical material

- **Decision**: `specs/` and `.specify/` stay in place (Spec Kit needs them), with a
  `specs/README.md` that explains them in one paragraph. `docs/redesign/` stays (its README
  is rewritten in English, with a line saying it is the v2 redesign record).
  `docs/spikes/`, `docs/next-moves.md` and `docs/ui-redesign.md` move to `docs/archive/`
  with a `docs/archive/README.md`, and every link to them is updated. A `docs/README.md`
  index lists the current docs and points to the archive.
- **Rationale**: Moving `specs/` would break Spec Kit and Delilah's own workflows. The rest
  is history that would otherwise read as current.
- **Alternatives**: deleting history (loses the record the decisions log links to).

## R11. Private claude.ai links

- **Decision**: In current docs and `CLAUDE.md`, the design-doc link becomes
  `docs/vision.md`, and the UI mock-up and board links become `docs/redesign/` (screenshots)
  plus `docs/archive/ui-redesign.md`. Links inside `specs/` stay, since `specs/` is history.
- **Rationale**: FR-009. The repo already holds the content those links stood for.

## R12. Generic project name

- **Decision**: `plataforma` becomes `platform` in the README and docs examples
  (`~/work/platform`, `--project platform`) and in the test fixtures of
  `crates/delilah-daemon/src/session_api.rs` and `store_projects.rs`. `specs/004-project-config`
  stays as history.
- **Rationale**: FR-011. A plain English word with the same shape keeps the tests readable.

## R13. Community files and repo settings

- **Decision**:
  - CONTRIBUTING.md: short, and links to `docs/development.md` for detail. It covers
    prerequisites, `scripts/make-fixture-parent.sh`, `scripts/preview.sh`, the check table
    from CLAUDE.md "Stack & checks", Conventional Commits, the versioning policy (semver levels as in
    CLAUDE.md), and the PR flow (one PR per change, bump on the branch).
  - SECURITY.md: GitHub private vulnerability reporting (to be enabled under Settings ›
    Security), with a response target of about 7 days. Supported versions: the latest
    release only. The threat model is based on `crates/delilah-daemon/src/config.rs:6`
    (`127.0.0.1:7420`), `api.rs:23-38` (CORS allow-list of the app's origins), and
    `api.rs:237-267` (`Origin` check on writes and WebSockets, `Host` must be loopback against
    DNS rebinding). It also covers agents running as the user with `--settings` hooks, the Jira
    token in the Keychain, and what is out of scope (other local processes running as the
    same user). It describes only what exists; the pre-announcement audit is a separate task.
  - CODE_OF_CONDUCT.md: Contributor Covenant 2.1, with reports going through GitHub
    (private vulnerability reporting isn't for conduct, so the contact is the maintainer's
    GitHub profile; no email is published).
  - `.github/ISSUE_TEMPLATE/bug_report.yml` and `feature_request.yml` (issue forms),
    `config.yml` (security reports to SECURITY.md), `.github/pull_request_template.md`.
  - `.github/dependabot.yml`: `cargo` at `/` and `/app/src-tauri`, `npm` at `/app`,
    `github-actions` at `/`; weekly; each ecosystem's updates grouped; at most 5 open PRs.
  - Remote changes, made only after `gh auth status` confirms the account
    (FranPerezFolgado): `gh repo edit --description "…" --add-topic …`, and turning on
    private vulnerability reporting
    (`gh api -X PUT repos/{owner}/{repo}/private-vulnerability-reporting`).
- **Rationale**: These are the files GitHub's community profile checks (SC-008), and each
  stays small by linking to existing docs.

## R14. Version level

- **Decision**: one `minor` bump (1.0.0 → 1.1.0), done last with
  `scripts/bump-version.sh minor --commit`, which now also regenerates CHANGELOG.md.
- **Rationale**: The English fallback changes UI behaviour, and releases gain new assets.
  Both are backwards-compatible features. Everything else is patch-level (FR-031).
