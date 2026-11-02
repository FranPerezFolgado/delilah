# Work packages — 014-portfolio-readme

Source: specs/014-portfolio-readme/tasks.md · 16 pending tasks (T001, T002 done) · 11 packages

Checks: this is a docs change, so there is no test suite. The checks are `quickstart.md` ("Checks before the PR") and, for WP-01, `web: cd app && npx tsc --noEmit`.

## WP-01 Playground captures and hero images
- Phase: 2 Foundational
- Tasks: T003, T004, T005
- Files: app/e2e/screens.spec.ts, docs/images/playground-light.png, docs/images/playground-dark.png, docs/images/playground.gif (only if T004 passes), docs/images/board-*.png (removed only if unused)
- Depends on: —
- Parallel group: 2A
- Acceptance: FR-004 — Playground screen is captured in light and dark with demo data only, the GIF outcome is recorded, and existing screens behave as before without `DELILAH_SCREENS_MOTION`.
- Checks: web (tsc --noEmit), ui-verifier for the images
- Reviewers: —

## WP-02 Install detail moved to usage.md
- Phase: 2 Foundational
- Tasks: T006
- Files: docs/usage.md
- Depends on: —
- Parallel group: 2A
- Acceptance: FR (install routes preserved) — `## Install` and `## Getting started` exist in usage.md, are in the Contents list, and keep the old README meaning, including the links to decisions 79 and 80.
- Checks: quickstart link and anchor check
- Reviewers: —

## WP-03 README rewrite
- Phase: 3 User Story 1
- Tasks: T007
- Files: README.md
- Depends on: WP-01, WP-02
- Parallel group: 3A
- Acceptance: US1 — README follows the approved outline.md, is about 200 lines or fewer and in English, has Mermaid, a light/dark `<picture>` hero and an `## Install` section; its claims match usage.md and architecture.md (FR-013).
- Checks: quickstart (line count, links)
- Reviewers: —

## WP-04 Decision record and status
- Phase: 3 User Story 1
- Tasks: T008, T009
- Files: docs/decisions.md, docs/status.md
- Depends on: —
- Parallel group: 3A
- Acceptance: US1 — a new decision (104 unless taken) is added, decision 82 has an inline note, and status.md records the feature and fixes line 18.
- Checks: quickstart grep and link checks
- Reviewers: —

## WP-05 Install route walkthrough
- Phase: 4 User Story 2
- Tasks: T010
- Files: README.md, docs/usage.md
- Depends on: WP-02, WP-03
- Parallel group: 4A
- Acceptance: US2 — every old install route is reachable from README in at most one link, and the install section is at most a quarter of the page (SC-002).
- Checks: quickstart link checks
- Reviewers: —

## WP-06 Contributor stance
- Phase: 5 User Story 3
- Tasks: T011, T012
- Files: CONTRIBUTING.md, SECURITY.md
- Depends on: —
- Parallel group: 5A
- Acceptance: US3 — CONTRIBUTING opens with the personal-project stance and no rule is lost; SECURITY line 9 is best-effort with no time target.
- Checks: quickstart diffs
- Reviewers: —

## WP-07 Dependabot proposal refresh
- Phase: 6 User Story 4
- Tasks: T013
- Files: specs/014-portfolio-readme/dependabot-proposal.md
- Depends on: —
- Parallel group: 6A
- Acceptance: US4 — counts and dates are refreshed using the right `gh` account, and the alert status is stated or marked undeterminable.
- Checks: quickstart
- Reviewers: —

## WP-08 Dependabot owner gate (orchestrator)
- Phase: 6 User Story 4
- Tasks: T014
- Files: .github/dependabot.yml (only on explicit owner approval)
- Depends on: WP-07
- Parallel group: 6B
- Acceptance: FR-017 — the orchestrator presents the proposal to the owner; `git diff main -- .github/dependabot.yml` is empty without approval. Not for an implementer.
- Checks: `git diff main -- .github/dependabot.yml`
- Reviewers: —

## WP-09 CLAUDE.md wording
- Phase: 7 Polish
- Tasks: T015
- Files: CLAUDE.md
- Depends on: WP-03
- Parallel group: 7A
- Acceptance: Line 24 no longer says "short newcomer page" and matches the new positioning.
- Checks: quickstart `README|newcomer` grep
- Reviewers: —

## WP-10 Final checks
- Phase: 7 Polish
- Tasks: T016
- Files: README.md, CONTRIBUTING.md, docs/usage.md (fix-ups only; any file the checks flag)
- Depends on: WP-01, WP-02, WP-03, WP-04, WP-05, WP-06, WP-07, WP-08, WP-09
- Parallel group: 7B
- Acceptance: all quickstart checks pass, and README figures match a fresh run at the stated revision.
- Checks: quickstart "Checks before the PR"
- Reviewers: —

## WP-11 Push and PR (orchestrator)
- Phase: 7 Polish
- Tasks: T017, T018
- Files: —
- Depends on: WP-10
- Parallel group: 7C
- Acceptance: SC-007 — the `gh` account is FranPerezFolgado, Mermaid and hero render in light and dark (via ui-verifier), and the PR is opened with the agreed title, the GIF outcome and the dependabot proposal. No version bump. Not for an implementer.
- Checks: ui-verifier
- Reviewers: —
