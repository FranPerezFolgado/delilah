# Feature Specification: Create a task from an idea, with Claude

**Feature Branch**: `feature/LIBRE-4`

**Created**: 2026-10-20

**Status**: Draft

**Input**: User description: "Crear las tasks a mano es un poco coñazo, quizás estaría guay aprovechar el claude. Yo le digo a claude mi idea, me crea una task y esa task ya hace el flujo de ahora. El prompt que este primer agente le pasa al nuevo es el feature con el mini análisis. Esto no quita que yo pueda crearlas a mano es un extra"

## Context

Today a task is created by hand: the "Nueva tarea" dialog (or `delilah new`) asks for a
title, a project, notes and where to work, and a free task then starts `/feature <title>`.
Writing a good title and notes for an idea that is still vague is the tedious part.

Delilah already has **project sessions**: "Preguntar a Claude" opens a read-only Claude
in a terminal on the project's checkout to ask about the code. "Convertir en tarea"
turns *that same conversation* into a task's agent. This feature is different: the
conversation stays an analysis, and a **new** task is created whose agent starts
fresh from a brief written by the first Claude (the feature plus a mini-analysis).

The idea conversation is shown **without a terminal**. Delilah shows Claude's replies,
its questions as the same cards the Bandeja shows for tasks, and a text field to write
to it. A Delilah-provided skill tells Claude how to behave there: ask with question
cards, and end with a confirmation card.

## Clarifications

### Session 2026-10-20

- Q: Where does the user talk to Claude about an idea? → A: Both. A new "Nueva tarea con
  Claude" action opens a dedicated idea conversation. Existing project sessions can also
  create a task the same way.
- Q: Once created, does the task start right away or go to the backlog? → A: Always to
  the backlog. The user reviews or edits the brief and starts it.
- Q: Is the idea conversation a terminal? → A: No. A conversation view: Claude's replies,
  question cards (as in tasks) and a free-text field, driven by a skill that Delilah
  launches. The terminal stays reachable as a fallback.
- Q: Where does "Nueva tarea con Claude" appear? → A: In the "Nueva tarea" dialog (next
  to Jira and Libre, after picking the project) and on the project's screen and its "+".
- Q: What happens if the user writes while Claude is working? → A: The message is sent and
  queued: Claude reads it after the current turn. A "Detener" button stops the turn.
- Q: Is the conversation's history kept when it is reopened? → A: Yes, always, also after
  Delilah or the Mac restarts. Claude resumes the same conversation.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Tell Claude an idea and get a task in the backlog (Priority: P1)

The user picks "Nueva tarea con Claude" on a project. Instead of a terminal, Delilah opens
a conversation view and the user describes the idea in their own words. Claude looks at
the code as needed. Its replies appear as messages, and its questions appear as cards
with options. When it has enough, it shows a confirmation card with the proposed title
and the brief: the idea, a summary, the affected areas and the open questions. The
options are create, change something, or discard. On "create", the task appears in the
project's backlog with that brief. Claude says the key, and the conversation stays open.

**Why this priority**: it is the whole request. It turns an idea into a ready task
without filling in a form or reading a terminal.

**Independent Test**: on a project, open "Nueva tarea con Claude", describe an idea, answer
its cards and press "create". A new `LIBRE-n` task appears in the project's backlog with
Claude's title and brief, and no terminal was shown.

**Acceptance Scenarios**:

