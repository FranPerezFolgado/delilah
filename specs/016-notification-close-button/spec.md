# Feature Specification: Close button on every notification

**Feature Branch**: `016-notification-close-button`

**Created**: 2026-11-26

**Status**: Draft

**Input**: User description: "Las notificaciones, TODAS, deben tener botón de X." (task LIBRE-45; brief in the task notes)

## Definitions

- **Notification**: a message the app shows on its own initiative to tell the user about a state or event — a toast, a bar above the main views, or a banner inside a screen. It does not need an answer to keep working.
- **Not a notification** (out of scope): inline form and field errors, the Inbox question and permission cards (they need an answer) and the other Inbox items with their own "Descartar" action (the Inbox is a list to work through, not a notification), the status-bar connection indicator, and native macOS notifications (the system already gives them a close control).
- **Close button (X)**: the same icon-only button everywhere, with the design system's close icon and the accessible name "Cerrar".
- **Hidden until it changes**: closing a bar hides it in this app session only; it comes back by itself when what it reports changes (see FR-006). Closing never changes the real state behind it unless a requirement says so.

## Inventory of notifications

| # | Notification | Where | Has a close control today |
|---|---|---|---|
| N1 | Toasts (info, error, wait…) | bottom-left stack | text "×" |
| N2 | Usage-limit bar, one per window (5 h, week) | above the main views | no |
| N3 | Autopilot-paused bar, one per task, and the "y N más" bar | above the main views | no |
| N4 | Update bar: checking (manual), up to date, available, downloading, installing, error, confirm sessions, waiting | above the main views | text "Cerrar" in up to date and error only |
| N5 | "Sin conexión con delilahd" banner | over the main area | no |
| N6 | Base-stale bars | task screen, above the tabs | text "Descartar" |
| N7 | Project setup banner (running, failed) | task screen | no |
| N8 | Autopilot-paused banner with resume / manual | task screen, handoff actions | no |
| N9 | Task status bars (paused, error, waiting for you, other) | task screen, task view | no |
| N10 | "N subtareas nuevas de Jira" banner | task screen, subtasks | no |
| N11 | PR problem notice, base warnings (subtask commits, wrong base), "ready to leave draft" hint | task screen, PR tab | no |
| N12 | "Jira rechazó el token (401)" banner | Jira view | no |

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Close any global bar (Priority: P1)

A user sees a bar above the main views (usage limit, autopilot paused, update) that they have already read. They click its X and the bar goes away, so the view gets its space back, without the underlying state changing.

**Why this priority**: These bars sit above every screen and today most of them cannot be closed at all; they are the main source of the complaint.

**Independent Test**: With a preview whose usage is above the threshold and a task with autopilot paused, each bar shows an X; clicking it hides that bar only, and the task stays paused.

**Acceptance Scenarios**:

1. **Given** the usage-limit bar for the 5 h window is showing at 92 %, **When** the user clicks its X, **Then** the bar disappears and other bars stay.
2. **Given** that bar was closed at 92 %, **When** the window reaches 100 % (limit reached), **Then** the bar shows again.
3. **Given** that bar was closed, **When** usage falls below the threshold and later rises above it again, **Then** the bar shows again.
4. **Given** an autopilot-paused bar for task A, **When** the user closes it, **Then** it disappears, task A stays paused, and bars for other tasks stay.
5. **Given** the autopilot-paused bar for task A was closed, **When** task A pauses again with a different reason (or resumes and pauses again), **Then** the bar shows again.
6. **Given** the "y N más" bar, **When** the user closes it, **Then** it disappears until the number of extra paused tasks changes.

---

### User Story 2 - Close the update bar in every state (Priority: P1)

A user sees the update bar in any of its states and can close it with the same X, getting the behaviour that matches the state.

**Why this priority**: The update bar is the other global bar; half of its states have no way out today.

**Independent Test**: Drive the update flow through each state in a component test and check that each one has an X with the expected effect.

**Acceptance Scenarios**:

