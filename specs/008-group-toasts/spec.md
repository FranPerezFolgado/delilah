# Feature Specification: Group toasts

**Feature Branch**: `008-group-toasts`

**Created**: 2026-10-26

**Status**: Draft

**Input**: User description (task LIBRE-18): "The toast notifications accumulate and are creating visual noise. Analyze how we can improve that and change it." Expanded: identical toasts merge with a count, «te espera» toasts become one grouped toast that follows the tasks' real state, the stack collapses to the newest toast with a «+N» indicator, errors persist but group like the rest, and every existing toast (including the «Deshacer» undo toasts) keeps working.

## Context: why toasts pile up today

- At most 3 toasts are on screen, but errors and «te espera» toasts never go away on their own, so after a while the 3 slots are full of stale items and every new toast pushes out an older one, sometimes the one that mattered.
- Every task that starts waiting pushes its own «{key} te espera» toast; with several agents running, the stack fills with them.
- The same message (e.g. the same daemon error after several attempts) shows up as separate toasts.
- A «te espera» toast stays after the user answered or opened that task; the Inbox and the sidebar already show it.
- All visible toasts are drawn full size, one above the other, covering the bottom-left of the window.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - One «te espera» toast that follows the tasks (Priority: P1)

While working in one task, several other tasks start waiting for an answer. Instead of one toast per task, the user sees a single toast: «LIBRE-9 te espera · Responder» when it is one task, «3 tareas te esperan · Ver» when there are several. Answering or opening a task removes it from that toast; when no waiting task is left, the toast disappears by itself.

**Why this priority**: It is the main source of accumulation (persistent, one per task, never cleaned up) and the one that hides the useful toasts.

**Independent Test**: With three tasks switching to «waiting for user» while another task is open, exactly one waiting toast is on screen with the count 3; answering two of them leaves «LIBRE-x te espera · Responder»; answering the last one removes the toast.

**Acceptance Scenarios**:

1. **Given** no waiting toast, **When** task A starts waiting while the user is elsewhere, **Then** one toast reads «A te espera» with «Responder», which opens task A.
2. **Given** that toast, **When** task B also starts waiting, **Then** the same toast now reads «2 tareas te esperan» with «Ver», which opens the Inbox; no second waiting toast appears.
3. **Given** the grouped toast for A and B, **When** A stops waiting (answered, by any means) or the user opens A's screen, **Then** the toast goes back to «B te espera · Responder».
4. **Given** the toast for only B, **When** B stops waiting, **Then** the toast disappears without the user closing it.
5. **Given** the grouped toast, **When** the user closes it, **Then** it stays closed for the tasks it contained; a task that starts waiting later brings it back with only the new task.
6. **Given** the user is on task A's screen, **When** A starts waiting, **Then** A is not added to the toast (unchanged from today).

---

### User Story 2 - Repeated messages merge (Priority: P2)

When the same message is raised again while its toast is still on screen (same kind and same text), the existing toast shows a repeat count («×3») instead of a new toast being added, and moves to the newest position.

**Why this priority**: Repeated errors are the second cause of noise and push out other toasts.

**Independent Test**: Raise the same error three times: one toast with «×3» is shown.

**Acceptance Scenarios**:

1. **Given** an error toast «No se pudo conectar», **When** the same error is raised twice more, **Then** one toast shows «No se pudo conectar» with «×3».
2. **Given** an info toast «Comando copiado» about to expire, **When** the same message is raised again, **Then** its count goes to «×2» and its 6-second timer starts again.
3. **Given** two toasts with the same text but different kind (info and error), **Then** they are not merged.
4. **Given** two toasts with the same text but each with its own action (e.g. two «Deshacer» toasts for different undos), **Then** they are not merged: each keeps its own action.

---

### User Story 3 - A collapsed stack (Priority: P2)

When more than one toast is present, only the newest is shown in full; the older ones are drawn as a compact «+N» indicator behind it. Hovering, focusing or clicking the stack expands it to show all toasts; leaving it collapses it again. A «Cerrar todo» action is available whenever more than one toast is present.