1. **Given** project P, **When** the user picks "Nueva tarea con Claude" (in the "Nueva
   tarea" dialog with P chosen, or from P's screen or "+"), **Then** a conversation view opens for P with a text field, and no terminal is shown.
2. **Given** the conversation is open, **When** the user sends a message, **Then** it
   appears in the conversation. While Claude works, the view shows that it is working
   (e.g. which files it is reading). When it finishes, its reply appears as a message.
3. **Given** Claude is working, **When** the user sends a message, **Then** it shows as
   queued and Claude answers it after the current turn. **When** the user presses
   "Detener", **Then** the turn stops and Claude waits for the next message.
4. **Given** Claude asks a question, **When** it reaches the view, **Then** it shows as a
   card with its options and a free-text "Otra". The user's answer goes back to Claude.
5. **Given** Claude has enough to propose a task, **When** it does, **Then** a confirmation
   card shows the title and the full brief, with the options create / change / discard.
6. **Given** the confirmation card, **When** the user picks "create", **Then** a new free task
   (`LIBRE-n`) is created in P's backlog with that title and brief, and Claude replies with
   its key.
7. **Given** the confirmation card, **When** the user asks for changes or discards,
   **Then** no task is created and the conversation goes on.
8. **Given** the conversation, **When** the user opens "Ver terminal", **Then** they see the
   same Claude session in its terminal, and can go back to the conversation view.

---

### User Story 2 - Start the task from its brief (Priority: P1)

The new task waits in the backlog with the brief as its notes. The user reads it, edits
it if needed, and starts the task as any backlog task. Its agent runs the usual flow
(workspace, `/feature`, phases), with the brief as its starting point.

**Why this priority**: the brief is what the first Claude hands to the second. Without
it, the task starts from a bare title, as today.

**Independent Test**: create a task from an idea, edit a line of its notes and start it.
The agent's first instruction contains the edited brief.

**Acceptance Scenarios**:

1. **Given** a task created from an idea, **When** the user opens it in the backlog,
   **Then** its notes show the brief, marked as written by Claude from a conversation.
2. **Given** that task, **When** the user edits the notes and starts it, **Then** its agent
   starts the usual feature flow from the brief as it is now.
3. **Given** the task has started, **When** it is restarted later, **Then** the agent can
   still read the brief, as with the notes of a task made by hand.

---

### User Story 3 - Create a task from a project session (Priority: P2)

In a project session ("Preguntar a Claude", in its terminal), the conversation about the
code turns into "this should be a task". The user asks Claude to create it. The same
proposal and confirmation happen, and the task lands in the backlog the same way.

**Why this priority**: the user chose both entry points. Questions about the code often
end in an idea, but the dedicated action already covers the request.

**Independent Test**: in a project session, ask Claude to make a task of what was
discussed and confirm. A `LIBRE-n` task with a brief appears in the backlog, and the session
is unchanged.

**Acceptance Scenarios**:

1. **Given** a project session, **When** the user asks for a task and confirms Claude's
   proposal, **Then** a free task with that brief is created in the project's backlog.
2. **Given** that, **When** the task is created, **Then** the project session keeps its
   conversation, and is not turned into the task (unlike "Convertir en tarea").

---

### User Story 4 - Several ideas in one conversation (Priority: P3)

In the same conversation the user can go through several ideas and get one task per
idea, each with its own brief.

**Why this priority**: brainstorming is a natural use, but one idea per conversation
already covers the request.

**Independent Test**: in one conversation, confirm two proposals. Two backlog tasks with
different keys and briefs appear.

**Acceptance Scenarios**:

1. **Given** a conversation that already created `LIBRE-5`, **When** the user confirms a
   second proposal, **Then** `LIBRE-6` is created with its own brief, and `LIBRE-5` is
   untouched.

---

### Edge Cases

- **The project is gone, or Delilah's daemon does not answer**: no task is created and
  Claude says so in the conversation. The brief is not lost: the user can retry.
- **Claude never asked for confirmation**: a task is only created after the
  user's yes on a confirmation card. A request in prose ("créala ya") still leads to the
  card.
- **Something the conversation view cannot show** (Claude stuck on a prompt that only the
  terminal shows): the view shows Claude as waiting, and "Ver terminal" is the way out.
- **Usage limit reached mid-conversation**: the view says Claude is paused by the limit,
  as task screens do.
- **Closing the view or restarting Delilah**: the conversation stays listed under the
  project, like project sessions, and reopens in the same view with its whole history.
- **Very long brief**: it is kept whole as notes. The agent's first instruction points
  to the notes instead of repeating them if they are too long to pass inline.
- **Idea about another project**: the task goes to the conversation's project. Moving it
  is done as today.
- **Manual creation**: the "Nueva tarea" dialog, `delilah new` and "Convertir en tarea"
  behave exactly as before.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: "Nueva tarea con Claude" MUST open an idea conversation for a project, and
  MUST be offered in two places: the "Nueva tarea" dialog (as a third origin next to Jira
  and Libre, for the chosen project), and the project's screen and its "+" in the sidebar.
- **FR-002**: An idea conversation MUST be shown as a conversation view, without a
  terminal. It shows the user's messages, Claude's replies at the end of each turn, an
  indication of what Claude is doing while it works, Claude's questions as cards, and a
  free-text field to send messages.
- **FR-002a**: While Claude is working, the user MUST still be able to send a message: it
  shows in the conversation as queued and Claude reads it after the current turn. The view
  MUST offer "Detener" to stop the current turn; queued messages are kept.
- **FR-003**: Question cards in the conversation view MUST behave like the ones the Bandeja
  shows for tasks: options, several choices when the question allows them, and a free
  "Otra". They MUST also show a question's longer detail (such as the brief) when it
  has one.
- **FR-004**: The conversation view MUST offer the same Claude session in a terminal
  ("Ver terminal") as a fallback, and a way back to the conversation view.
- **FR-005**: The idea conversation MUST read the code without editing it, and MUST NOT
  depend on terminal-only prompts to make progress.
- **FR-006**: Delilah MUST provide the instructions (a skill) that the idea conversation
  follows. They ask with question cards rather than prose, write the brief, and end
  each proposal with a confirmation card.
- **FR-007**: The brief MUST contain at least: the idea as the user put it, a short
  summary of the feature, the areas of the code it likely touches, and its open questions.
- **FR-008**: Claude MUST create a task only after the user picks "create" on a
  confirmation card that shows the title and the full brief.
- **FR-009**: A task created this way MUST be a free task (`LIBRE-n`, numbered like any
  other free task), in the conversation's project, using the project's default workflow
  and default place to work, and it MUST go to the backlog (not started).
- **FR-010**: The brief MUST be stored as the task's notes, editable while the task is in
  the backlog. When the task starts, its agent MUST receive the brief (as it is then) as
  its starting point, together with the usual feature workflow.
- **FR-011**: Claude MUST tell the user the result in the conversation: the new task's
  key on success, or a reason on failure. The conversation MUST stay usable afterwards.
- **FR-012**: One conversation MUST be able to create more than one task, each
  independent.
- **FR-013**: Project sessions MUST also be able to create a task the same way (proposal,
  confirmation, backlog). They keep their terminal and their conversation.
- **FR-014**: Only idea conversations and project sessions MAY create tasks. Agents
  working inside tasks MUST NOT gain that ability.
- **FR-015**: The task MUST record in its history that it was created from a
  conversation.
- **FR-016**: Idea conversations MUST be listed under their project, like project
  sessions, and reopen in the conversation view with their whole history (messages,
  Claude's replies, cards and the answers given), also after Delilah or the computer
  restarts. Claude resumes the same conversation.
- **FR-017**: Manual task creation (dialog, CLI, "Convertir en tarea") MUST keep working
  unchanged.

### Key Entities

- **Idea conversation**: a read-only Claude conversation tied to one project, shown in
  the conversation view, from which tasks can be created. It outlives the tasks it creates.
- **Conversation view**: the user's messages, Claude's end-of-turn replies, activity while
  it works, question cards and a text field. It is a view of one Claude session.
- **Conversation entry**: one item of a conversation's history: a user message (sent or
  queued), a Claude reply, or a question card with the answer given. Kept with the
  conversation, in order.
- **Brief**: the text Claude writes for a task: the idea, a summary, the affected areas and
  the open questions. It becomes the task's notes and its agent's starting point.
- **Task** (existing): gains the brief as notes and a history entry saying it came from a
  conversation.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: From "create" on the confirmation card, the task appears in the project's
  backlog within 5 seconds, without the user typing a title, project or notes.
- **SC-002**: The user can turn an idea into a backlog task without seeing a terminal or
  a form: messages, answered cards and one confirmation.
- **SC-003**: In 100% of tasks created this way, the task's notes hold the brief
  confirmed on the card, and the agent starts from the notes as they are when it starts.
- **SC-004**: No task is ever created without the user's "create" on a confirmation card.
- **SC-005**: Every way of creating tasks by hand gives the same result as before this
  feature.

## Assumptions

- Only the Claude Code agent exists today; this feature targets it, with the user's own
  subscription, as every other session.
- Claude's replies are shown when each turn ends, not word by word. Activity during the
  turn (files read, searches) is enough feedback for a short analysis.
- The analysis is "mini": minutes of reading, not a spec. The spec is the new task's
  own `/feature` job.
- Tasks from ideas are always free (`LIBRE-n`). Creating one linked to a Jira ticket
  from a conversation is out of scope.
- The new task's agent does not inherit the conversation, only the brief. Carrying the
  conversation over is what "Convertir en tarea" already does.
- The conversation view is only for idea conversations in this feature. Task sessions
  and project sessions keep their terminal.
- Delilah runs on the user's own machine. Who may create tasks is limited to the sessions
  Delilah launched for this purpose, with no extra login.
