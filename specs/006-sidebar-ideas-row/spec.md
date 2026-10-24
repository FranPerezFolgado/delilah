# Feature Specification: One "Ideas" row per project in the sidebar

**Feature Branch**: `feature/LIBRE-14`

**Created**: 2026-10-23

**Status**: Draft

**Input**: User description: "Agrupar las conversaciones de idea en una sola entrada «Ideas» por proyecto en la barra lateral. Las pestañas de idea en la barra izquierda se acumulan; sería mejor tener «Ideas» y dentro todas las pestañas de idea arriba como ya salen."

## Why

Each idea conversation ("Nueva tarea con Claude") of a project is listed today as
its own row under the project in the sidebar. With use they pile up and push the
project's tasks down. The project's screen already shows every idea conversation as
a tab at the top, so the sidebar only needs a single way in.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - A single "Ideas" row instead of one row per idea (Priority: P1)

A project with several idea conversations shows, under the project name, one row
"Ideas" with the idea icon and how many idea conversations it has. The project's
other sessions (read-only "Preguntar a Claude", terminals) and its tasks are listed
as before.

**Why this priority**: It is the problem the user reported: the rows pile up.

**Independent Test**: With a project that has three idea conversations and one
read-only Claude session, the sidebar shows one "Ideas" row with "3" and one row for
the read-only session, and no row per idea.

**Acceptance Scenarios**:

1. **Given** a project with 3 idea conversations, **When** the sidebar shows the project unfolded, **Then** it shows exactly one "Ideas" row with the idea icon and the count 3, and no row per idea conversation.
2. **Given** a project with idea conversations and other sessions, **When** the sidebar shows it, **Then** the "Ideas" row comes first, then the other sessions as today, then the tasks.
3. **Given** a project with no idea conversations, **When** the sidebar shows it, **Then** there is no "Ideas" row.
4. **Given** a project with one idea conversation, **When** the sidebar shows it, **Then** the "Ideas" row is shown with the count 1.

---

### User Story 2 - Open the ideas from the row (Priority: P1)

Clicking "Ideas" opens the project's screen on one of its idea conversations; the
rest are the tabs at the top, as today.

**Why this priority**: Without it the grouped row is a dead end; together with
Story 1 it is the feature.

**Independent Test**: Click "Ideas" on a project with two idea conversations and
check that the project's screen opens on an idea tab, with both ideas as tabs.

**Acceptance Scenarios**:

1. **Given** a project where one idea conversation waits for the user, **When** the user clicks "Ideas", **Then** the project's screen opens on that conversation.
2. **Given** no idea waits for the user and the project's screen last showed an idea conversation that still exists, **When** the user clicks "Ideas", **Then** it opens on that conversation.
3. **Given** neither of the above, **When** the user clicks "Ideas", **Then** it opens on the newest idea conversation.
4. **Given** the project is folded in the sidebar, **When** it is folded, **Then** the "Ideas" row is hidden like the rest of the project's rows.

---

### User Story 3 - See that an idea waits for you (Priority: P2)

When any idea conversation of the project waits for the user (a question or a
proposal card), the "Ideas" row says so, the same way a folded project does.

**Why this priority**: Grouping must not hide that Claude is waiting; but the inbox
already lists it, so it ranks below the grouping itself.

**Independent Test**: With one of two idea conversations waiting, the "Ideas" row
shows the waiting highlight; with none waiting, it does not.

**Acceptance Scenarios**:

1. **Given** at least one idea conversation of the project waits for the user, **When** the sidebar shows the project, **Then** the "Ideas" row's count has the waiting highlight.
2. **Given** none waits, **When** the sidebar shows the project, **Then** the count has no highlight.

### Edge Cases

- Several ideas wait for the user: the first of them in the project's list opens.
- The last idea conversation shown on the project's screen was closed: it is not chosen; the newest remaining one opens.
- An idea whose task was already created still counts while the conversation is listed.
- The folded project's count and waiting highlight keep counting each idea conversation, as today.
- Idea conversations are project sessions only; task sessions are never ideas, so tasks are unaffected.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The sidebar MUST NOT show a row per idea conversation under a project.
- **FR-002**: The sidebar MUST show one "Ideas" row per project that has at least one idea conversation, and none for a project without them.
- **FR-003**: The "Ideas" row MUST show the idea icon, the text "Ideas" (translated per language) and the number of the project's idea conversations.
- **FR-004**: The "Ideas" row MUST be the first row under the project, before its other sessions and its tasks.
- **FR-005**: The project's other sessions (read-only Claude, Claude, terminals) MUST keep their own rows, unchanged.
- **FR-006**: The "Ideas" row's count MUST carry the waiting highlight when any of the project's idea conversations waits for the user.
- **FR-007**: Clicking the "Ideas" row MUST open the project's screen on an idea conversation tab, chosen in this order: the first that waits for the user; else the one the project's screen last showed, if it is an idea conversation still listed; else the newest.
- **FR-008**: The "Ideas" row MUST hide with the project when the project is folded, and has no fold of its own.
- **FR-009**: The "Ideas" row MUST be reachable and usable by keyboard and have an accessible name that includes the count.

### Key Entities

- **Idea conversation**: a project session marked as a conversation; has a state (waiting for the user or not). Listed in the order the daemon returns them, newest last.
- **Project screen's last tab**: what the project's screen showed last, remembered per viewer.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: A project with N idea conversations takes 1 sidebar row for them instead of N.
- **SC-002**: From the sidebar, any idea conversation is reachable in at most 2 clicks ("Ideas", then its tab).
- **SC-003**: Every case where an idea waits for the user is visible in the sidebar without unfolding anything beyond the project.

## Assumptions

- Only idea conversations are grouped; "Preguntar a Claude" sessions stay as rows (answer to the brief's open question).
- The row only navigates; it does not unfold a list in the sidebar.
- Closed conversations do not reach the sidebar's list today, so they do not count.
- The row is not marked as selected when the project's screen shows an idea; this is not required.
- Front-end only: no daemon or API change.
