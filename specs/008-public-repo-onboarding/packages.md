# Work packages — 008 Public repo onboarding

Source: specs/008-public-repo-onboarding/tasks.md · 48 pending tasks · 24 packages

Check names: `rust:workspace` (cargo test --workspace), `rust:app-shell` (cargo test --manifest-path app/src-tauri/Cargo.toml), `web` (cd app && npx tsc --noEmit && npx vitest run), `shellcheck`, `docs:greps` and `docs:links` (quickstart.md greps and `npx --yes lychee --offline`).

Flags: [MAINTAINER/REMOTE] = touches GitHub or needs a human decision; an implementer may run it only if `gh auth status` shows FranPerezFolgado and the rules in tasks.md allow it. [LOCAL-ENV] = changes the machine, not the repo.

## WP-01 Setup: tools and gitignore
- Phase: 1 Setup
- Tasks: T001, T002
- Files: specs/008-public-repo-onboarding/progress.md, .gitignore
- Depends on: —
- Parallel group: 1A
- Acceptance: FR-012 — missing tools (git-cliff, gitleaks, shellcheck, lychee) noted in progress.md; CLAUDE.local.md is git-ignored. T001 is [LOCAL-ENV] (brew install on the machine).
- Checks: git status
- Reviewers: —

## WP-02 Docs layout: archive and history labels
- Phase: 2 Foundational
- Tasks: T003, T004, T005, T006, T007
- Files: docs/archive/spikes/, docs/archive/next-moves.md, docs/archive/ui-redesign.md, docs/archive/README.md, specs/README.md, docs/redesign/README.md, CLAUDE.md, docs/decisions.md, docs/status.md, docs/architecture.md, docs/development.md
- Depends on: WP-01
- Parallel group: 2A
- Acceptance: FR-007, FR-010 — moved docs have no dangling links; archive and specs are labelled as history; redesign README in English.
- Checks: docs:links
- Reviewers: —

## WP-03 usage.md and docs index
- Phase: 3 US1
- Tasks: T008, T013
- Files: docs/usage.md, docs/README.md, README.md (read only as source)
- Depends on: WP-02
- Parallel group: 3A
- Acceptance: FR-006, FR-007, FR-008 — full reference in English with English UI labels and a ToC, no stale claims; docs index exists.
- Checks: docs:greps, docs:links
- Reviewers: —

## WP-04 Translate current docs to English
- Phase: 3 US1
- Tasks: T011
- Files: docs/architecture.md, docs/decisions.md, docs/development.md, docs/status.md, docs/vision.md, docs/adr/ADR-001-chat-mode-allow-always-allowlist.md
- Depends on: WP-02
- Parallel group: 3A
- Acceptance: FR-007, FR-009, FR-012 — no Spanish prose or labels, no claude.ai link, no "work Mac".
- Checks: docs:greps
- Reviewers: —

## WP-05 Rename work project name in code
- Phase: 3 US1
- Tasks: T010
- Files: crates/delilah-daemon/src/session_api.rs, crates/delilah-daemon/src/store_projects.rs, other non-doc `plataforma` hits (guess; docs hits belong to WP-04, README to WP-07)
- Depends on: WP-02
- Parallel group: 3A
- Acceptance: FR-011 — `plataforma` is gone outside specs/ and README; tests pass.
- Checks: rust:workspace
- Reviewers: —

## WP-06 English screenshots
- Phase: 3 US1
- Tasks: T009
- Files: app/e2e/screens.spec.ts, docs/images/board-light.png, docs/images/board-dark.png
- Depends on: WP-02
- Parallel group: 3A
- Acceptance: FR-004 — two 1440x900 PNGs under 300 KB in English, captured only against an isolated e2e backend, never 7420. Needs a browser/Playwright run.
- Checks: web
- Reviewers: —

## WP-07 README rewrite
- Phase: 3 US1
- Tasks: T012
- Files: README.md
- Depends on: WP-03, WP-06
- Parallel group: 3B
- Acceptance: FR-004, FR-005, SC-001, SC-002 — under 200 lines, badges, picture, Why, Requirements, placeholder Install, quickstart of at most 5 commands, links, Anthropic disclaimer.
- Checks: docs:links
- Reviewers: —

