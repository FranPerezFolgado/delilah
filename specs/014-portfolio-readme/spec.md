# Feature Specification: Portfolio README and personal-project tone

**Feature Branch**: `014-portfolio-readme`

**Created**: 2026-11-01

**Status**: Draft

**Input**: User description: "Delilah stops being positioned as an open source product: it is a portfolio project (show that I am a product builder) and a personal tool. Rewrite the README to sell the project rather than document it, at most ~200 lines, in English. Adjust the CONTRIBUTING tone (personal project, issues and PRs welcome, no support commitment) and assess whether dependabot brings more noise than value (propose, do not remove without asking)." (LIBRE-33)

## User Scenarios & Testing *(mandatory)*

### User Story 1 - A visitor understands what was built and why it is impressive (Priority: P1)

Someone evaluating the author (a hiring manager, a founder, another engineer) lands on the repository page. Within a minute of reading and scrolling they know what Delilah does, which real problem it solves, how it is designed and how it was built, and they come away with a view of the author as a product builder.

**Why this priority**: This is the reason for the change; the README is the portfolio piece.

**Independent Test**: Give the rendered README page to a reader who has never seen the project and ask them to state the problem, three things it does, and how it was built.

**Acceptance Scenarios**:

1. **Given** the repository page, **When** the visitor reads the first screen, **Then** they see a one-line description and a featured visual of several agents working at once.
2. **Given** the README, **When** the visitor reads it top to bottom, **Then** the sections appear in this order: one line and visual; the problem; what it does (6-8 bullets); architecture diagram; 4-5 key decisions; how it was built, with figures; status; install, documentation links, licence and disclaimer.
3. **Given** the decisions section, **When** the visitor reads it, **Then** at least one of the decisions shown is an option that was ruled out, and each decision links to its full record.
4. **Given** the "how it was built" section, **When** the visitor reads a figure, **Then** it can be reproduced from the repository at the stated revision.

---

### User Story 2 - A visitor who wants to run it can still install it (Priority: P2)

A visitor who is curious enough to try Delilah finds working install instructions on the same page, or one link away, without the install detail dominating the page.

**Why this priority**: The install path works today and must not regress, but it is no longer what the page is for.

**Independent Test**: Follow only what the README gives (directly or through its links) to reach each install route that exists today.

**Acceptance Scenarios**:

1. **Given** the new README, **When** a visitor looks for how to install, **Then** the one-line install command and the macOS and Claude Code requirements are on the page.
2. **Given** content moved out of the README, **When** a visitor follows the link, **Then** the manual download, unsigned-app explanation, verification, build-from-source and update and uninstall instructions are all still reachable, with their meaning unchanged.
3. **Given** any existing link to a README section, **When** it is followed after the rewrite, **Then** it still lands on relevant content.

---

### User Story 3 - A would-be contributor knows what to expect (Priority: P3)

Someone who wants to file an issue or send a pull request learns that this is a personal project: contributions are welcome, and there is no commitment to support, response times or a roadmap.

**Why this priority**: It sets honest expectations, and it is a small change.

**Independent Test**: Read CONTRIBUTING and any other contributor-facing text and check that none of it promises more than the personal-project stance.

**Acceptance Scenarios**:

1. **Given** CONTRIBUTING, **When** a contributor opens it, **Then** the first paragraph says it is a personal project, that issues and PRs are welcome and that there is no support commitment.
2. **Given** CONTRIBUTING, **When** a contributor needs the practical rules (previews, checks, commits, versioning, PR process), **Then** they are still there, unchanged in substance.
3. **Given** the README and contributor-facing files, **When** they are compared, **Then** they do not contradict each other on the project's stance.

---

### User Story 4 - The owner decides about dependabot with evidence (Priority: P3)

The owner receives a written assessment of what dependabot has produced in this repository so far, with a concrete proposal, and decides. Nothing changes until they do.

**Why this priority**: It only costs a decision, but it must not happen by itself.

**Independent Test**: Check that the assessment exists and that the dependabot configuration is byte-identical to `main` unless the owner approved a change.

**Acceptance Scenarios**:

1. **Given** the work is finished, **When** the owner reads the proposal, **Then** it gives the PR counts by state, the current schedule, and a recommended change with its trade-off.
2. **Given** no explicit approval from the owner, **When** the branch is compared with `main`, **Then** the dependabot configuration is unchanged.

---

### Edge Cases