**Why this priority**: Even with merging and grouping, several different toasts can coexist; the collapsed stack keeps the screen quiet while nothing is lost.

**Independent Test**: Raise three different persistent toasts: one is shown in full with «+2»; expanding shows the three; «Cerrar todo» removes all.

**Acceptance Scenarios**:

1. **Given** three different toasts, **Then** only the newest is shown in full and a «+2» indicator is visible.
2. **Given** the collapsed stack, **When** the user hovers it, focuses it with the keyboard or clicks «+2», **Then** the stack expands and every toast is readable with its own actions and close button.
3. **Given** the expanded stack, **When** the pointer leaves and focus moves out, **Then** it collapses again.
4. **Given** more than one toast, **When** the user picks «Cerrar todo», **Then** every toast is dismissed, including the waiting toast (closing it as in Story 1, scenario 5).
5. **Given** only one toast, **Then** no «+N» indicator and no «Cerrar todo» are shown.

---

### User Story 4 - Timers that respect the user (Priority: P3)

Info toasts still go away on their own after 6 seconds (or their own duration), but not while the user is reading them: the timers pause while the stack is hovered, focused or expanded, and resume when it collapses. Errors and the waiting toast still stay until dismissed or resolved.

**Why this priority**: Expanding the stack is useless if the toasts vanish while being read; it is a refinement of Story 3.

**Independent Test**: Raise an info toast, hover the stack for 10 seconds, leave: the toast is still there and disappears 6 seconds after leaving (or its remaining time).

**Acceptance Scenarios**:

1. **Given** an info toast, **When** nobody touches the stack, **Then** it disappears after 6 seconds (or its own duration).
2. **Given** an info toast with 2 seconds left, **When** the user hovers the stack for 10 seconds and leaves, **Then** the toast stays for its remaining time (at least 2 seconds) after leaving.
3. **Given** a «Deshacer» toast whose duration is tied to a pending action (e.g. «Tarea descartada»), **Then** its action and timing behave as today; pausing the toast does not delay the action it undoes.

---

### Edge Cases

- More than the maximum number of toasts: the oldest **info** toast is dropped first; errors, waiting and undo toasts are kept over info toasts. If only those remain, the oldest is dropped as today.
- The waiting toast and a «Deshacer» toast at the same time: both kept; the newest is on top.
- A task that is waiting when the app starts or the task list is first loaded does not produce a toast (as today: the first list only seeds).
- A task that leaves the task list (deleted, archived) while in the waiting toast is removed from it.
- Undo flows that dismiss their own toast by id (TaskView «Marcar como hecha») keep working after a merge or while the stack is collapsed.
- A toast whose count is high shows the count capped for display («×99+»).
- Screen readers: a merged repeat or a change in the waiting toast's text is announced once, not as a new item each time.
- Very long text: the collapsed stack shows the newest toast with its text clipped to its width, full text when expanded.

## Requirements *(mandatory)*

### Functional Requirements

**Merging**

- **FR-001**: A new toast with the same kind and the same text as a toast on screen, and with no action of its own, MUST NOT add a toast; the existing one MUST increase a visible repeat count («×N», shown from 2), move to the newest position and, if it auto-dismisses, restart its timer.
- **FR-002**: Toasts with different kind, different text, or with an action MUST NOT be merged.
- **FR-003**: Dismissing a merged toast MUST remove it with all its repeats; the identifier returned when it was raised (for each repeat) MUST keep referring to that toast, so code that dismisses by identifier still works.

**Waiting group**

