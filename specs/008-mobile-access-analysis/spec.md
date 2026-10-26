# Feature Specification: Mobile access analysis (Delilah from the phone, no app store)

**Feature Branch**: `008-mobile-access-analysis`

**Created**: 2026-10-26

**Status**: Draft

**Input**: User description: "Análisis Versión Mobile. Mi idea es una web view o algo, que no tenga que subir la app a la store ya que no tengo cuenta de dev. No quiero implementar nada, solo analizar cómo hacerlo."

This feature is an **analysis**, not a build. Its deliverable is a decision-ready write-up
plus updated project docs, so that a later feature can build mobile access without
re-opening the basic questions. It starts from the existing sketch in
[docs/vision.md](../../docs/vision.md) ("Mobile, through Tailscale") and decision #12 in
[docs/decisions.md](../../docs/decisions.md) (loopback-only, no authentication), and either
confirms or replaces them.

The stories below have two layers:
- **US1–US2** are about the analysis itself (what the user reads and decides).
- **US3–US6** describe the mobile experience the recommended option must be able to deliver.
  They are the yardstick each option is judged against, not work to do now.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Pick a way to reach Delilah from the phone (Priority: P1)

The user reads one document that lists the realistic ways to use Delilah from a phone
without publishing to the App Store or Play Store (at least: the mobile browser on a
private network, an installable home-screen web app, a self-signed or sideloaded native
wrapper, and a hosted relay). For each it states what it costs, what it needs on the Mac
and the phone, what works and what does not (live updates, notifications, terminal,
background), and how safe it is. It ends with one recommended option and why the others
were ruled out.

**Why this priority**: it is the question the task asks; nothing else can be planned until
it is answered.

**Independent Test**: hand the document to the user; they can say which option they would
build and why, without asking anything that the document should have answered.

**Acceptance Scenarios**:

1. **Given** the analysis, **When** the user reads the options table, **Then** every option
   says whether it needs a paid developer account, a store review, or a third-party service,
   and which phone platforms it works on.
2. **Given** the analysis, **When** the user reaches the recommendation, **Then** it names a
   single option, lists the trade-offs accepted, and links each ruled-out option to the
   reason it lost.
3. **Given** the recommendation, **When** the user checks it against the existing vision
   sketch, **Then** the document says explicitly what it keeps, changes or drops from that
   sketch.

---

### User Story 2 - Know what has to change and in what order (Priority: P1)

The user sees, for the recommended option, what has to change in Delilah before a phone can
connect safely: how the daemon becomes reachable from outside the Mac, how a phone proves it
is allowed (pairing and revoking), how the UI reaches the phone, and how the screens adapt to
a small display. The changes are grouped into ordered, independently shippable steps with a
rough size, so a later feature can pick them up one by one.

**Why this priority**: the security model is the real blocker named in decision #12; without
an ordered path the analysis is not actionable.

**Independent Test**: a later `/feature` run can take the first step of the roadmap as its
input and specify it without re-doing this analysis.

**Acceptance Scenarios**:

1. **Given** the roadmap, **When** the user reads step 1, **Then** it is shippable on its own
   and leaves today's desktop behaviour and security unchanged when mobile access is off.
2. **Given** the security section, **When** the user reviews it, **Then** it states who can
   reach the daemon, how a device is paired, how it is revoked, what a stolen phone can do,
   and what stays desktop-only.
3. **Given** the docs, **When** the analysis is approved, **Then** `docs/vision.md`,
   `docs/decisions.md` and `docs/status.md` reflect the outcome (decision recorded with what
   was ruled out; status no longer says "waiting for the user's answers" for the answered
   questions).

---

### User Story 3 - Answer an agent from the phone (Priority: P2, yardstick)

Away from the Mac, the user gets a notification that an agent is waiting (a permission, a
question, a plan to approve), opens Delilah on the phone, reads enough context, and answers.
The agent continues on the Mac.

**Why this priority**: it is the main reason to want mobile access (see, answer, approve).

**Independent Test**: judged per option in the analysis: can this option deliver it, and with
what limits (e.g. notifications only while the page is open)?

**Acceptance Scenarios**:

1. **Given** an agent waiting for input, **When** the user is on the phone, **Then** the
   analysis says how (and whether) they are told, and how many taps it takes to answer.

---

### User Story 4 - Glance at the state of work (Priority: P2, yardstick)

The user opens Delilah on the phone and sees the inbox (Bandeja), the tasks and their
phases, and which agents are running, updating live while the screen is on.

**Independent Test**: judged per option: does live update survive the phone locking and
unlocking, and switching between Wi-Fi and mobile data?

**Acceptance Scenarios**:

1. **Given** the phone was locked for 10 minutes, **When** the user unlocks it on the open
   page, **Then** the analysis says how the view catches up and how long that takes.

---

### User Story 5 - Follow an agent's output (Priority: P3, yardstick)

The user opens a running session on the phone and follows its conversation or terminal output
without resizing the session on the Mac.

**Independent Test**: judged per option and per view (conversation vs. raw terminal): readable
and usable on a phone-width screen, or explicitly desktop-only.

**Acceptance Scenarios**:

1. **Given** a session open on both the Mac and the phone, **When** the phone shows it,
   **Then** the analysis states whether the Mac's view is affected.

---

### User Story 6 - Launch work from the phone (Priority: P3, yardstick)

From the phone, the user creates a task (from an idea or a ticket), starts its agent, or moves
a task to the next phase; the work runs on the Mac.

**Independent Test**: judged per option: which launch actions are offered on the phone and
which stay desktop-only, and why.

