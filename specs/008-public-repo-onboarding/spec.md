# Feature Specification: Public repo onboarding

**Feature Branch**: `008-public-repo-onboarding`

**Created**: 2026-10-26

**Status**: Draft

**Input**: Task LIBRE-15: make the public GitHub repo (already public, v1.0.0)
understandable and welcoming for outside users and contributors, with no behaviour change
except the UI's fallback language. Decisions already taken: no Apple signing or
notarization (distribution stays unsigned; reduce friction and add trust another way);
the licence is MIT.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - A newcomer understands and installs Delilah (Priority: P1)

Someone lands on the GitHub page with no prior context. In under a minute they learn
what Delilah is, why they might want it, whether it runs on their machine, and how to
install it and run a first task. Nothing on the page is stale, half in Spanish, or links
somewhere they cannot open.

**Why this priority**: The repo is already public. The README is the first and often
only thing visitors read, and today it is a 573-line manual for the author, with stale
and wrong claims.

**Independent Test**: Give the repo page to someone who has never seen Delilah. They can
tell what it does, check the requirements, install it and run a first task by following
only the README and the docs it links to.

**Acceptance Scenarios**:

1. **Given** a visitor on the repo page, **When** they read the top of the README, **Then** they see a one-line pitch, a screenshot of the app, badges (CI, licence, latest release, macOS only), and a short "why" covering isolated workspaces, automatic pause/resume on usage limits, the inbox, and its light footprint, with a link to the measured resource usage.
2. **Given** a visitor checking whether Delilah fits their machine, **When** they read the requirements, **Then** they learn it needs macOS on Apple Silicon and a logged-in Claude Code, and that Claude Code is the only agent supported today.
3. **Given** a visitor who wants to try it, **When** they follow the quickstart, **Then** at most 5 commands take them from install to a running task.
4. **Given** a visitor who needs the full CLI, settings, Jira or app reference, **When** they follow the README's link, **Then** that reference is in a separate usage document, unchanged in substance and written in English.
5. **Given** any page in the repo's docs, **When** a visitor reads it, **Then** it is in English, names UI elements by their English labels, and contains no stale statement ("MVP-0 in progress", "the repo is private / deploy key", support for agents other than Claude Code).
6. **Given** the repo page, **When** a visitor looks at the description and topics, **Then** the description matches what Delilah does today (Claude Code only), and the topics include claude-code, ai-agents, tauri, rust, macos, orchestrator and git-worktree.
7. **Given** the README footer, **When** a visitor reads it, **Then** it states the MIT licence and that Delilah is not affiliated with or endorsed by Anthropic.

---

### User Story 2 - A user installs an unsigned build without fighting macOS (Priority: P1)

A user who downloads Delilah gets it running with as little friction as an unsigned app
allows, understands why macOS warns about it, and can check that the build they got is
the one the project's CI produced.

**Why this priority**: Without Apple signing, macOS blocks a downloaded app by default.
If the install path is unclear, people give up before the first run. Being able to verify
the build makes up for the missing signature.

**Independent Test**: On a clean Apple Silicon Mac, install the latest release by each
documented route and launch the app. Then verify the downloaded artefact against the
published checksum and the build attestation.

**Acceptance Scenarios**:

1. **Given** a Mac with no Delilah, **When** the user runs the documented one-line install script, **Then** the latest release is downloaded without the quarantine flag, `Delilah.app` ends up in `/Applications`, and it opens with no Gatekeeper prompt.
2. **Given** a user who downloaded the release by hand, **When** they read the install section, **Then** they find why the app is unsigned and two manual ways to open it: clearing the quarantine flag with one command, and System Settings › Privacy & Security › Open Anyway. The section also says that right-click › Open no longer works since macOS Sequoia.
3. **Given** a user who prefers to build from source, **When** they follow the install section, **Then** they find the prerequisites and the commands to build and install the app.
4. **Given** a published release, **When** the user downloads its assets, **Then** a SHA-256 checksum file is published alongside, and a build-provenance attestation can be checked with `gh attestation verify`. The install section shows both checks.
5. **Given** a user already running Delilah, **When** the app updates itself, **Then** the update needs no quarantine workaround, and the install section says so.
6. **Given** the install script, **When** the download or checksum check fails, or the Mac is not Apple Silicon, **Then** the script stops with a clear message and does not touch an existing install.

---

### User Story 3 - An outside contributor can build, test and send a change (Priority: P2)

A developer who wants to fix something finds how to build, run and test the project, how
commits and versions work, how to report bugs and vulnerabilities, and what behaviour is
expected of them.

**Why this priority**: Contributions only come once users exist (Stories 1–2), but the
files have to be there before the repo is announced.