## WP-08 Story 1 verification pass
- Phase: 3 US1
- Tasks: T014
- Files: README.md, docs/**/*.md (fixes only where a check hits)
- Depends on: WP-03, WP-04, WP-05, WP-06, WP-07
- Parallel group: 3C
- Acceptance: SC-004, SC-005 — forbidden-terms grep, Spanish-labels grep and offline link check all clean (docs/archive excluded).
- Checks: docs:greps, docs:links
- Reviewers: —

## WP-09 Repo description and topics [MAINTAINER/REMOTE]
- Phase: 3 US1
- Tasks: T015
- Files: — (GitHub remote only; result noted in specs/008-public-repo-onboarding/progress.md)
- Depends on: —
- Parallel group: 3A
- Acceptance: FR-003 — `gh repo view --json description,repositoryTopics` shows the new description and 7 topics. Must stop unless the active gh account is FranPerezFolgado. Changes the public remote, so the orchestrator should confirm before running it.
- Checks: gh repo view
- Reviewers: security

## WP-10 Install script and its test
- Phase: 4 US2
- Tasks: T016, T017
- Files: scripts/test-install.sh, scripts/install.sh
- Depends on: —
- Parallel group: 4A
- Acceptance: FR-023 — test written first (fails), then install.sh per contracts/install-script.md; all cases a-h pass and shellcheck is clean.
- Checks: shellcheck, scripts/test-install.sh
- Reviewers: security

## WP-11 CI and release pipeline: checksums and attestation
- Phase: 4 US2
- Tasks: T018, T019, T020
- Files: .github/workflows/ci.yml, .github/workflows/release.yml, scripts/release.sh
- Depends on: WP-10
- Parallel group: 4B
- Acceptance: FR-025, FR-026, FR-027 — CI runs shellcheck and test-install; release uploads SHA256SUMS and attests the assets, publish stays last; dry run prints sums. Not verifiable until the next tag.
- Checks: shellcheck, workflow YAML review
- Reviewers: security

## WP-12 Release docs, decisions and uninstall
- Phase: 4 US2
- Tasks: T021, T022, T023
- Files: docs/development.md, docs/decisions.md, docs/usage.md
- Depends on: WP-03, WP-04
- Parallel group: 4A
- Acceptance: FR-026, FR-028, FR-030 — Releasing section lists SHA256SUMS, attestation, verify commands; two decisions (unsigned distribution, no Homebrew tap) added; Uninstall section in usage.md.
- Checks: docs:links
- Reviewers: —

## WP-13 README Install section
- Phase: 4 US2
- Tasks: T024
- Files: README.md
- Depends on: WP-07, WP-10
- Parallel group: 4B
- Acceptance: FR-024 — script, why unsigned, manual download with xattr / Open Anyway, verification, build from source, in-app updates; README stays under 200 lines.
- Checks: docs:links
- Reviewers: —

## WP-14 Community files and dependabot
- Phase: 5 US3
- Tasks: T025, T028, T029, T030, T031, T032
- Files: LICENSE, app/package.json, CODE_OF_CONDUCT.md, .github/ISSUE_TEMPLATE/bug_report.yml, .github/ISSUE_TEMPLATE/feature_request.yml, .github/ISSUE_TEMPLATE/config.yml, .github/pull_request_template.md, .github/dependabot.yml
- Depends on: —
- Parallel group: 5A
- Acceptance: FR-001, FR-002, FR-015, FR-017, FR-018 — LICENSE and all manifests declare MIT; issue forms, PR template and dependabot config valid.
- Checks: web (tsc only), YAML validity
- Reviewers: —

## WP-15 CONTRIBUTING and SECURITY
- Phase: 5 US3
- Tasks: T026, T027
- Files: CONTRIBUTING.md, SECURITY.md (reads crates/delilah-daemon/src/config.rs and api.rs)
- Depends on: —
- Parallel group: 5A
- Acceptance: FR-013, FR-014 — contributor guide reusing docs/development.md; threat model describing only defences present in the code.
- Checks: docs:links
- Reviewers: security

