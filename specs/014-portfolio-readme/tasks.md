# Tasks: Portfolio README and personal-project tone

**Input**: Design documents from `specs/014-portfolio-readme/`

**Prerequisites**: plan.md, spec.md, research.md, outline.md, dependabot-proposal.md, quickstart.md

**Tests**: No test tasks: the spec doesn't ask for them, and this is a docs change verified by `quickstart.md`. `app/e2e/screens.spec.ts` is a capture tool, skipped unless `DELILAH_SCREENS` is set.

**Format**: `[ID] [P?] [Story] Description`. `[P]` means it can run in parallel (different files, no dependency on an unfinished task).

**Two owner approvals are not self-approvable.** Autopilot covers the workflow phases, not T002 (FR-019) or T014 (FR-017). An owner approval must come from the owner, in the conversation or on the task. If nobody answers, stop and report; don't infer approval from autopilot or from a subagent.

## Phase 1: Setup and owner gate

- [X] T001 Check whether `main` has moved past `e336e24`. If so, rebase this branch on it and recompute every figure with the commands in `specs/014-portfolio-readme/quickstart.md`, updating `specs/014-portfolio-readme/outline.md`
- [X] T002 **GATE (FR-019)**: present `specs/014-portfolio-readme/outline.md` with its figures to the owner and wait for explicit approval, using AskUserQuestion with "Approve the outline (Recommended)" as the first option. Apply any changes they request to `outline.md`. Do not start T003 or later without this approval

**Checkpoint**: Outline approved by the owner.

## Phase 2: Foundational (hero images and install destination)

**Purpose**: The README links both. Captures (T003-T005) and the install move (T006) share no files, so they can run in parallel.

- [ ] T003 [P] Add a `Playground` screen as a new serial test at the end of `app/e2e/screens.spec.ts` that opens the view and captures it through `shot()`. Run it per `quickstart.md` to a scratch directory (`DELILAH_SCREENS_OUT`), with `DELILAH_SCREENS_LANG=en`. Inspect the result through the `ui-verifier` agent rather than opening images in the main thread. If the seed doesn't show several states (typing, reading, terminal, waiting, paused), add a `page.route` on `GET /api/v1/activity` inside that test only, with demo names (research R2). Run `cd app && npx tsc --noEmit`
- [ ] T004 Timeboxed to 45 minutes: attempt the GIF (research R3). Add `DELILAH_SCREENS_MOTION=1`, read by `open()` in `app/e2e/screens.spec.ts`, and a multi-frame screenshot loop in the Playground test that doesn't use `shot()`. Assemble frames with Pillow in a script in the session scratchpad, not committed. Accept only if it is under 5 MB, the characters visibly move and it loops cleanly. Otherwise revert the switch and the loop, and use the fallback: stills plus the recording instructions in `quickstart.md`. Without the env var, existing screens must behave as they do today
- [ ] T005 Write the final images to `docs/images/` as `playground-light.png` and `playground-dark.png`, plus `playground.gif` if T004 passed. Confirm that only demo data appears (FR-004) and record the outcome for the PR description. Remove `docs/images/board-*.png` only if the new README doesn't use them
- [X] T006 [P] In `docs/usage.md`, add `## Install` (manual download, why the app is unsigned, verify a download, moved from `README.md` lines 33-70 with meaning unchanged, including the links to decisions 79 and 80) and `## Getting started` (`README.md` lines 89-109), and add both to the Contents list at the top. `Building from source`, `Uninstall` and `Updates` already exist there. Don't touch `scripts/install.sh` or any release tooling

**Checkpoint**: Images exist and the install detail has a home.

## Phase 3: User Story 1 - A visitor understands what was built (Priority: P1) 🎯 MVP

**Goal**: The portfolio page.

**Independent Test**: A reader new to the project can state the problem, three capabilities and how it was built after the rendered page alone.

- [ ] T007 [US1] Rewrite `README.md` following the approved `outline.md`, in its section order: hero, problem, what it does, Mermaid architecture, decisions, how it was built, status, install and links. At most about 200 lines, in English. Use `<picture>` for the light and dark hero. Re-check each "what it does" bullet against `docs/usage.md` (FR-013), and check the Mermaid edges against `docs/architecture.md`. Describe decision 67 as adjusted by a spike, not ruled out, and name the decision number in each section link. Keep the problem statement generic: no work repository, company or ticket names. Link `docs/status.md`, not `docs/project-state.md`. Include the pixtuoid credit (research R4), MIT and the non-affiliation disclaimer. Keep an `## Install` heading with the requirements, the one-line command and a link to `docs/usage.md#install`
- [X] T008 [P] [US1] Add the "Portfolio positioning" decision to `docs/decisions.md`: the next free number (104 unless taken), what changed and why, and what was ruled out (keeping the README as a newcomer page). Add an inline italic note to decision 82, in the style of the "Superseded by 85" note on decision 58
- [X] T009 [P] [US1] In `docs/status.md`, record this feature and correct line 18, which describes the README as rewritten for newcomers