**Independent Test**: Following only CONTRIBUTING.md, a developer on a fresh clone
builds the project, runs a preview and the checks, and opens a PR that fills in the
template.

**Acceptance Scenarios**:

1. **Given** a fresh clone, **When** a contributor follows CONTRIBUTING.md, **Then** they can build the project, run a preview of their branch, and run the same checks the project uses (Rust workspace, desktop shell, web UI). They also learn the commit convention (Conventional Commits) and the versioning policy.
2. **Given** someone who found a vulnerability, **When** they open SECURITY.md, **Then** they find a private way to report it and what to expect, plus a short threat model of the local daemon: an HTTP API on loopback only, that it starts agent processes, and how it defends against browser-based attacks (cross-origin requests, DNS rebinding).
3. **Given** a contributor opening an issue, **When** they pick "Bug report", **Then** the form asks for the macOS, Delilah and Claude Code versions. A "Feature request" form exists too.
4. **Given** a contributor opening a PR, **When** the PR form loads, **Then** a template asks what changed, how it was tested, and which version level it bumps.
5. **Given** the repo, **When** a visitor looks for the code of conduct, licence and changelog, **Then** each is at the repo root. The changelog is generated from the commit history and covers the published releases.
6. **Given** the repo's dependency manifests (Rust, npm, GitHub Actions), **When** an update is published upstream, **Then** automated update PRs are opened for it.
7. **Given** the project's agent instructions file (CLAUDE.md) and docs, **When** an outsider reads them, **Then** they contain no personal notes (machines, accounts, credentials) and no links to private pages. The historical material (specs/, .specify/, docs/redesign/, docs/spikes/, docs/next-moves.md) is either explained in one line where it lives or moved under an archive folder, so it is clear it is history and not current docs.
8. **Given** the code and tests, **When** someone greps for the name of the author's work project ("plataforma"), **Then** there are no matches outside the historical specs. Examples and fixtures use a generic name.

---

### User Story 4 - A non-Spanish speaker sees the app in English (Priority: P2)

A user whose Mac is not set to Spanish opens Delilah and sees English everywhere,
including in the native menu, without touching Settings. Spanish speakers keep Spanish.

**Why this priority**: Most outside users don't speak Spanish. Today the "auto" setting
already picks English for most system languages, but several fallbacks still default to
Spanish when the system language can't be read.

**Independent Test**: With the language setting on "auto" (or unset), launch the app
under several system languages (Spanish, English, French, Japanese, and an unreadable or
empty one). Only Spanish gives Spanish UI; every other case gives English UI and English
native menus.

**Acceptance Scenarios**:

1. **Given** the language setting is "auto" or was never set, **When** the system's preferred language is Spanish (any region), **Then** the UI and the native menu are in Spanish.
2. **Given** the language setting is "auto" or was never set, **When** the system's preferred language is anything else, **Then** the UI and the native menu are in English.
3. **Given** the system language can't be determined, or the stored setting holds an unknown value, **When** the app starts, **Then** it uses English.
4. **Given** the user explicitly chose Spanish or English in Settings, **When** the app starts, **Then** that choice wins over the system language, as today.
5. **Given** the app is loading, **When** the first screen is drawn before preferences arrive, **Then** the page's declared language and the first texts already match the language the app is about to settle on. If that can't be known yet, they are English, not Spanish.

---

### User Story 5 - The maintainer knows the history holds no secrets (Priority: P3)

Before announcing the repo, the maintainer gets a report of any secret ever committed in
the full git history, so they can decide what to rotate and whether to rewrite history.

**Why this priority**: The repo is already public, so anything found is already exposed.
The scan tells the maintainer what to rotate. It changes no files.

**Independent Test**: Run the scan over every commit. The report lists each finding
(file, commit, rule, whether it is a real secret or a false positive) or states that
nothing was found.

**Acceptance Scenarios**:

1. **Given** the full git history, **When** the secret scan runs, **Then** the maintainer gets a report of findings with file, commit and kind, and an assessment of each one (real or false positive).
2. **Given** findings exist, **When** the report is delivered, **Then** no history rewrite or force-push happens unless the maintainer explicitly asks for it.

---

### Edge Cases

