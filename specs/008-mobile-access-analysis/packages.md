# Work packages — 008-mobile-access-analysis

Source: specs/008-mobile-access-analysis/tasks.md · 20 pending tasks · 9 packages

Docs-only feature (FR-009): no code, no tests. Checks for every package: none executable; the check is that the named Markdown files exist, contain the requested content and every added link resolves.

## WP-01 Experiment folder
- Phase: 1 Setup
- Tasks: T001
- Files: specs/008-mobile-access-analysis/experiment/README.md
- Depends on: —
- Parallel group: 1A
- Acceptance: FR-009 — experiment/ exists with a README saying it holds throwaway notes and screenshots, not shipped
- Checks: none (docs only)
- Reviewers: —

## WP-02 Complete research.md
- Phase: 2 Foundational
- Tasks: T002, T003, T004, T005, T006
- Files: specs/008-mobile-access-analysis/research.md, specs/008-mobile-access-analysis/checklists/security.md (read only)
- Depends on: —
- Parallel group: 2A
- Acceptance: FR-001, FR-002, FR-003, FR-004, FR-005, FR-006, FR-007, FR-011, FR-012 — research.md answers every FR: complete option table, one recommendation, security model, roadmap with "Input for /feature" under step 1, R9 phone journeys with tap counts (SC-004); claims still marked `[verify in Qn]`
- Checks: none (docs only)
- Reviewers: —

## WP-03 Phone experiment (manual / needs user)
- Phase: 3 Verify
- Tasks: T007, T008, T009, T010, T011, T012
- Files: specs/008-mobile-access-analysis/experiment/README.md, (screenshots in specs/008-mobile-access-analysis/experiment/, viewed by a subagent)
- Depends on: WP-01, WP-02
- Parallel group: 3A
- Acceptance: SC-004, SC-005, FR-007 — MANUAL / NEEDS USER (Tailscale on the personal Mac and iPhone, HTTPS certs on). Q1–Q6 recorded with setup time; preview only, never port 7420; `tailscale serve reset` and preview stopped at the end. T008–T011 are sequential (one phone session, one notes file). If the user skips it, drop this package and run WP-04 on its skip path
- Checks: none (docs only); preview via `scripts/preview.sh`
- Reviewers: —

## WP-04 Resolve verify markers
- Phase: 3 Verify
- Tasks: T013
- Files: specs/008-mobile-access-analysis/research.md
- Depends on: WP-02, WP-03 (or WP-02 alone if Phase 3 is skipped)
- Parallel group: 3B
- Acceptance: FR-007 — no `[verify in` marker remains: each is "verified (Qn)" or a correction; if skipped, "from docs, unverified"; contradictions of R1–R9 revised and flagged at the top of research.md
- Checks: grep `\[verify in` in research.md returns nothing
- Reviewers: —

## WP-05 docs/vision.md mobile section
- Phase: 4 Docs
- Tasks: T014
- Files: docs/vision.md
- Depends on: WP-04
- Parallel group: 4A
- Acceptance: FR-008 — "Mobile, through Tailscale" section rewritten with the decided approach, linked to research.md, answered open questions removed
- Checks: none (docs only); link resolves
- Reviewers: —

## WP-06 Decision 79
- Phase: 4 Docs
- Tasks: T015
- Files: docs/decisions.md
- Depends on: WP-04
- Parallel group: 4A
- Acceptance: FR-002, FR-008 — decision 79 added at the end (why, local-vs-remote rule, ruled out); last line of decision 12 points to 79
- Checks: none (docs only)
- Reviewers: —

## WP-07 docs/status.md mobile line
- Phase: 4 Docs
- Tasks: T016
- Files: docs/status.md
- Depends on: WP-04
- Parallel group: 4A
- Acceptance: FR-008 — the `- **Mobile access:**` line replaced with "analysed (decision 79, research); next: roadmap step 1, device auth"
- Checks: none (docs only)
- Reviewers: —

## WP-08 Docs link check
- Phase: 4 Docs
- Tasks: T017
- Files: docs/architecture.md (read only), README.md (read only), docs/vision.md, docs/decisions.md, docs/status.md (link check)
- Depends on: WP-05, WP-06, WP-07
- Parallel group: 4B
- Acceptance: FR-008, FR-009 — architecture.md and README.md need no change; links added by WP-05..07 resolve
- Checks: none (docs only)
- Reviewers: —

## WP-09 Close
- Phase: 5 Close
- Tasks: T018, T019, T020
- Files: specs/008-mobile-access-analysis/checklists/security.md, version files via scripts/bump-version.sh (T019, orchestrator), PR text (T020, orchestrator, not a file)
- Depends on: WP-08
- Parallel group: 5A
- Acceptance: SC-006, FR-009 — T018 (implementer): security checklist items re-marked after T013, experiment changes noted. T019 (ORCHESTRATOR, not the implementer): `scripts/bump-version.sh patch --commit` from origin/main, commit `chore: bump version to X.Y.Z`. T020 (ORCHESTRATOR): PR text via pr-writer, in English, stating patch and why; the user pushes
- Checks: none (docs only)
- Reviewers: —
