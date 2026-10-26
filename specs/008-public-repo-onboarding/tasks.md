# Tasks: Public repo onboarding

**Input**: Design documents from `specs/008-public-repo-onboarding/`
**Prerequisites**: [plan.md](plan.md), [spec.md](spec.md), [research.md](research.md), [data-model.md](data-model.md), [contracts/](contracts/), [quickstart.md](quickstart.md)

**Tests**: Written first only where code changes: the language rule (US4) and the install
script (US2). Docs work is checked with the greps and the link check in [quickstart.md](quickstart.md).

**Format**: `- [ ] [ID] [P?] [Story] Description with file path`. `[P]` = can run in
parallel (different files, no dependency on an unfinished task).

**Ground rules for every task**:
- Never touch the installed app (port 7420, `~/.delilah` daemon). Use `scripts/preview.sh` or the isolated e2e battery.
- English UI labels come from `app/src/i18n/en.ts` and `app/src/i18n/en/*.ts`, never guessed.
- Commits are Conventional Commits in English, with no AI attribution.
- No history rewrite, no force-push, and no remote change without confirming the gh account first.

---

## Phase 1: Setup

- [X] T001 Check the tools needed later and note any missing one in `specs/008-public-repo-onboarding/progress.md`: `git cliff --version`, `gitleaks version`, `shellcheck --version`, `npx --yes lychee --version`. Install them with `brew install git-cliff gitleaks shellcheck`; lychee runs through npx.
- [X] T002 [P] Add `CLAUDE.local.md` to `.gitignore` (root), next to the existing `.claude/settings.local.json` entry.

---

## Phase 2: Foundational (docs layout that US1 and US3 link into)

- [X] T003 Move the historical docs with `git mv`: `docs/spikes/` → `docs/archive/spikes/`, `docs/next-moves.md` → `docs/archive/next-moves.md`, `docs/ui-redesign.md` → `docs/archive/ui-redesign.md`.
- [X] T004 Update every link to the moved files (find them with `git grep -nE 'spikes/|next-moves|ui-redesign'` outside `specs/`) in `CLAUDE.md`, `docs/decisions.md`, `docs/status.md`, `docs/architecture.md`, `docs/development.md` and `docs/redesign/README.md`. Fix relative paths inside the moved files too.
- [X] T005 [P] Create `docs/archive/README.md`: one paragraph saying these are records of past spikes, the old roadmap and the v2 redesign, kept for history and not maintained, and listing each entry in one line.
- [X] T006 [P] Create `specs/README.md`: one paragraph explaining that `specs/` holds the Spec Kit spec, plan and tasks of each feature as it was built, and `.specify/` holds Spec Kit's config and templates. Both are a historical record (and in Spanish in places); current behaviour is in `docs/`.
- [X] T007 [P] Rewrite `docs/redesign/README.md` in English with the same content: what `antes/` and `despues/` hold, how to regenerate them with `app/e2e/screens.spec.ts`, and the data used. Add a first line saying it records the v2 redesign and that its screenshots show the UI in Spanish.

**Checkpoint**: `git grep -nE '(spikes|next-moves|ui-redesign)' -- . ':!specs' ':!docs/archive'` shows only paths that exist.

---

## Phase 3: User Story 1 - A newcomer understands and installs Delilah (P1) 🎯 MVP

**Goal**: A short README for newcomers, the full reference in `docs/usage.md`, every current doc in English with no stale claims, private links or work names, and correct repo metadata.

**Independent test**: The Story 1 block of [quickstart.md](quickstart.md): README under 200 lines, zero matches for the forbidden-terms grep, zero broken links, and the repo description and topics set.