1. **Given** the bar says Delilah is up to date, or shows an update error, **When** the user clicks X, **Then** the bar closes exactly as the old "Cerrar" button did, and that text button is no longer shown.
2. **Given** a new version is available, **When** the user clicks X, **Then** it has the same effect as "Más tarde" (that version is not offered again), and "Más tarde" is still shown.
3. **Given** the bar asks to confirm stopping running sessions, **When** the user clicks X, **Then** it has the same effect as "Cancelar" (back to the offer), and "Cancelar" is still shown.
4. **Given** the bar is checking, downloading, installing or waiting for sessions to finish, **When** the user clicks X, **Then** the bar hides but the check, download, install or wait goes on.
5. **Given** the bar was hidden while downloading, **When** the flow moves to another state (installing, error…), **Then** the bar shows again.

---

### User Story 3 - Toasts keep their close, now with the shared X (Priority: P2)

Toasts already close; they now use the same X as every other notification, and nothing else about them changes.

**Why this priority**: Consistency only; behaviour is already there.

**Independent Test**: Push a toast, check it has the shared X named "Cerrar", click it, and check the toast's owner is told it was dismissed.

**Acceptance Scenarios**:

1. **Given** a toast is showing, **When** the user clicks its X, **Then** it closes and its dismiss handler runs, as today.
2. **Given** several toasts, **Then** "Cerrar todo" is still offered and works as today.
3. **Given** an info toast, **Then** it still closes on its own after its time.

---

### User Story 4 - Close banners inside the task screen and the Jira view (Priority: P2)

A user working on a task sees banners in its screen (base stale, setup, autopilot paused, task status, new Jira subtasks, PR warnings) or in the Jira view (token rejected) and can close each with the same X.

**Why this priority**: Less intrusive than global bars, but the request is "TODAS".

**Independent Test**: Render each screen with the state that shows each banner, close it, and check it hides until its state changes.

**Acceptance Scenarios**:

1. **Given** a base-stale bar, **When** the user clicks X, **Then** the notice is dismissed the same way "Descartar" did (it stays dismissed after a reload), and the "Descartar" text button is no longer shown; "Actualizar rama" stays.
2. **Given** the project setup is running or has failed, **When** the user closes the setup banner, **Then** it hides, the setup is not cancelled or retried, and it shows again when the setup state changes (e.g. running → failed).
3. **Given** the autopilot-paused banner in the task screen, **When** the user closes it, **Then** it hides and the autopilot stays paused; it shows again when the pause reason or pause time changes.
4. **Given** a task status bar (paused, error, waiting for you…), **When** the user closes it, **Then** it hides until the task's status changes.
5. **Given** "N subtareas nuevas de Jira", **When** the user closes it, **Then** it hides until the number of new subtasks changes.
6. **Given** a PR problem notice, base warning or ready-to-leave-draft hint, **When** the user closes it, **Then** it hides until what it reports changes (another problem kind, another commit count, another base).
7. **Given** the Jira token-rejected banner, **When** the user closes it, **Then** it hides, the Jira data stays marked as stale, and the banner shows again the next time the token is rejected after a successful load.
8. **Given** the user opens another task and comes back, **Then** banners closed in the task screen may show again (closing is not remembered across screens).

---

### User Story 5 - Close the disconnected banner (Priority: P3)

When delilahd is unreachable, the user can close the "Sin conexión" banner to read what is behind it; the app stays greyed and the status bar still shows the lost connection.

**Why this priority**: The banner is deliberately blocking; closing it is a convenience.

**Independent Test**: Simulate a lost connection, close the banner, check the content stays disabled; reconnect and lose it again and check the banner is back.

**Acceptance Scenarios**:

1. **Given** the connection is lost and the banner shows, **When** the user clicks X, **Then** the banner hides, the content stays greyed and disabled, retries continue, and "Reintentar ahora" is no longer reachable from the banner.
2. **Given** the banner was closed, **When** the connection comes back and is lost again, **Then** the banner shows again.

### Edge Cases