- A link in the docs that pointed into the README's moved sections must still land on the right content (anchors updated, no dead links inside the repo).
- The README screenshot must come from the current UI (v2), not the old one.
- The install script is run on an Intel Mac, on a Mac without `curl`, or with an existing Delilah in `/Applications`, including one that is still running: it must not leave a half-copied app behind.
- The install script is run twice: the second run reinstalls or updates cleanly.
- The latest release has no checksum file (older releases): the script reports this and stops, unless the user overrides that on purpose.
- The system language is reported as something like "es-419" or "ES": it still counts as Spanish.
- The stored language setting holds a value from an older or newer version: it is treated as "auto".
- The changelog generator meets commits that don't follow Conventional Commits (older history): they are grouped or skipped, not a failure.
- Automated dependency updates must not flood the repo: they are grouped and rate-limited.
- Moving historical folders must not break Spec Kit, which needs specs/ and .specify/ where they are.

## Requirements *(mandatory)*

### Functional Requirements

**Licence and metadata**

- **FR-001**: The repo MUST have a root LICENSE file with the MIT licence text and the copyright holder.
- **FR-002**: Every package manifest (Rust workspace, desktop shell, web UI) MUST declare the MIT licence.
- **FR-003**: The GitHub repo description MUST describe Delilah as it is today (Claude Code only), and the repo MUST carry the topics claude-code, ai-agents, tauri, rust, macos, orchestrator and git-worktree. The remote change MUST be made only after confirming the account that makes it.

**README and docs**

- **FR-004**: The README MUST open with a one-line pitch, a current screenshot, and badges for CI status, licence, latest release and "macOS only".
- **FR-005**: The README MUST include, in this order or close to it: why Delilah exists, requirements, install, a quickstart of at most 5 commands, links to the detailed docs, contributing, licence, and a "not affiliated with Anthropic" note.
- **FR-006**: The detailed CLI, settings, Jira and app reference now in the README MUST move to a separate usage document linked from the README, with its substance preserved.
- **FR-007**: All repo docs meant for readers (README, docs/, community files) MUST be in English and name UI elements by their English labels.
- **FR-008**: These stale statements MUST be removed or corrected: "Status: MVP-0 in progress"; "the repo is private / deploy key"; claims of Codex or Pi support; the Spanish "Actualizaciones" section (rewritten in English).
- **FR-009**: No current doc or CLAUDE.md MUST link to a private claude.ai page. Each link MUST be replaced by the repo content it stood for, or removed.
- **FR-010**: Historical material (specs/, .specify/, docs/redesign/, docs/spikes/, docs/next-moves.md) MUST be labelled as history in one line where it lives, or moved under an archive folder. Spec Kit MUST keep working.
- **FR-011**: The name of the author's work project ("plataforma") MUST be replaced by a generic name in the README, the docs and the tests. Tests MUST still pass.
- **FR-012**: CLAUDE.md MUST hold no personal notes (personal or work machines, account names, work credentials). The project rules that matter to any contributor's agent MUST stay.

**Community files**

- **FR-013**: The repo MUST have CONTRIBUTING.md covering: prerequisites, building, running a preview of a branch, the checks per area, Conventional Commits, the versioning policy, and how PRs are reviewed. It MUST reuse the development docs rather than duplicate them.
- **FR-014**: The repo MUST have SECURITY.md with a private reporting channel and expected response, the supported versions, and a threat model of the local daemon: the API listens on loopback only, it starts agent processes with the user's rights, and the cross-origin and DNS-rebinding defences it has. The threat model MUST describe what exists, not invent defences.
- **FR-015**: The repo MUST have CODE_OF_CONDUCT.md (Contributor Covenant) with a contact for reports.
- **FR-016**: The repo MUST have CHANGELOG.md generated from the Conventional Commits history and covering every published release. The release process MUST say how to regenerate it.
- **FR-017**: The repo MUST have issue forms for a bug report (asking for the macOS, Delilah and Claude Code versions, steps, expected and actual result, logs) and a feature request, plus a pull request template.
- **FR-018**: Automated dependency updates MUST be configured for the Rust crates (workspace and desktop shell), npm (web UI) and GitHub Actions, grouped and on a weekly schedule.

**UI language**

- **FR-019**: With the language setting on "auto", unset or unknown, the UI MUST use Spanish when the system's preferred language is Spanish (any region or casing) and English in every other case, including when the system language can't be read.
- **FR-020**: The native menu (desktop shell) MUST follow the same rule as the UI.
- **FR-021**: An explicit choice of Spanish or English in Settings MUST keep winning over the system language.
- **FR-022**: Before preferences load, the page's declared language and the first texts MUST follow FR-019, so the first screen never flashes Spanish for a non-Spanish system.

**Distribution and trust**