- [X] T008 [US1] Create `docs/usage.md` from README.md lines 23–565 (Usage, Desktop app, project settings, Jira, sessions, Resource usage, "Installing on another Mac" → "Building from source", and "Actualizaciones" → "Updates", translated). Keep the substance. Replace each Spanish UI label with its English value from `app/src/i18n/en*.ts` (e.g. "Te espera" → "Waiting for you", "Bandeja" → "Inbox", "Ajustes" → "Settings", "Detener el daemon" → its EN value). Add a table of contents. Drop "Status: MVP-0", the "repo is private / deploy key" comment and every Codex/Pi mention.
- [ ] T009 [P] [US1] Extend `app/e2e/screens.spec.ts` with `DELILAH_SCREENS_LANG=en` (set the `language` preference to `"en"` through the API before capturing) and an `DELILAH_SCREENS_ONLY` filter on screen names. Then run it as `docs/redesign/README.md` says, with `DELILAH_SCREENS=../docs/images` (or the spec's output-dir variable), `DELILAH_SCREENS_LANG=en` and `DELILAH_SCREENS_ONLY=Tablero`. Save the results as `docs/images/board-light.png` and `docs/images/board-dark.png` (1440×900, under 300 KB each; compress with `sips`/`pngquant` if needed). Never against 7420.
- [X] T010 [P] [US1] Rename the work project name `plataforma` → `platform` in the test fixtures of `crates/delilah-daemon/src/session_api.rs` and `crates/delilah-daemon/src/store_projects.rs`, and in any other `git grep -n plataforma -- . ':!specs'` hit except README.md (rewritten in T012). Run `cargo test --workspace`.
- [X] T011 [P] [US1] Translate the remaining Spanish prose and UI labels to English (labels from `app/src/i18n/en*.ts`) in `docs/architecture.md`, `docs/decisions.md`, `docs/development.md`, `docs/status.md`, `docs/vision.md` and `docs/adr/ADR-001-chat-mode-allow-always-allowlist.md`. Quoted UI strings become their English label, and code identifiers stay. In `docs/vision.md` replace the claude.ai design-doc link with a line saying this file is the public summary of the original design. In `docs/decisions.md` #24 and `docs/status.md`, reword "the work Mac" as "a real project".
- [ ] T012 [US1] Rewrite `README.md` (< 200 lines) per plan Block A step 3:
  - title and one-line pitch;
  - badges: CI (`.github/workflows/ci.yml`), license MIT, latest release, "macOS (Apple Silicon)";
  - a `<picture>` with `docs/images/board-dark.png` / `board-light.png`;
  - **Why**: isolated git-worktree workspaces; auto pause/resume on usage limits; the Inbox; lightweight (~195 MB with 12 agents) linking `docs/usage.md#resource-usage`;
  - **Requirements**: macOS 13+ on Apple Silicon, Claude Code installed and logged in, Claude Code as the only agent today;
  - **Install**: a placeholder heading with the one-line script command; T024 completes it;
  - **Quickstart** in at most 5 commands: install, open the app or `delilah daemon`, `delilah project add <path>`, `delilah run <KEY> --project <name>`, open the app;
  - **Documentation**: links to `docs/README.md`, `docs/usage.md`, architecture, decisions, development, status;
  - **Contributing** → CONTRIBUTING.md; **Security** → SECURITY.md; **License** MIT → LICENSE;
  - the note "Delilah is an independent project, not affiliated with or endorsed by Anthropic. Claude and Claude Code are trademarks of Anthropic."

  Use `platform` in examples and no claude.ai links.
- [X] T013 [P] [US1] Create `docs/README.md`: an index of the current docs (usage, architecture, decisions, development, status, vision, adr/), one line each, plus "History" pointing to `docs/archive/`, `docs/redesign/` and `specs/`.
- [ ] T014 [US1] Run the Story 1 checks from [quickstart.md](quickstart.md):
  - the forbidden-terms grep over everything except `specs/`, `.specify/`, `docs/archive/` and `docs/redesign/` screenshots;
  - the Spanish-labels grep;
  - `npx --yes lychee --offline` over `README.md` and `docs/**/*.md`, excluding only `docs/archive` (not `docs/redesign`).

  Fix every hit and broken link, including anchors into `docs/usage.md`.
- [X] T015 [US1] Repo metadata (remote):
  1. Run `gh auth status` and continue only if the active account for github.com is `FranPerezFolgado`; otherwise stop and record it in `progress.md`.
  2. `gh repo edit FranPerezFolgado/delilah --description "Lightweight macOS app that runs Claude Code agents in parallel, each in its own git worktree, and pauses/resumes them around usage limits"`.
  3. Add the topics with `--add-topic` for each of claude-code, ai-agents, tauri, rust, macos, orchestrator and git-worktree.
  4. Check with `gh repo view --json description,repositoryTopics`.

**Checkpoint**: Story 1 passes on its own.

---

## Phase 4: User Story 2 - A user installs an unsigned build without fighting macOS (P1)

**Goal**: `scripts/install.sh` per [contracts/install-script.md](contracts/install-script.md); `SHA256SUMS` and provenance attestation in every release per [contracts/release-assets.md](contracts/release-assets.md); README install section.

**Independent test**: `shellcheck` and `scripts/test-install.sh` pass. The `release.yml` diff matches the contract. After the next tag, the Story 2 block of quickstart.md passes.

### Tests first

- [ ] T016 [US2] Write `scripts/test-install.sh` (bash, `set -euo pipefail`, temp dirs only, cleaned with `trap`). It builds a fake release dir: a dummy `Delilah.app` (with `Contents/Info.plist` `CFBundleShortVersionString=9.9.9`) packed as `Delilah_aarch64.app.tar.gz`, plus `SHA256SUMS`. It runs `scripts/install.sh --yes` with `DELILAH_BASE_URL=file://<dir>`, `DELILAH_INSTALL_DIR=<tmp>` and `DELILAH_ARCH` set, and asserts:
  - (a) a fresh install creates the app and exits 0 with no `com.apple.quarantine` attribute;
  - (b) a reinstall replaces it, exit 0;
  - (c) a tampered archive → exit 4, and the previous app is untouched;
  - (d) a missing `SHA256SUMS` → exit 3;
  - (e) `DELILAH_SKIP_CHECKSUM=1` with no `SHA256SUMS` installs and prints the warning;
  - (f) `DELILAH_ARCH=x86_64` → exit 2;
  - (g) an archive without `Delilah.app` → exit 5;
  - (h) an unwritable install dir → exit 8.

  It must fail now, since `install.sh` doesn't exist yet.

### Implementation

- [ ] T017 [US2] Write `scripts/install.sh` per [contracts/install-script.md](contracts/install-script.md): bash 3.2-compatible, `set -euo pipefail`, `--yes`/`-h`, the environment variables in the contract, the exit codes 0–9, `mktemp -d` + `trap`, `curl -fL --retry 2`, `shasum -a 256` against the archive's line, `tar -xzf`, the `pgrep -x delilah-app` check, moving the old app aside and `ditto`, restoring it on failure, `xattr -dr com.apple.quarantine` (errors ignored), and printing the installed version. No `sudo`. Make it executable. Then `scripts/test-install.sh` and `shellcheck scripts/install.sh scripts/test-install.sh` must pass.
- [ ] T018 [P] [US2] Add to `.github/workflows/ci.yml`, in the macOS "Rust workspace" job only, a step that runs `shellcheck scripts/install.sh scripts/test-install.sh` (`brew install shellcheck` if the runner lacks it) and then `scripts/test-install.sh`.
- [ ] T019 [P] [US2] In `.github/workflows/release.yml`:
  1. Add `id-token: write` and `attestations: write` to `permissions`.
  2. After "Build and upload latest.json", add a "Checksums" step:
     - `rm -rf "$RUNNER_TEMP/dist"`, then `gh release download "$GITHUB_REF_NAME" -D "$RUNNER_TEMP/dist"`;
     - fail unless `Delilah_aarch64.app.tar.gz` and `latest.json` are present;
     - `(cd "$RUNNER_TEMP/dist" && shasum -a 256 * | sort -k2) > "$RUNNER_TEMP/SHA256SUMS"`;
     - `gh release upload "$GITHUB_REF_NAME" "$RUNNER_TEMP/SHA256SUMS" --clobber`.
  3. Add a step `uses: actions/attest-build-provenance@v3` with `subject-path: '${{ runner.temp }}/dist/*'`.
  4. Keep "Publish release" as the last step.

  Update the header comment to list the new steps.
- [ ] T020 [P] [US2] Update `scripts/release.sh` (the dry run) to also write a local `SHA256SUMS` for the bundle outputs and print it, so the rehearsal shows what the release will publish (no upload, no attestation).
- [ ] T021 [P] [US2] Add to `docs/development.md` "Releasing": what each release now publishes (`SHA256SUMS`, attestation), how to verify (`shasum -a 256 -c SHA256SUMS --ignore-missing`, `gh attestation verify Delilah_aarch64.app.tar.gz --repo FranPerezFolgado/delilah`), and how to test the install script (`scripts/test-install.sh`).
- [ ] T022 [P] [US2] Add decisions to `docs/decisions.md` (next numbers after #78):
  - **Unsigned distribution**: no Apple Developer account; `install.sh` via curl, which sets no quarantine; checksums and build-provenance attestation for trust; ad-hoc signature kept. *Ruled out*: notarization (cost).
  - **No Homebrew tap for now** (research R1): Homebrew removes `--no-quarantine` and ends support for casks that fail Gatekeeper on 2026-09-01. An own tap with a `postflight` xattr is possible but adds a repo and a release step for the same trust trade as `install.sh`. Cite Homebrew/brew#20755.
- [ ] T023 [US2] Add the uninstall steps to `docs/usage.md` ("Uninstall": stop the daemon from the app menu, delete the app, `~/Library/LaunchAgents/dev.delilah.daemon.plist`, optionally `~/.delilah`). Write them in English from the old README text.
- [ ] T024 [US2] Write the README "Install" section (`README.md`):
  1. The script: `curl -fsSL https://raw.githubusercontent.com/FranPerezFolgado/delilah/main/scripts/install.sh | bash`, with one line on what it does and that it verifies the checksum.
  2. "Why unsigned": no paid Apple Developer account, so macOS blocks a downloaded copy.
  3. Manual download of the DMG, then either `xattr -dr com.apple.quarantine /Applications/Delilah.app` or System Settings › Privacy & Security › Open Anyway (after the first blocked launch); right-click › Open no longer works since macOS Sequoia.
  4. Verify the download: `shasum` and `gh attestation verify`.
  5. Build from source: Xcode CLT, Rust, Node 22+, `cd app && npm install && npm run dist`, details in `docs/usage.md#building-from-source`.
  6. "Updates install in-app and aren't quarantined."

  Keep README under 200 lines.

**Checkpoint**: Story 2 passes locally (`scripts/test-install.sh`). End-to-end checking waits for the first tag (recorded in status, T045).

---

## Phase 5: User Story 3 - An outside contributor can build, test and send a change (P2)

**Goal**: LICENSE and the community files, the changelog pipeline, dependabot, a CLAUDE.md with no personal notes, and private vulnerability reporting enabled.

**Independent test**: The Story 3 block of [quickstart.md](quickstart.md). After merge, the community profile is at 100%.

- [ ] T025 [P] [US3] Create `LICENSE` (standard MIT text, "Copyright (c) 2026 Francisco Pérez Folgado") and add `"license": "MIT"` to `app/package.json` (after `"version"`). The workspace `Cargo.toml` and `app/src-tauri/Cargo.toml` already declare MIT; confirm that.
- [ ] T026 [P] [US3] Create `CONTRIBUTING.md`, short and linking to `docs/development.md` for detail. Cover:
  - prerequisites: macOS on Apple Silicon, Xcode CLT, Rust stable, Node 22+, Claude Code;
  - the fixtures (`scripts/make-fixture-parent.sh`);
  - running a preview of a branch (`scripts/preview.sh`, and why never the installed app);
  - the checks table from CLAUDE.md "Stack & checks";
  - Conventional Commits;
  - the versioning policy (major/minor/patch rules from CLAUDE.md, `scripts/bump-version.sh <level> --commit`, which also regenerates CHANGELOG.md and needs git-cliff);
  - the PR flow (branch, PR template, CI green);
  - Spec Kit (`specs/README.md`) as optional.
- [ ] T027 [P] [US3] Create `SECURITY.md`:
  - **Reporting**: GitHub private vulnerability reporting (Security tab › Report a vulnerability), acknowledgement within ~7 days, no public issues for vulnerabilities.
  - **Supported versions**: the latest release.
  - **Threat model**, checked against the code:
    - the daemon listens on `127.0.0.1:7420` only (`crates/delilah-daemon/src/config.rs`);
    - the CORS allow-list is limited to the app's origins (`crates/delilah-daemon/src/api.rs` ~23-38);
    - the `Origin` check on state-changing requests and WebSockets, and `Host` must be loopback against DNS rebinding (~237-267);
    - there is no authentication between local processes: anything running as the same user can call the API, which is out of scope;
    - it starts Claude Code agents with the user's rights in per-task worktrees, with hooks passed through `--settings` outside the repo;
    - the Jira token lives in the macOS Keychain;
    - updates are verified with the updater signature; releases carry checksums and attestation.
  - **Known limits**: an unsigned app; agents run untrusted repo content with the user's rights.

  Describe only what the code does; re-read those lines before writing.
- [ ] T028 [P] [US3] Create `CODE_OF_CONDUCT.md`: Contributor Covenant 2.1 text, with enforcement contact "the maintainer through GitHub (@FranPerezFolgado)" and no email.
- [ ] T029 [P] [US3] Create `.github/ISSUE_TEMPLATE/bug_report.yml` (issue form):
  - required fields: what happened, steps to reproduce, expected; macOS version; Mac model/chip; Delilah version (About or `delilah --version`); Claude Code version (`claude --version`);
  - optional: logs (`~/Library/Logs/Delilah/delilahd.log`), screenshots;
  - label `bug`.
- [ ] T030 [P] [US3] Create `.github/ISSUE_TEMPLATE/feature_request.yml` (problem, proposal, alternatives; label `enhancement`) and `.github/ISSUE_TEMPLATE/config.yml` (`blank_issues_enabled: false`; a contact link "Report a vulnerability" → the repo's security advisories page).
- [ ] T031 [P] [US3] Create `.github/pull_request_template.md` with sections What and why, How it was tested (checks run, preview), Version bump (level and reason, per CONTRIBUTING), Docs updated (status/README/decisions), and a checklist.
- [ ] T032 [P] [US3] Create `.github/dependabot.yml` (version 2):
  - `cargo` at `/` and at `/app/src-tauri`, with a group `tauri` for `tauri*` crates and a group `all` for the rest;
  - `npm` at `/app`, with a group `tauri` for `@tauri-apps/*` and a group `all` for the rest;
  - `github-actions` at `/`, grouped;
  - all weekly, `open-pull-requests-limit: 5`, commit-message prefix `chore(deps)`.
- [ ] T033 [US3] Add `cliff.toml` (root): Conventional Commits parsers grouping feat → Features, fix → Fixes, perf → Performance, refactor → Refactor, docs → Docs, everything else → Other. Skip `chore: bump version`, merge commits and unconventional commits. Use the header `## [X.Y.Z] - YYYY-MM-DD` and a link to the GitHub compare. Generate `CHANGELOG.md` with `git cliff -o CHANGELOG.md` over all tags (v0.3.0…v1.0.0).
- [ ] T034 [US3] Extend `scripts/bump-version.sh`:
  - before writing anything, fail with "git-cliff is needed: brew install git-cliff" if `git cliff --version` fails;
  - after the lockfiles, run `git cliff --tag "v$new" -o CHANGELOG.md`;
  - add `CHANGELOG.md` to the `--commit` file list;
  - update its header comment.
- [ ] T035 [US3] Extend `scripts/release/check.mjs`: fail if `CHANGELOG.md` has no line starting with `## [X.Y.Z]` for the version being released, with the message "CHANGELOG.md has no X.Y.Z section: run scripts/bump-version.sh (or git cliff --tag vX.Y.Z -o CHANGELOG.md) on a branch". If `check.mjs` has tests (`scripts/release/*.test.*`), add a case. Document both changes in `docs/development.md` "Releasing".
- [ ] T036 [US3] Clean `CLAUDE.md`:
  1. Move the personal rules into a new git-ignored `CLAUDE.local.md`: developed on a personal Mac and tested on the work Mac, never copying from work repos, git/gh as the personal account FranPerezFolgado, work credentials on the machine. That file is not committed; check with `git status`.
  2. In `CLAUDE.md` keep the generic versions: "never copy code or data from private repos; use `scripts/make-fixture-parent.sh` and the Jira JSON fixtures"; "if several GitHub accounts are configured, confirm the one gh uses before any remote operation".
  3. Replace the two claude.ai links at the top with `docs/vision.md` (design) and `docs/redesign/` (UI screenshots).
  4. Update any links into moved docs.
- [ ] T037 [US3] Enable private vulnerability reporting (remote). Run `gh auth status` first and continue only as `FranPerezFolgado`. Then `gh api -X PUT repos/FranPerezFolgado/delilah/private-vulnerability-reporting`, and check with `gh api repos/FranPerezFolgado/delilah/private-vulnerability-reporting`.

**Checkpoint**: Story 3 passes; `cargo test --workspace` is green.

---

## Phase 6: User Story 4 - A non-Spanish speaker sees the app in English (P2)

**Goal**: The resolution rule in [contracts/language.md](contracts/language.md), in the web UI and the native menu.

**Independent test**: The Story 4 block of [quickstart.md](quickstart.md).

### Tests first

- [ ] T038 [P] [US4] Write `app/src/i18n.test.ts` (vitest). Use the contract's table as a `describe.each`: pref ∈ {missing, `null`, `"auto"`, `"fr"`, `42`, `"es"`, `"en"`} × system ∈ {`es-ES`, `es-419`, `ES`, `en-US`, `fr-FR`, `ja-JP`, `""`, no navigator}, asserting `detect()`. Also assert that `setLanguage("fr")` makes `lang()` `"en"` on an English system, and that `document.documentElement.lang` matches after import and after `setLanguage`. Mock `navigator.language` with `vi.stubGlobal`. Add a header comment saying the table is mirrored in `app/src-tauri/src/daemon.rs` tests and `contracts/language.md`. It must fail now.
- [ ] T039 [P] [US4] Add `#[cfg(test)] mod tests` to `app/src-tauri/src/daemon.rs` for `english_for(pref: Option<&str>, apple_languages: &str) -> bool`. Use the same table as T038, with the system given as `defaults read -g AppleLanguages` output, e.g. `"(\n    \"es-ES\",\n    \"en-US\"\n)"`, plus empty and garbage output. Add a comment pointing to the mirrored table. It must fail to compile now.

### Implementation

- [ ] T040 [US4] Implement it:
  - in `app/src/i18n.ts`, `detect(setting: unknown): Lang`: `"es"`/`"en"` are explicit; otherwise the system language, where `es*` (case-insensitive) → `es` and anything else, or no navigator → `en`;
  - `setLanguage(setting: unknown)`;
  - call `setLanguage("auto")` at module load so `document.documentElement.lang` is set from the start (guarded for no `document`), and keep the `LangSetting` type for Settings;
  - update the header comment;
  - in `app/index.html`, `lang="es"` → `lang="en"`;
  - in `app/src-tauri/src/daemon.rs`, extract `pub fn english_for(...)` with the same rule and make `english()` call it with the preference string (when it is a string) and the `defaults` output.

  T038 and T039 pass; `cd app && npx tsc --noEmit && npx vitest run`; `cargo test --manifest-path app/src-tauri/Cargo.toml`. `app/src/App.tsx` needs no change. Confirm that it passes the raw preference through `setLanguage`.
- [ ] T041 [US4] Try it in a preview: `scripts/preview.sh` (background), then open the UI with Settings › Language on Auto. Under an English browser locale the UI is English and `<html lang="en">`. Save a non-standard value through the API (`curl -X PUT <API>/api/v1/preferences/language -d '"fr"'` with the preview's API URL), reload, and expect English. Stop the preview by its process, never by name. The native menu needs the Tauri app: record it as checked by unit tests only in `progress.md`.
- [ ] T042 [P] [US4] Document the rule in `docs/architecture.md` (the UI section on language: Auto follows the system, Spanish only for Spanish systems, English otherwise and as fallback, native menu same rule) and in `docs/usage.md` (Settings › Language). Add a decision to `docs/decisions.md`: English is the fallback language; *ruled out*: English-only default even for Spanish systems.

**Checkpoint**: Story 4 passes on its own.

---

## Phase 7: User Story 5 - The maintainer knows the history holds no secrets (P3)

**Goal**: A redacted, triaged report of secrets across the full history, delivered outside the repo. No rewrite.

**Independent test**: `~/.delilah/tasks/LIBRE-15/secret-scan.md` exists, lists every finding (or none), and the git history is unchanged.

- [ ] T043 [P] [US5] Run `gitleaks git --log-opts=--all --redact -v --report-format json --report-path "$TMPDIR/gitleaks-delilah.json" .` from the repo root.
- [ ] T044 [US5] Triage each finding (open the commit and file; decide whether it is a real secret, a test fixture or a false positive) and write `~/.delilah/tasks/LIBRE-15/secret-scan.md` (outside the repo, never committed). Include the scan date, tool and version, commit range, and a table of findings (file, commit, rule, verdict, what to rotate). If a real secret shows up, also run `trufflehog git file://. --only-verified` for a second opinion. Stop and say so in `progress.md` and the PR description without rewriting anything. Delete the raw JSON afterwards.

---

## Phase 8: Polish & cross-cutting

- [ ] T045 Update `docs/status.md`:
  - built: public-repo docs, community files, English fallback, install script, checksums and attestation;
  - pending: the first tagged release verifying `SHA256SUMS` + attestation end to end (SC-007), community profile at 100% after merge (SC-008), and the pre-announcement audit (separate task);
  - known problems: none new.
- [ ] T046 Add the decision on the docs layout to `docs/decisions.md`: README for newcomers and the reference in `docs/usage.md`; history labelled or archived (`docs/archive/`, `specs/README.md`); personal agent rules in the git-ignored `CLAUDE.local.md`; changelog via git-cliff in the version bump.
- [ ] T047 Run every check:
  - `cargo test --workspace`;
  - `cargo test --manifest-path app/src-tauri/Cargo.toml`;
  - `cd app && npx tsc --noEmit && npx vitest run`;
  - `shellcheck` and `scripts/test-install.sh`;
  - every grep and the link check in [quickstart.md](quickstart.md).

  Fix whatever fails.
- [ ] T048 Bump the version last: `scripts/bump-version.sh minor --commit` (1.0.0 → 1.1.0; it regenerates CHANGELOG.md with the 1.1.0 section). Re-check the version on `main` first. Note the level and why in the PR description: minor, for the English fallback and the new release assets; the rest is patch-level.

---

## Dependencies & execution order

- **Setup (T001–T002)** → **Foundational (T003–T007)** → the story phases.
- **US1**:
  - T008 is needed before T012 (README links to usage.md) and T023;
  - T009, T010, T011 and T013 run in parallel;
  - T012 needs T008, T009 and T013;
  - T014 needs T008–T013;
  - T015 is independent.
- **US2**:
  - T016 → T017 (test first);
  - T018 needs T017;
  - T019–T022 run in parallel with T016/T017;
  - T023 needs T008;
  - T024 needs T012 and T017.
- **US3**:
  - T025–T032 run in parallel with each other and with US1/US2 files;
  - T033 → T034 → T035 (shared changelog pipeline);
  - T036 after T003/T004;
  - T037 is independent.
- **US4**: T038 ∥ T039 → T040 → T041; T042 after T008 (usage.md exists) and is serialised with other `docs/decisions.md` edits (T022, T046).
- **US5**: T043 → T044; independent of everything else.
- **Polish**: T045–T047 after all stories; T048 last.

**Same-file serialisation** (no two writers at once):
- `README.md`: T012 → T024
- `docs/usage.md`: T008 → T023 → T042
- `docs/decisions.md`: T011 → T022 → T042 → T046
- `docs/development.md`: T021 → T035
- `docs/status.md`: T011 → T045
- `CLAUDE.md`: T004 → T036
- `.gitignore`: T002

## Parallel examples

- After Foundational:
  - US1: T009 (screenshots), T010 (rename + tests), T011 (docs translation), T013 (docs index);
  - US2: T016 → T017 (install script), T019 (release.yml), T020 (release.sh);
  - US3: T025–T032 (community files);
  - US4: T038 + T039 (tests);
  - US5: T043 (scan).

## Implementation strategy

- **MVP**: Setup + Foundational + US1, which removes the stale, Spanish and private content from the first thing visitors see.
- **Then**: US2 (install path and trust) and US3 (community files) together, since the README's Install and Contributing sections need them, then US4 (minor-level behaviour change) and US5 (scan report).
- **Delivery**: one PR, with commits per block: docs layout, README/usage, community files, changelog pipeline, English fallback, distribution, status/decisions, version bump.