- A bar whose state changes while it is hidden and then changes back to the same value it had when closed (e.g. 92 % → 100 % → 92 %): it shows again, because a change happened in between.
- Several bars of the same kind (one per window, one per task): closing one hides only that one.
- The X must not take the place of the bar's own actions (Abrir, Actualizar, Reintentar, Reanudar…); they stay, and the X goes last.
- Keyboard: the X is reachable with Tab and activates with Enter/Space, and after closing, focus does not get lost off-screen (it moves to the next focusable element).
- Long texts: the X stays visible and aligned at the end of the bar even when the text is truncated.
- While delilahd is unreachable the rest of the app is disabled, so the global bars' X cannot be used until the connection is back; only the disconnected banner's X works then.
- Reloading the app (or the window) forgets every in-memory close; persisted dismissals (base-stale notices) stay.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: Every notification in the inventory (N1–N12) MUST show a close button (X).
- **FR-002**: The close button MUST look and behave the same everywhere: the design system's close icon, icon-only, accessible name "Cerrar", placed at the end of the notification after its other actions.
- **FR-003**: Closing a notification MUST NOT change the state it reports, except where FR-004 says otherwise.
- **FR-004**: Where a notification already had its own close-like action, the X MUST do exactly that action: toasts (dismiss), update up to date and error ("Cerrar"), update available ("Más tarde"), update confirm sessions ("Cancelar"), base-stale bars ("Descartar"). The text buttons "Cerrar" (update bar) and "Descartar" (base-stale bars) MUST be removed; "Más tarde" and "Cancelar" MUST stay.
- **FR-005**: For every other notification, closing MUST only hide it, in memory, for the current app session.
- **FR-006**: A hidden notification MUST show again when what it reports changes:
  - usage-limit bar: when its window changes between "near the limit" and "limit reached", or leaves the threshold and comes back;
  - autopilot-paused bars: when the task's pause reason or pause time changes; "y N más": when N changes;
  - update bar (checking, downloading, installing, waiting): when the update state changes kind;
  - disconnected banner: on the next loss of connection after a reconnection;
  - setup banner: when the setup state changes (running ↔ failed, or a new run);
  - task-screen autopilot banner: when pause reason or time changes;
  - task status bars: when the task's status changes;
  - new Jira subtasks: when the count changes;
  - PR notices: when the problem kind, the commit count, or the base changes;
  - Jira token banner: on the next rejection after a successful load.
- **FR-007**: Closing one notification MUST NOT hide others, including others of the same kind.
- **FR-008**: Toasts MUST keep "Cerrar todo", the info toasts' auto-close, and their dismiss handlers exactly as today.
- **FR-009**: Closing the disconnected banner MUST leave the content greyed and actions disabled while the connection is down.
- **FR-010**: The close button MUST be operable by keyboard and announced by screen readers by its name.
- **FR-011**: All new user-facing text MUST go through the app's translation layer like the rest of the UI.

### Key Entities

- **Notification**: a message with a kind (toast, global bar, in-screen banner), a text, optional actions and a close button.
- **Dismissal**: the fact that the user closed a notification, tied to the value of what it reported at that moment; it stops applying as soon as that value changes. In memory only, except the existing persisted base-stale dismissal.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: 100 % of the notifications in the inventory (12 of 12 kinds, every update state) can be closed with one click on the same X.
- **SC-002**: 0 notifications change the underlying state when closed, other than the five actions listed in FR-004.
- **SC-003**: Every notification that is closed and whose reported value then changes is visible again without user action.
- **SC-004**: Every notification has an automated UI check that its close button exists and has the expected effect.
- **SC-005**: There is exactly one visual style of close button across all notifications (no "×" characters or "Cerrar"/"Descartar" text buttons used as close).

## Assumptions

- In-memory dismissal (lost on reload) is enough; the brief says the bar hides "hasta que cambie su estado" and nothing asks for persistence.
- The disconnected banner gets an X that only hides it (open question in the brief, decided in autopilot: include it, since the app stays greyed and the status bar keeps showing the lost connection).
- The Inbox question and permission cards are not notifications: closing them without an answer would leave the agent waiting, so they are excluded (open question in the brief, decided in autopilot).
- During download or install, the X hides the bar without cancelling (open question, decided: there is no cancel for them today).
- "Más tarde" and "Cancelar" stay next to the X (open question, decided: they are named choices; the X maps to them).
- Native macOS notifications are out of scope (they have their own close).
- No daemon or HTTP API change: every dismissal is client-side, except base-stale notices, which keep the existing dismiss call.
- Banners closed inside a task screen may reappear when the screen is opened again; only the global bars keep their dismissal for the whole app session.
- Changes are tried in a preview (`scripts/preview.sh`), never against the installed daemon; no version bump.