**Checkpoint**: The README stands on its own as a portfolio page.

## Phase 4: User Story 2 - A visitor can still install it (Priority: P2)

**Goal**: No install route lost.

**Independent Test**: From the README alone, reach every route that exists today in at most one link.

- [ ] T010 [US2] Walk through the routes from the old `README.md` (one-line script, `DELILAH_INSTALL_DIR`, manual DMG, quarantine and Open Anyway, verify, build from source, updates, uninstall, the optional CLI) and confirm each is reachable from the new `README.md` in at most one link with meaning unchanged, fixing `docs/usage.md` if not. Confirm that the line count of the install section is at most a quarter of the page (SC-002)

**Checkpoint**: Every install route is reachable.

## Phase 5: User Story 3 - A would-be contributor knows what to expect (Priority: P3)

**Goal**: Honest expectations, with no rule lost.

**Independent Test**: Nothing contributor-facing promises more than the personal-project stance.

- [X] T011 [P] [US3] In `CONTRIBUTING.md`, open with the stance: a personal project, issues and PRs welcome, no commitment to support, response times or a roadmap. Reword "Thanks for helping" (line 3) and "A maintainer reviews and squash-merges" (line 73) to match. Keep previews, checks, commits, versioning and PR rules unchanged in substance
- [X] T012 [P] [US3] In `SECURITY.md`, change line 9 only to a best-effort response with no time target. Keep the private reporting route, supported versions and threat model. Leave `CODE_OF_CONDUCT.md` and the issue and PR templates as they are (research R8)

**Checkpoint**: Stance is consistent across public files.

## Phase 6: User Story 4 - The owner decides about dependabot (Priority: P3)

**Goal**: A decision with evidence, and no change without it.

**Independent Test**: `git diff main -- .github/dependabot.yml` is empty unless the owner approved.

- [X] T013 [P] [US4] Refresh the counts and dates in `specs/014-portfolio-readme/dependabot-proposal.md` (`gh pr list --state all --author app/dependabot`), since a weekly run was due about 2026-11-02, and revise the assessment if the new data changes it. Confirm the `gh` account first. Re-check the vulnerability-alerts status, and say so if it can't be determined
- [ ] T014 [US4] **GATE (FR-017)**: present the proposal to the owner in the final report or PR description. Change `.github/dependabot.yml` only on their explicit approval, and only as the proposal's "If approved" section describes. No approval means no change, and this task is still complete. Don't change the repository's alert settings or touch the open dependabot PRs

**Checkpoint**: The proposal is delivered.

## Phase 7: Polish

- [ ] T015 [P] In `CLAUDE.md` line 24, replace "the README is a short newcomer page" with wording that matches the new positioning
- [ ] T016 Run every check in `specs/014-portfolio-readme/quickstart.md` ("Checks before the PR"): line count, the empty diffs, the `README|newcomer` grep, and all relative links and anchors in `README.md`, `CONTRIBUTING.md` and the new `docs/usage.md` sections. Have every figure in `README.md` match a fresh run of the figure commands at the revision it states. Fix what fails
- [ ] T017 Confirm the active `gh` account is `FranPerezFolgado`, push the branch and check on GitHub, in light and dark, that the Mermaid diagram renders and the hero reads well (via `ui-verifier`; SC-007)
- [ ] T018 Open the PR with the title `docs: rewrite README as a portfolio page and set personal-project tone`, with the GIF outcome and the dependabot proposal in the body. Don't bump the version. Merging is the owner's call, squash only

## Dependencies & Execution Order

- **T001 → T002** come first, and T002 blocks everything else.
- **Phase 2**: T003 → T004 → T005 (same spec file and images); T006 is independent of them.
- **T007** needs T005 (image names) and T006 (link target). T008, T009 and all of Phases 5-6 need only T002.
- **T010** needs T006 and T007.
- **T016 → T017 → T018** come last, after everything else.

### Parallel opportunities

After T002, run at once: T003 (then T004, T005), T006, T008, T009, T011, T012, T013, T015. Each touches different files. T007 then starts when T005 and T006 are done.

## Implementation Strategy

**MVP**: Phases 1-3 give the portfolio page. Stills are enough for it, so if T004 fails, nothing else waits. Phases 4-7 protect what exists and finish the tone and records. All of it ships in one PR, as one change.