- **FR-023**: The repo MUST provide an install script that downloads the latest release over HTTPS without the quarantine flag, checks it against the published SHA-256 checksum, and installs `Delilah.app` into `/Applications`. It MUST refuse non-Apple-Silicon Macs, stop on any failed step without breaking an existing install, and say what it did.
- **FR-024**: The README install section MUST explain why the app is unsigned and offer: the install script; downloading by hand and clearing the quarantine flag; System Settings › Privacy & Security › Open Anyway; and building from source. It MUST say that right-click › Open no longer bypasses Gatekeeper since macOS Sequoia, and that in-app updates are not quarantined.
- **FR-025**: Every release MUST publish a SHA-256 checksum file covering its downloadable assets.
- **FR-026**: Every release MUST publish a build-provenance attestation for its downloadable assets, verifiable with `gh attestation verify`. The docs MUST show the command.
- **FR-027**: Releases MUST keep the ad-hoc code signature and the in-app updater signature they have today.
- **FR-028**: The current Homebrew options for unsigned apps (a third-party tap, quarantine handling) MUST be researched and the outcome recorded as a decision. Publishing a tap is out of scope for this feature.

**Process**

- **FR-029**: A secret scan over the full git history MUST be run and its findings reported to the maintainer. No history rewrite or force-push may happen without the maintainer's explicit approval.
- **FR-030**: The project status doc, the decisions log and the release docs MUST be updated with what this feature changes (English fallback, checksums and attestation, install script, community files, the Homebrew outcome).
- **FR-031**: The change MUST bump the version once, at the highest level it contains: minor, because of the English fallback (UI behaviour) and the release workflow additions. The rest is patch-level.

### Key Entities

- **Release assets**: the app archive and disk image, the updater manifest and signature, plus the new checksum file and provenance attestation.
- **Language setting**: "auto" (default), "es" or "en", kept by the daemon. It resolves to the language the UI and native menu use.
- **Community files**: LICENSE, CONTRIBUTING, SECURITY, CODE_OF_CONDUCT, CHANGELOG, the issue and PR templates, and the dependency-update config.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: A first-time visitor can say what Delilah does, which platform it needs and how to install it after reading only the part of the README before the first section break (under 60 seconds of reading).
- **SC-002**: The README is under 200 lines, and its quickstart has at most 5 commands.
- **SC-003**: On a clean Apple Silicon Mac, the install script gets Delilah from nothing to an open app with zero Gatekeeper prompts, in under 2 minutes on a normal connection.
- **SC-004**: 0 occurrences in current docs, CLAUDE.md and code of: claude.ai artifact links, "plataforma", "MVP-0 in progress", "deploy key", Codex/Pi support claims, Spanish prose, or Spanish UI labels. Historical folders are excluded.
- **SC-005**: 0 broken relative links across the README and docs.
- **SC-006**: Under 6 system languages (es-ES, es-419, en-US, fr-FR, ja-JP, and none or unreadable) with the "auto" setting, the UI and native menu are Spanish for the first 2 and English for the other 4.
- **SC-007**: The next tagged release publishes a checksum file and an attestation that verifies with one command.
- **SC-008**: GitHub's community profile for the repo lists every recommended file as present (description, README, licence, code of conduct, contributing, security policy, issue templates, PR template).
- **SC-009**: The maintainer receives the secret-scan report before the feature's PR is opened, and the git history is unchanged unless they approved a rewrite.
- **SC-010**: All existing checks (Rust workspace, desktop shell, web UI) still pass.

## Assumptions

- Delivered as one branch and one PR, with one commit (or more) per block: basics, community files, English fallback, distribution. The notes allowed block 1 as a PR of its own. One PR keeps the single required version bump simple, and the per-block commits still let it be split if wanted.
- The copyright holder in LICENSE is the maintainer's public name as used on GitHub, and the year is 2026.
- Vulnerability reports go through GitHub's private vulnerability reporting, which must be enabled on the repo (a remote setting, confirmed with the account like FR-003). The code-of-conduct contact is the maintainer's public GitHub contact. No personal email is published unless the maintainer adds one.
- specs/ and .specify/ stay where they are, because Spec Kit needs them; a one-line note labels them. docs/redesign/ also stays (the README screenshot comes from it), with a one-line note. Spikes and next-moves are labelled or moved under docs/archive/.
- The private design-doc artifact is replaced by docs/vision.md, which already summarises it. The UI-board artifacts are replaced by the screenshots in docs/redesign/.
- The changelog is generated once now and regenerated at each release by a step in the release process. Older commits that don't follow Conventional Commits are grouped under "Other" or skipped.
- The updater endpoint and repository owner in the build config stay as they are; transferring the repo is out of scope.
- Out of scope: the pre-announcement security and AI-safety audit (a separate task), Apple signing or notarization, publishing a Homebrew tap, support for other agents or Intel Macs, and translating the Spanish source strings in the UI code (the UI's English translation table already exists).