**Acceptance Scenarios**:

1. **Given** a new idea on the phone, **When** the user creates a task and starts it,
   **Then** the analysis says how many steps it takes and what the phone shows while the agent
   starts.

### Edge Cases

- The Mac is asleep, off, or the daemon is not running: what the phone shows.
- The phone is lost or stolen: how the user cuts it off, and what it could do until then.
- The phone is on a network the Mac cannot be reached from (mobile data, hotel Wi-Fi).
- Two devices (phone and tablet) paired at the same time.
- The work Mac: company policy may forbid VPNs or tunnels; the analysis must say what still
  works without them, or that mobile is personal-Mac-only.
- An agent shows content from a repo or ticket on the phone: the phone must not become a way
  to run that content.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The analysis MUST compare at least four access options (private-network
  browser, installable home-screen web app, sideloaded/self-signed native wrapper, hosted
  relay) on: cost, developer account or store needed, phone platforms, setup steps, live
  updates, push notifications, behaviour in background, behaviour when the Mac is asleep, off
  or unreachable, terminal usability, security exposure.
- **FR-002**: The analysis MUST recommend exactly one option and record the ruled-out ones
  with their reason.
- **FR-003**: The analysis MUST define the security model for the recommended option:
  reachability (never the public internet unless explicitly justified), device pairing,
  revocation, session lifetime, what a paired device may and may not do, and how today's
  protections (origin and host checks) are kept or replaced. It MUST also cover:
  - launch actions (start an agent, create a task, move a phase) run code on the Mac, so the
    model says what extra guard they need from the phone (confirmation, re-authentication);
  - content an agent shows (from a repo, ticket or web page) is displayed on the phone as
    text only and never becomes a command or link that acts on the Mac by itself;
  - what a notification may show on a locked phone (work content on the lock screen).
- **FR-004**: The analysis MUST define the mobile scope: which screens and actions are
  available on the phone and which stay desktop-only, with destructive actions desktop-only
  by default.
- **FR-005**: The analysis MUST give an ordered roadmap of independently shippable steps,
  each with a rough size and a statement that mobile off means today's behaviour.
- **FR-006**: The analysis MUST state, for each option, what works on iPhone and on Android
  separately where they differ (e.g. notifications for home-screen web apps).
- **FR-007**: The analysis MUST be checked against the current code (how the daemon is
  reached, how the UI finds it, how live updates and the terminal travel) and cite the places
  that would change, without changing them.
- **FR-008**: The project docs MUST be updated: the mobile section of `docs/vision.md`,
  a new decision in `docs/decisions.md` (with what was ruled out), and the mobile line in
  `docs/status.md`.
- **FR-009**: No production code, migration or API contract MUST change in this feature;
  any prototype or measurement made to support the analysis lives outside the shipped code
  and is described in the write-up.
- **FR-010**: The analysis MUST optimise for **iPhone** (iOS Safari and home-screen web apps):
  push only for installed home-screen apps (iOS 16.4+), background and WebSocket behaviour on
  lock, and the sideloading limits without a paid account. Android is noted only where it
  changes the recommendation.
- **FR-011**: The analysis MUST cover **both the personal and the work Mac**: what works on
  the work Mac if IT does not allow a VPN or tunnel (e.g. Tailscale), what the fallback is
  (same-Wi-Fi only, or nothing), and must not depend on anything from the work repos. For each
  option it MUST state what data leaves the work Mac and through which third parties
  (coordination servers, relays, push services).
- **FR-012**: The mobile scope MUST cover **seeing, answering and launching work**: inbox,
  permissions, questions, plan approvals, plus creating a task, starting an agent and moving a
  phase. Destructive actions (delete, discard a workspace, force operations) stay
  desktop-only unless the analysis justifies otherwise.

### Key Entities

- **Access option**: one way of reaching Delilah from the phone; its requirements,
  capabilities, limits, cost and risk.
- **Paired device**: a phone or tablet the user allowed; name, when paired, last seen,
  what it may do, revocable.
- **Mobile scope**: the set of screens and actions offered on the phone, versus
  desktop-only.
- **Roadmap step**: an ordered, shippable unit of later work, with size and dependencies.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: The user can choose an option after one read of the analysis, with zero
  questions the document should have answered (checked when approving it).
- **SC-002**: 100 % of the options in the table have every comparison column filled, with a
  source or a test for each "works / does not work" on notifications and background behaviour.
- **SC-003**: The first roadmap step can be turned into a spec by `/feature` with no other
  input than the step text and the analysis.
- **SC-004**: The recommended option lets the user answer a waiting agent from the phone in
  under 30 seconds from the notification (as estimated in the analysis, and verified later
  when built).
- **SC-005**: The setup the recommended option asks of the user (Mac + phone) takes under
  15 minutes and is a one-time effort.
- **SC-006**: With mobile access turned off, the security exposure of the daemon is the same
  as today (stated and justified in the security section).

## Assumptions

- No paid Apple or Google developer account, and no store publishing; anything needing them
  is ruled out or listed only for completeness.
- The daemon stays the single source of truth and the phone is one more client of its API,
  as the architecture already intends (the web UI already runs without the desktop shell).
- The Mac is the host: when it sleeps or is off, mobile access is unavailable; keeping it
  awake is out of scope beyond noting it.
- Single user: the user's own devices only, no sharing with other people.
- The analysis may include a throwaway experiment (e.g. opening a preview on the phone) to
  confirm claims, kept out of shipped code.
- Write-up language matches the project docs (English); conversation with the user may be
  in Spanish.