- A GIF cannot be produced with reasonable tooling or quality: the page uses still images, and the instructions to record the GIF are included for the owner.
- Figures go stale as the project moves on: they state the revision or date they were taken at, so they stay true.
- A number could mislead (test-attribute counts are not test runs; lines include tests, and specs and docs are not code): each figure says what it counts, and the areas are kept separate.
- Decision records have numbering collisions (49 and 50 appear twice): the README counts entries rather than quoting the highest number, and does not link to an ambiguous number.
- The brief links `docs/project-state.md`, which does not exist: the status section links `docs/status.md`.
- GitHub light and dark themes: the visual and the diagram stay readable in both.
- The Jira and submodule problem statement could expose the employer's setup: it describes the problem generically, with no work repository, company or ticket names.

## Requirements *(mandatory)*

### Functional Requirements

**README**

- **FR-001**: The README MUST be in English and at most about 200 lines.
- **FR-002**: The README MUST follow the section order in User Story 1, scenario 2.
- **FR-003**: The first screen MUST show a one-line description and a featured visual of several agents working in parallel, as an animation if viable or stills otherwise.
- **FR-004**: The visual MUST show only fixture or demo data, with nothing from real or private repositories.
- **FR-005**: The problem section MUST cover tickets in Jira, parent repositories with submodules, running agents in parallel and subscription usage limits.
- **FR-006**: The "what it does" section MUST have 6-8 bullets, each describing something the product does today.
- **FR-007**: The architecture section MUST contain a diagram rendered by the repository host (not an image) showing the API-first daemon, how agents are driven (terminal and stream), hooks, the database and the desktop app.
- **FR-008**: The decisions section MUST present 4-5 decisions from the decision record, at least one of them ruled out, each with a link to the record.
- **FR-009**: The "how it was built" section MUST give figures for elapsed days, commits, lines per area, specs, decisions and tests, each computed from the repository, with what it counts and the revision or date it was taken at.
- **FR-010**: The status section MUST link the current status document.
- **FR-011**: The install section MUST keep the one-line install command and the requirements on the page; every other install route that exists today MUST remain reachable, with its content unchanged in meaning.
- **FR-012**: The README MUST link the documentation, the licence, and keep the disclaimer of non-affiliation with Anthropic.
- **FR-013**: Every claim of fact in the README MUST be true of the current product; it MUST NOT describe pending features as built.

**Tone**

- **FR-014**: CONTRIBUTING MUST state that this is a personal project, that issues and PRs are welcome and that there is no support commitment, and it MUST keep its practical rules.
- **FR-015**: Contributor-facing and public text outside CONTRIBUTING MUST NOT promise support beyond that stance. Where it does (such as a response time), the change is limited to that wording; how to report a vulnerability stays intact.

**Dependabot**

- **FR-016**: The work MUST produce a written dependabot assessment and a recommendation for the owner.
- **FR-017**: The dependabot configuration MUST NOT change without the owner's explicit approval.

**Project records**

- **FR-018**: The decision record MUST capture the new positioning of the project and of the README, and documents that describe the README as a newcomer page MUST agree with it.

**Process**

- **FR-019**: The outline of the README, with the computed figures, MUST be presented to the owner for approval before the README is written.
- **FR-020**: Install and release tooling and its behaviour, product features and the version number MUST NOT change.

### Key Entities

- **README outline**: the proposed sections, bullets, decisions and figures; what the owner approves before implementation.
- **Project figures**: a set of counts, each with a definition and the revision it was taken at.
- **Dependabot proposal**: PR counts by state, the current schedule, the recommendation and its trade-off.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: A first-time reader can state the problem, three capabilities and how the project was built after no more than 60 seconds on the page.
- **SC-002**: The README has no more than about 200 lines, against 135 today, of which at most a quarter are about installing.
- **SC-003**: 100% of the figures on the page can be reproduced from the repository at the stated revision.
- **SC-004**: 100% of relative links and anchors in the README and CONTRIBUTING resolve.
- **SC-005**: Every install route that exists before the change is reachable from the README in at most one link.
- **SC-006**: The diff touches no install or release tooling, no product code, and, without approval, not the dependabot configuration.
- **SC-007**: The featured visual and the diagram are readable in the host's light and dark themes.

## Assumptions

- The audience, in order: people evaluating the author, curious developers, potential contributors.
- The repository stays public and MIT-licensed; `SECURITY.md` and `CODE_OF_CONDUCT.md` stay, with only the support wording adjusted if needed.
- Install detail that leaves the README moves to `docs/usage.md`, the existing reference.
- Screenshots may be regenerated from the existing end-to-end screen tests with their demo data; a capture-only change to them is acceptable.
- The figures in this spec were taken at `e336e24` (v1.3.0); they are recomputed if `main` has moved when the README is written.
- Installing new tools on the machine to produce a GIF is not assumed; if tools already available cannot do it, the fallback applies.