## WP-16 Changelog pipeline
- Phase: 5 US3
- Tasks: T033, T034, T035
- Files: cliff.toml, CHANGELOG.md, scripts/bump-version.sh, scripts/release/check.mjs, scripts/release/*.test.* (guess), docs/development.md
- Depends on: WP-12
- Parallel group: 5A
- Acceptance: FR-016 — CHANGELOG.md covers v0.3.0 to v1.0.0; bump-version regenerates it; release check fails without the version's section. Needs git-cliff installed (WP-01).
- Checks: node scripts/release tests, bash -n scripts/bump-version.sh
- Reviewers: —

## WP-17 Clean CLAUDE.md
- Phase: 5 US3
- Tasks: T036
- Files: CLAUDE.md, CLAUDE.local.md (git-ignored, not committed)
- Depends on: WP-01, WP-02
- Parallel group: 5A
- Acceptance: FR-009, FR-012 — no personal or work notes and no claude.ai links in CLAUDE.md; they live in the ignored CLAUDE.local.md.
- Checks: docs:greps, git status
- Reviewers: —

## WP-18 Private vulnerability reporting [MAINTAINER/REMOTE]
- Phase: 5 US3
- Tasks: T037
- Files: — (GitHub remote only)
- Depends on: —
- Parallel group: 5A
- Acceptance: FR-014 — `gh api repos/FranPerezFolgado/delilah/private-vulnerability-reporting` reports enabled. Changes repo settings; stop unless gh account is FranPerezFolgado, and the orchestrator should confirm before running.
- Checks: gh api
- Reviewers: security

## WP-19 English fallback (web UI and native menu)
- Phase: 6 US4
- Tasks: T038, T039, T040, T041
- Files: app/src/i18n.test.ts, app/src/i18n.ts, app/index.html, app/src-tauri/src/daemon.rs, app/src/App.tsx (verify only)
- Depends on: —
- Parallel group: 6A
- Acceptance: FR-019, FR-020, FR-021, FR-022, SC-006 — tests first fail then pass; the same table holds in TS and Rust; preview check in English via scripts/preview.sh. The native menu cannot be tried without the Tauri app (unit tests only).
- Checks: web, rust:app-shell
- Reviewers: architecture

## WP-20 Language docs and decision
- Phase: 6 US4
- Tasks: T042
- Files: docs/architecture.md, docs/usage.md, docs/decisions.md
- Depends on: WP-19, WP-03
- Parallel group: 6B
- Acceptance: FR-030 — language rule documented in architecture and usage; decision added with the ruled-out option.
- Checks: docs:links
- Reviewers: —

## WP-21 Secret scan [MAINTAINER/REMOTE-ADJACENT]
- Phase: 7 US5
- Tasks: T043, T044
- Files: ~/.delilah/tasks/LIBRE-15/secret-scan.md (outside the repo, never committed), specs/008-public-repo-onboarding/progress.md (only if a real secret is found)
- Depends on: WP-01
- Parallel group: 7A
- Acceptance: FR-029, SC-009 — redacted, triaged report delivered to the maintainer; history unchanged. Running gitleaks is automatable; deciding that a finding is a real secret, rotating it and any history rewrite are maintainer decisions. If a real secret appears the implementer must stop and report, not rewrite.
- Checks: git log unchanged
- Reviewers: security

## WP-22 Status and layout decision
- Phase: 8 Polish
- Tasks: T045, T046
- Files: docs/status.md, docs/decisions.md
- Depends on: WP-08, WP-13, WP-16, WP-17, WP-20, WP-21
- Parallel group: 8A
- Acceptance: FR-030 — status lists what is built and pending (first tagged release verification, community profile, audit); layout decision recorded.
- Checks: docs:links
- Reviewers: —

## WP-23 Run every check
- Phase: 8 Polish
- Tasks: T047
- Files: any file a failing check points to
- Depends on: WP-22
- Parallel group: 8B
- Acceptance: SC-010, SC-004, SC-005 — all three test suites, shellcheck, test-install, greps and link check pass.
- Checks: rust:workspace, rust:app-shell, web, shellcheck, docs:greps, docs:links
- Reviewers: —

## WP-24 Version bump
- Phase: 8 Polish
- Tasks: T048
- Files: version files and lockfiles per docs/development.md#releasing, CHANGELOG.md
- Depends on: WP-23
- Parallel group: 8C
- Acceptance: FR-031 — `scripts/bump-version.sh minor --commit` (1.0.0 to 1.1.0) as its own commit after re-checking main's version; level and reason go into the PR text.
- Checks: rust:workspace
- Reviewers: —