- **FR-004**: There MUST be at most one «te espera» toast on screen; it MUST list the tasks waiting for the user that started waiting while the app was open and that the user has not dismissed or opened.
- **FR-005**: With one task in the group the toast MUST read «{key} te espera» with «Responder», which opens that task; with N > 1 it MUST read «{n} tareas te esperan» with «Ver», which opens the Inbox.
- **FR-006**: A task MUST leave the group when it stops waiting, when its screen is opened, or when it is no longer in the task list; the toast MUST update its text in place and disappear when the group is empty.
- **FR-007**: Closing the waiting toast (its close button or «Cerrar todo») MUST empty the group; a task that starts waiting later MUST bring the toast back with only that task.
- **FR-008**: The waiting toast MUST NOT auto-dismiss.

**Collapsed stack**

- **FR-009**: With more than one toast, only the newest MUST be shown in full; the rest MUST be represented by a «+N» indicator (N = hidden toasts).
- **FR-010**: Hovering the stack, giving keyboard focus to anything in it, or activating the «+N» indicator MUST expand it to show every toast with its text, actions and close button; the stack MUST collapse when both pointer and focus have left it.
- **FR-011**: With more than one toast, a «Cerrar todo» action MUST dismiss every toast.
- **FR-012**: The stack keeps its current place (bottom-left of the window) and the shared notice style of the app (007-ui-v2 FR-059).

**Lifetimes and limits**

- **FR-013**: Info toasts MUST auto-dismiss after their duration (default 6 s); errors and the waiting toast MUST stay until dismissed (errors) or resolved/dismissed (waiting).
- **FR-014**: While the stack is hovered, focused or expanded, auto-dismiss timers MUST pause, and resume with their remaining time when it collapses.
- **FR-015**: When a new toast exceeds the maximum kept (3 today, unchanged), the oldest info toast without action MUST be dropped first; only when none exists is the oldest toast of any kind dropped. The waiting toast counts as one toast.
- **FR-016**: Undo toasts whose action is tied to a pending operation MUST behave as today (their action runs or is cancelled at the same time as before the change, regardless of pauses).

**Compatibility, language, accessibility**

- **FR-017**: Every existing place that raises or dismisses toasts MUST keep working without changes to what it shows, except the «te espera» toasts, which become the grouped toast.
- **FR-018**: New texts («{n} tareas te esperan», «Ver», «Cerrar todo», «+{n}», «×{n}» and the accessible labels for the indicator) MUST be in Spanish through the app's translation mechanism, with their English translations.
- **FR-019**: Toasts MUST remain announced to assistive technologies as status messages; the «+N» indicator and «Cerrar todo» MUST be reachable and operable by keyboard and have accessible names.
- **FR-020**: README and docs/status.md MUST describe the new toast behaviour.

### Key Entities

- **Toast**: a message on screen with a kind (info, error, wait), a text, an optional action, a duration (info only), a repeat count and an identifier (plus the identifiers of its merged repeats).
- **Waiting group**: the set of tasks currently represented by the single «te espera» toast.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: With 5 tasks starting to wait while the user is elsewhere, exactly 1 waiting toast is on screen (today: up to 3, filling every slot).
- **SC-002**: Raising the same message 10 times produces 1 toast with «×10» (today: 3 toasts and 7 lost).
- **SC-003**: With any number of toasts present, at most 1 toast is shown full size while the stack is collapsed.
- **SC-004**: A waiting toast disappears within one task-list refresh after its last task stops waiting, with no user action.
- **SC-005**: Every existing toast flow (copy command, task started, PR merged, branch updated, avisos cleared + «Deshacer», task done + «Deshacer», proposal discarded + «Deshacer», errors) still works, verified by the existing tests passing plus new ones for merging, grouping and collapse.

## Assumptions

- Frontend-only change: no daemon, HTTP API or data change; the waiting group is derived from the task list the app already polls.
- «Ver» opens the existing Inbox view, which already lists the tasks that wait for the user.
- Merging matches text exactly (after translation); messages that differ only in a number are different toasts.
- The maximum kept (3) and the default duration (6 s) stay as in 007-ui-v2; only what is shown changes.
- Toasts do not survive an app reload (unchanged).
- The macOS notifications of the Tauri app are out of scope; this only covers in-app toasts.
