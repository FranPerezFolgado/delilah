# Feature Specification: UI v2 (full redesign of the desktop UI and the data it needs)

**Feature Branch**: `feature/ui-v2`

**Created**: 2026-10-25

**Status**: Draft

**Input**: User description: "Apply the UI redesign brief `docs/ui-redesign.md` (canvas «Delilah · Cabecera de tarea») to the whole app: tokens and components first, then every screen, plus every daemon/API change of §6 of the brief. One branch, one PR, one commit per step of §8, one version bump at the end."

The brief (`docs/ui-redesign.md`) is the source of truth for every visual detail (token
values, component anatomy, copy). This spec states what the user gets and how to tell it
is done; it refers to the brief's sections instead of repeating them. The canvas boards
named in each story are the visual reference.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - A calm, consistent look in both themes (Priority: P1)

The user opens Delilah in dark or light (following the OS). Panels are distinguishable from
the background, text is readable without glare, and the same thing always looks the same:
one form for a task's status, one for an inbox item's kind, one for a PR's state, one for
the phase, one primary button per view. The terminal and the editor follow the theme when
the OS switches, without reloading.

**Why this priority**: every other story is built from these tokens and components; the
review that started the redesign found the theme too dark and ~8 treatments for status.

**Independent Test**: open the app (step 1 alone), switch the OS theme with a terminal tab
and an editor tab open; compare with board Sistema v2 › Sistema in both themes.

**Acceptance Scenarios**:

1. **Given** the app in dark, **When** the OS switches to light, **Then** every surface,
   the open terminal and the open editor switch to the light palette without a reload.
2. **Given** any screen, **When** a task status is shown, **Then** it is a status badge (dot
   + text on a tonal background) with one of: Backlog, Trabajando, Te espera, En pausa,
   Hecha, Error (mapping in brief §3).
3. **Given** any view, **Then** the colour rules of FR-008 hold (one primary button per
   view; amber and blue each with their single meaning).
4. **Given** any interactive control, **When** it receives keyboard focus, **Then** a 2 px
   focus ring is visible.

---

### User Story 2 - Know at a glance what needs me, and whether Delilah is up (Priority: P1)

From any screen the user sees, in the sidebar and the statusbar, what is waiting for them
(hot count on Bandeja), how many tasks are working, waiting, paused or failed, the usage
quota, and whether the daemon is connected. Task rows show the title first. When the daemon
stops, the app says so in plain words, keeps the last data greyed out, retries on its own
and recovers alone. Logs open from the statusbar, also in a plain browser. The sidebar
folds to an icon rail with ⌘\ or on its own on narrow windows.

**Why this priority**: the shell is on screen all the time; the old statusbar read
«Agentes trabajando 0» while tasks waited, and a stopped daemon looked like an empty app.

**Independent Test**: in a preview, stop the preview daemon: the disconnected state shows;
start it again: the app recovers without a reload. Boards: Sistema v2 › Lateral, Diálogos
y estados › Desconectado, Logs.

**Acceptance Scenarios**:

1. **Given** 3 running, 1 waiting, 1 paused and 1 failed task, **When** the user looks at
   the statusbar, **Then** it reads «● Conectado · 3 trabajando · 1 te espera · 1 en pausa ·
   1 con error» plus the usage, and each count opens the board filtered by that status.
2. **Given** the daemon stops, **When** the next fetch fails, **Then** the statusbar shows
   «Sin conexión con delilahd · reintentando», a banner over the main area offers
   «Reintentar ahora» and how to start the daemon, actions are disabled, and the content
   shows «Datos de hace N min».
3. **Given** the disconnected state, **When** the daemon comes back, **Then** the banner
   goes away and data refreshes without user action.
4. **Given** the sidebar, **When** the user presses ⌘\ (or the window is below the FR-056
   threshold), **Then** it becomes a 56 px icon rail; the shortcut is listed and rebindable
   in Ajustes › Atajos.
5. **Given** a project with tasks, **When** the sidebar lists them, **Then** each row shows
   a status dot and the full title first; the key appears only on hover/selection; the
   tooltip reads key · title · status.
6. **Given** the browser build (no native menu), **When** the user clicks «Logs» in the
   statusbar or in Ajustes › Diagnóstico, **Then** the Logs view opens.

---

### User Story 3 - Work a task from one screen, never sent elsewhere to answer (Priority: P1)

On the task screen the user sees the key, an editable title, the status, the phase, the
Chat | Terminal switch of the open session, the Autopiloto and Ahorro toggles and the
Preview control in one header that never wraps the key. Tabs hold the task overview,
sessions, open files, Cambios and the PR, with one «+ Sesión» button. A pending question
or permission is answered inline in the Tarea tab and in the chat. Marking done can be
undone. The PR tab explains errors once with an action, shows check durations and review
comments, and can merge when checks pass. A failed setup shows exit code, command and the
output with retry/skip/full log. A done task shows Hecha, the merged PR and a relative date
instead of phase bars.

**Why this priority**: the task screen is where the user spends most time; answering
from the Bandeja only and a crowded header were the main complaints.

**Independent Test**: start a task whose fake agent asks a question; answer it from the
Tarea tab without opening the Bandeja. Boards: Cabecera de tarea › Main, Estados;
Pantallas › Tarea; Diálogos y estados › TareaSetup; TareaHecha, TareaCaida, TareaLibre.

**Acceptance Scenarios**:

1. **Given** a task waiting on a question, **When** the user opens its Tarea tab, **Then**
   the question is answerable there (options, free reply, Enviar / ⌘↵) and the task goes
   back to Trabajando after sending.
2. **Given** a chat-mode task whose agent asks a permission, **When** the card is pending,
   **Then** the task status is «Te espera»; **When** the card is answered (here, in the
   chat or in the Bandeja), **Then** the card stays in the conversation with its outcome.
3. **Given** a long title, **When** the header is narrow, **Then** the key stays on one
   line and the title takes the remaining space (ellipsis), never pushing the controls off.
4. **Given** the user clicks the title, **When** they type a new one and confirm, **Then**
   it is saved, and a muted line «Antes: <old> · Restaurar» appears when the analysis had
   renamed it.
5. **Given** autopilot is off, **When** the user turns the Autopiloto toggle on, **Then** a
   confirmation explains it runs on its own until it opens the PRs and uses quota, and
   that Ahorro stays on; after «Activar», Ahorro is on and disabled with the reason in its
   tooltip. When autopilot pauses, the header shows «Autopiloto en pausa · Reanudar» with
   the reason.
6. **Given** a PTY session tab, **When** the user picks Chat or Terminal, **Then** only
   that session's view changes; **Given** a chat-mode (stream) session, **When** they pick
   Terminal, **Then** a confirmation explains the session's runtime changes, and the
   history records the switch.
7. **Given** the user marks the task done, **When** the «Deshacer» toast is clicked within
   its lifetime, **Then** the task is open again (Te espera) and its sessions can be resumed.
8. **Given** a PR whose checks are running, **When** the PR tab is open, **Then** running
   checks are neutral with their running time, finished ones show their duration, review
   comments list author, file, text and time, and «Fusionar» stays disabled until checks
   pass (reason in the tooltip).
9. **Given** no `gh` session, **When** the PR tab loads, **Then** one friendly error appears
   with «gh auth login», «Abrir una terminal» and «Recargar», never the raw stderr twice.
10. **Given** the setup command failed, **When** the task opens, **Then** the header shows
    «Sin iniciar» instead of phase bars, and the panel shows the exit code, the command,
    the output tail, «Reintentar setup», «Iniciar sin setup», «Abrir el log completo» and a
    link to the project's setup command.
11. **Given** a project screen (`@project`), **When** it opens, **Then** the first tab is
    «Proyecto» and renders the project view.
12. **Given** many sessions and files, **When** the tab row overflows, **Then** «+ Sesión»
    stays reachable and opens a menu with Claude (⌘⇧T) and Terminal (⌘T).

---

### User Story 4 - Triage the Bandeja quickly and safely (Priority: P2)

The Bandeja lists what needs the user with its kind (Pregunta, Permiso, Aviso), relative
times and, in the detail, the right inline action for each kind. «Limpiar avisos» removes
only non-blocking avisos and can be undone. Keys move, choose and remove when focus is not
in a text field. New avisos tell when a secondary session's turn finished and when a PR
was opened.

**Why this priority**: the inbox is the second most used screen; clearing must never hide
something an agent waits on.

**Independent Test**: with one question, one permission, one blocking PR failure and three
non-blocking avisos, click «Limpiar avisos»: only the three avisos go; «Deshacer» brings
them back. Board: Pantallas › Bandeja.

**Acceptance Scenarios**:

1. **Given** blocking and non-blocking items, **When** the user clicks «Limpiar avisos»,
   **Then** only non-blocking avisos disappear and a toast offers «Deshacer», which restores
   exactly those.
2. **Given** a Pregunta item, **Then** the detail shows numbered options, a free reply and
   Enviar (⌘↵); a Permiso item shows the command in a mono block and «Permitir una vez» /
   «Permitir siempre en esta tarea» / «Denegar», with no free text; an Aviso shows its action
   and «Quitar» when not blocking.
3. **Given** focus is on the list, **When** the user presses ↑/↓, 1–3 or ⌫, **Then** the
   selection moves, an option is chosen, or the aviso is removed; **Given** focus is in a
   text field, **Then** those keys type normally.
4. **Given** nothing waits, **Then** the Bandeja reads «Nada te espera» and shows the latest
   activity line.
5. **Given** a removed aviso, **When** the same cause happens again later, **Then** a new
   aviso appears.

---

### User Story 5 - See all tasks on the board, start from zero, and browse Jira (Priority: P2)

The board shows Backlog, the four phases and Hecha as flexible columns (empty phases fold),
with a project filter and the status filter from the statusbar. Cards carry key, status
badge, a two-line title, project, mini phase bars, PR chip and an autopilot marker. A first
run shows a checklist (add a project, connect Jira optionally, create the first task). Jira
uses labelled filter chips, two-line rows, a detail with the linked Delilah task or
«Analizar con Claude», and subtasks with progress.

**Why this priority**: the board is the overview; first run and Jira are entry points.

**Independent Test**: fresh preview: the first-run checklist shows, «Omitir» on Jira is
remembered after a reload. Boards: Pantallas › Tablero, Jira; Diálogos y estados › PrimerUso.

**Acceptance Scenarios**:

1. **Given** tasks in Backlog, Plan and Hecha only, **Then** the Especificación,
   Implementación and Revisión columns fold to 40 px strips.
2. **Given** a status filter chosen from the statusbar, **Then** the board shows only tasks
   with that status, and the filter can be cleared.
3. **Given** no projects, **Then** the board shows the first-run checklist; each item is
   marked when done; «Omitir» on Jira persists across reloads and windows; the checklist
   hides once all 3 steps are done (Jira done or skipped).
4. **Given** a Jira parent with 20 subtasks of several assignees and statuses, **When** its
   detail opens, **Then** all subtasks are listed grouped by status with a progress count,
   and the parent row shows the count.
5. **Given** a ticket with a Delilah task, **Then** its detail shows one clickable row
   (phase · session, status, «Abrir ›»); otherwise the primary «Analizar con Claude» with
   one line saying what it does.
6. **Given** filters that match nothing, **Then** the list reads «Ningún ticket con estos
   filtros» with «Quitar filtros».

---

### User Story 6 - Create and configure without surprises (Priority: P2)

Nueva tarea is a fixed-size dialog whose «Iniciar» button is always visible. Where the agent
works defaults to the user's checkout. Backlog tasks show when they were created and use the
same form language. Projects are a table with an «Añadir proyecto» dialog, row menus and a
confirm dialog for removal (blocked with a note while tasks hold a workspace). Project
settings and every destructive action use one dialog pattern. Ajustes has a section index,
the saving-mode wording of decision 33, an auto-update preference and a Diagnóstico section.

**Why this priority**: the start button falling below the fold and worktree-by-default
surprised users; these screens share the dialog pattern.

**Independent Test**: open Nueva tarea, switch origin between Jira / libre / Con Claude and
open «Opciones avanzadas»: the dialog height never changes and «Iniciar» stays visible.
Boards: Diálogos y estados › NuevaTarea, Eliminar, QuickOpen; Pantallas › BacklogTarea,
Proyectos, ProyectoVista, Ajustes; Conversacion.

**Acceptance Scenarios**:

1. **Given** Nueva tarea, **When** the user switches origin or expands advanced options,
   **Then** the dialog stays 600×700 with a scrolling body and a sticky footer
   «Cancelar · Guardar en backlog · Iniciar».
2. **Given** a project with no workspace setting and a new task with no choice, **When** it
   starts, **Then** it works in the checkout; the picker labels that option «por defecto»;
   a project setting, when set, wins; existing tasks keep their saved choice.
3. **Given** a backlog task created yesterday, **Then** its view reads «Creada ayer» and its
   actions are «Iniciar» (primary), «Guardar», «Eliminar…» (danger, far right); after
   «Iniciar» it shows Trabajando and a toast «<KEY> iniciada · Abrir».
4. **Given** a project with an active workspace, **When** the user opens «Quitar…», **Then**
   the dialog explains it is blocked and «Quitar» is disabled.
5. **Given** any destructive action, **Then** it confirms in the same dialog with buttons in
   the order Cancelar · Acción; Esc closes it and focus returns to the opener.
6. **Given** Ajustes › Actualizaciones, **When** the user turns off «Buscar actualizaciones
   automáticamente», **Then** the app stops checking on its own (manual check still works).
7. **Given** Ajustes › Diagnóstico, **Then** it shows the daemon version, the data folder,
   a button to open Logs and, in the desktop app only, «Mostrar en Finder».
8. **Given** the project view, **Then** conversation rows show their last message and time.

---

### User Story 7 - Narrow windows and many tasks still work (Priority: P3)

At 1280 and 1024 px wide nothing scrolls horizontally except the board columns: the sidebar
becomes a rail, the file tree hides, the task header compacts, and the inbox and Jira
details open as overlays. With many tasks the sidebar shows the first 6 active tasks per
project with «N más» and a filter; the inbox groups items by task. Loading shows skeletons;
global notices (toasts, update banner, quota, autopilot pause reasons) follow one style.

**Why this priority**: polish on top of the screens; the window minimum is already 1024.

**Independent Test**: resize the preview window to 1024 on each screen; only the board
scrolls horizontally. Boards: Anchos y escala › Tarea1280, Tablero1280, Jira1024,
EscalaLateral, EscalaJira; Cargando; AvisosGlobales.

**Acceptance Scenarios**:

1. **Given** a 1024 px window, **When** the user visits every screen, **Then** no page
   scrolls horizontally except the board's columns.
2. **Given** below ~1200 px, **When** the user selects an inbox item or a Jira ticket,
   **Then** the detail opens as an overlay/drawer and closes with Esc.
3. **Given** a project with 15 active tasks, **Then** the sidebar shows 6 and «9 más»;
   typing in «Filtrar tareas» filters every project's rows.
4. **Given** any list or panel loading, **Then** skeletons show; empty-state text appears
   only after the load finished.

### Edge Cases

- The daemon goes away mid-action (e.g. while sending a reply): the action fails once, in
  human words, the disconnected state takes over, and nothing is retried silently.
- A permission card expires because it was answered in the terminal or the process died: the
  card stays with the outcome «Caducada · se respondió en la terminal»; the task leaves «Te
  espera» when no other ask is pending.
- Two asks pending on one task (a question and a permission): the task stays «Te espera»
  until both are answered.
- «Limpiar avisos» with only blocking items: nothing is removed and the button is disabled
  with the reason.
- «Deshacer» after the toast expired or after the app reloaded: the avisos stay removed;
  restoring an id that no longer exists is a no-op.
- Reopening a done task whose workspace was archived meanwhile: refused with a message; the
  toast's «Deshacer» is gone after archiving.
- Merging when checks changed to failing between render and click: the daemon refuses
  with the reason; the PR tab refreshes.
- Setup failed before any output (command not found): exit code and command still show;
  «Abrir el log completo» says the log is empty.
- A checkout task started by default when the checkout is dirty: the existing «checkout
  must be clean» refusal is shown as one human error with an action, not a raw message.
- A title edit to empty or whitespace: refused inline; the old title stays.
- A very long project name or task title in the breadcrumb or sidebar: ellipsis, full text
  in the tooltip.
- The OS theme changes while a dialog or menu is open: it re-themes in place.
- English UI: every new or renamed text has an English entry; no Spanish fallback appears.
- A Jira parent with no subtasks or Jira not connected: the subtask section is absent;
  the sample-data mode shows sample subtasks.
- Switching a session's runtime while its turn is running: refused until the turn ends.

## Requirements *(mandatory)*

### Functional Requirements

**Look and components (brief §1, §2)**

- **FR-001**: The app MUST use the v2 token set (brief §1 and appendix) for every colour,
  with dark as the default and light under the OS light preference; no raw colour values
  outside the token definitions, and none of the old token names left.
- **FR-002**: The app MUST use only the type scale 19 / 15 / 13.5 / 12 (mono 12), the radii
  7 / 10 / 14 / 11, and themed scrollbars, checkboxes and radios.
- **FR-003**: The terminal and the code editor MUST take their colours from the tokens and
  switch theme live when the OS theme changes; editor selection and search highlights MUST
  be neutral, never amber.
- **FR-004**: The app MUST provide and use one shared component each for: buttons (primary,
  secondary, ghost, danger; 32/28 px), task status badge, inbox kind, PR chip, phase bars,
  toggle, segmented control, option card, list row, count, skeleton, dialog and confirm
  dialog, menu, ask card and toast; the components they replace (brief §2 table) MUST be
  gone.
- **FR-005**: Selection MUST look the same everywhere (selected background + strong text,
  option cards add a ✓), except tabs, which use a neutral 2 px underline.
- **FR-006**: Layout values MUST live in style sheets; inline styles are allowed only for
  truly dynamic values.
- **FR-007**: Dialogs MUST be modal dialogs with a labelled title, trapped focus, Esc to
  close and focus returned to the opener; menus MUST support arrow keys, Esc and focus
  return; icon-only buttons MUST have an accessible name; single-choice value groups MUST be
  radio groups and action/view pairs MUST be pressed-state buttons.
- **FR-008**: There MUST be at most one primary button per view; amber MUST only mean «te
  espera»; blue MUST only mean «Trabajando» and focus; in-progress checks, spec tasks and
  open PRs MUST be neutral.

**Vocabulary and language (brief §3)**

- **FR-009**: Task status MUST map exactly as brief §3 (queued → Backlog; preparing and
  running → Trabajando, preparing with «Preparando el workspace»; waiting → Te espera;
  paused → En pausa with «se reanuda a las HH:MM»; done → Hecha; failed and setup-failed →
  Error with the reason as detail).
- **FR-010**: Inbox kinds MUST map to Permiso, Pregunta and Aviso as in brief §3; the
  blocking set MUST be the one hot counts use.
- **FR-011**: PR state MUST show as Borrador, Abierta (neutral), Checks fallan or Fusionada.
- **FR-012**: Sessions MUST show only a state dot, never the word «Inactiva».
- **FR-013**: All copy MUST follow the glossary (Iniciar, Detener, Abrir …, Recargar, Omitir,
  Guardar en backlog, Responder/Enviar, Eliminar…, Quitar, Cancelar, Autopiloto, Ahorro with
  the decision-33 wording, no English in the Spanish UI, Logs filter «Todo · Info ·
  Advertencias · Errores»).
- **FR-014**: Every visible text MUST have an English translation keyed by the Spanish text;
  renamed texts MUST drop their old keys; the untranslated strings listed in brief §3 MUST be
  translated.
- **FR-015**: The task history MUST show every event as a human sentence with a relative
  time, never a raw event name.

**Shell, sidebar, statusbar (brief §5 Shell, Sidebar)**

- **FR-016**: The titlebar MUST show a breadcrumb «Delilah / <vista>» or «Delilah / <proyecto>
  / <KEY>» and keep every native drag region and the room for the window controls.
- **FR-017**: The statusbar MUST show the connection state, counts of working / waiting /
  paused / failed tasks (each opening the board filtered by that status; «con error» in the
  error colour), the usage quota and a Logs button.
- **FR-018**: When the daemon cannot be reached the app MUST show the disconnected state
  (statusbar, banner with «Reintentar ahora» and how to start it, greyed content, disabled
  actions, age of the data) and MUST recover on its own when the daemon returns.
- **FR-019**: The sidebar MUST offer Bandeja (hot count of blocking items), Tareas (count),
  Jira and Proyectos, a «Proyectos» header with «Añadir proyecto», per-project hover actions
  («Preguntar a Claude», «Nueva tarea en este proyecto»), the Ideas row, conversation rows,
  task rows (dot + title first, key on hover/active, tooltip key · title · status), a
  collapsible «Backlog · N», «N hechas», «Sin proyecto», and Ajustes next to «Plegar barra
  lateral»; the old «+ Tarea» nav button MUST be gone; shortcuts MUST move to tooltips;
  project buttons MUST be announced by name.
- **FR-020**: The sidebar MUST fold to a 56 px icon rail with a rebindable shortcut (default
  ⌘\, listed in Ajustes › Atajos), and automatically on narrow windows (threshold in FR-056).
- **FR-021**: The Logs view MUST offer the level filter, search, «Seguir» and «Copiar», colour
  WARN/ERROR lines as brief §1, and be reachable from the statusbar and Ajustes › Diagnóstico
  in the browser build too.

**Task screen (brief §5 Task screen)**

- **FR-022**: The task header MUST show, in order: key (mono, never wraps), editable title
  (takes the remaining space; «Antes: <old> · Restaurar» after an analysis rename), status
  badge, phase bars with label (and «· N/M tareas» in Implementación), the Chat | Terminal
  switch for the open session, Autopiloto and Ahorro toggles, the Preview control and the
  tree toggle.
- **FR-023**: Turning Autopiloto on MUST ask for confirmation (title «Activar autopiloto»,
  what it does, Cancelar · Activar); while on, Ahorro MUST be forced on and disabled with
  the reason; when paused, the header MUST show «Autopiloto en pausa · Reanudar» with the
  reason. The separate «Desactivar» buttons and the old pills MUST be gone.
- **FR-024**: The Chat | Terminal switch MUST act on the open session only: for a PTY session
  it changes the view; for a stream session choosing Terminal (or for a PTY session
  choosing chat mode) MUST change that session's runtime after a confirmation, between
  turns. Chat MUST carry a «beta» tag, replacing the old experimental button and pill. It
  MUST be hidden on the Tarea tab and on plain terminal tabs.
- **FR-025**: Under the header the session chain row and the base-stale notice MUST be shown
  as thin neutral bars («main avanzó N commits · Actualizar rama»).
- **FR-026**: Tabs MUST be: Tarea (or Proyecto on a project screen) · sessions · file tabs
  (closable, mono names) · Cambios (closable, with a count) · PR, then «+ Sesión» with a menu
  (Claude / Terminal, with the existing shortcuts); the old in-tab «+ Claude» / «+ Terminal»
  MUST be gone.
- **FR-027**: The Tarea tab MUST show a meta line (proyecto · flujo · rama), «Marcar como
  hecha» with a «Deshacer» toast, a «Más» menu (Abrir cambios, Archivar workspace, Eliminar
  tarea…), the pending ask answered inline, Sesiones, Tareas de la spec (collapsible by
  phase, progress, «Abrir tasks.md») and Historial; no stepper.
- **FR-028**: Undoing «Marcar como hecha» MUST reopen the task (status Te espera) while its
  workspace still exists; the reopen MUST be recorded in the history.
- **FR-029**: A pending permission ask of a chat-mode agent MUST put the task in «Te espera»;
  only answering or expiring the **last** pending ask (permission or question) MUST return it
  to Trabajando; a session that ends with an open ask MUST expire it; after a daemon restart a
  task left waiting on a lost permission MUST be reconciled (the ask recorded as expired, the
  task resumable); the Bandeja MUST show one item per waiting session, not a permission and a
  generic wait.
- **FR-030**: Answered, expired and withdrawn permission cards MUST stay in the
  conversation with their command summary and outcome; expiries and runtime switches MUST
  appear in the history.
- **FR-031**: The chat tab MUST show turns, tool calls collapsed to one line, live
  permission cards and the composer with Enviar / Detener.
- **FR-032**: Cambios MUST list files grouped by folder with a filter, show a skeleton while
  loading, offer «Recargar», hide the tree panel on that tab and refresh the tree after
  commits.
- **FR-033**: The PR tab MUST show one friendly error with an action, «Abrir PR» disabled
  with its reason, checks with duration (running ones neutral, with running time), review
  comments (author, file, text, time), «Abrir en GitHub», and «Fusionar», enabled only when
  checks pass.
- **FR-034**: The user MUST be able to merge a task's PR from the PR tab after confirming; the
  daemon MUST check the signed-in `gh` account first and refuse when the PR is a draft, not
  open, its checks do not pass, or the repository's merge methods are unknown; in that last
  case «Fusionar» is disabled with «No se pudo saber qué métodos de fusión permite el
  repositorio».
- **FR-035**: The setup-failed state MUST show exit code, duration, command, output tail,
  «Reintentar setup», «Iniciar sin setup», «Abrir el log completo» (the full log) and a link
  to the project's setup command; the header MUST show «Sin iniciar» instead of bars.
- **FR-036**: A done task MUST show Hecha, the PR chip and a relative date, never phase
  bars; a crashed agent and a free workflow with several Claude sessions MUST follow the
  TareaCaida and TareaLibre boards.

**Bandeja (brief §5 Bandeja)**

- **FR-037**: The Bandeja MUST have a list (380 px) and a detail pane, relative times, the
  kind of each item, and the inline actions per kind of brief §5.
- **FR-038**: «Limpiar avisos» MUST remove only non-blocking avisos, show a «Deshacer» toast
  that restores exactly them, and «Quitar» MUST remove one non-blocking aviso; removals
  MUST persist across reloads and windows, and a new occurrence of the same cause MUST show
  again.
- **FR-039**: The Bandeja MUST raise non-blocking avisos when a secondary Claude session's
  turn finished and when a PR was opened or linked.
- **FR-040**: List keys (↑↓, 1–3, ⌫) MUST work only when focus is not in a text field, with
  a hint line saying so.
- **FR-041**: The empty Bandeja MUST read «Nada te espera» with the latest activity line.

**Board, first run, Jira (brief §5 Tareas, Jira)**

- **FR-042**: The board MUST show Backlog + 4 phases + Hecha as flexible columns, fold empty
  phases to 40 px, filter by project and by status, and offer the primary «Nueva tarea».
- **FR-043**: Board cards MUST show key, status badge (always with text), two-line title,
  project, mini phase bars (none for Backlog/Hecha), PR chip and the autopilot marker, with
  no highlight borders for waiting cards.
- **FR-044**: A first run MUST show the checklist «Añadir un proyecto» → «Conectar Jira
  (opcional)» → «Crear tu primera tarea», each marked when done, with «Omitir» for Jira;
  the skip and whether to show the checklist MUST persist.
- **FR-045**: Jira MUST use labelled dropdown filter chips (De quién, Estado, Sprint),
  «Agrupar por Estado | Épica», two-line rows, an avatar column only when «De quién» is not
  «Asignados a mí», and priority only for Alta/Crítica.
- **FR-046**: The Jira detail MUST offer «Abrir en Jira ↗», the linked Delilah task row or
  «Analizar con Claude», fields, description and all subtasks of the parent (any assignee
  or status) grouped by status with progress; parents MUST show their subtask count.
- **FR-047**: Jira MUST show «Ningún ticket con estos filtros» + «Quitar filtros» when empty,
  and the connect form MUST offer Cloud / Server / Datos de ejemplo.

**Dialogs, projects, settings, backlog, new task (brief §5)**

- **FR-048**: Nueva tarea MUST be a fixed-size dialog (600×700) with a scrolling body and a
  sticky footer «Cancelar · Guardar en backlog · Iniciar»; origin MUST be a radio group
  (Ticket de Jira | Tarea libre | Con Claude, the Con Claude step per the Conversacion
  board); «Opciones avanzadas» MUST hold Chat | Terminal (Chat «beta», default as today),
  Rama and Primer mensaje; Autopiloto/Ahorro MUST use the header's toggles and confirmation.
- **FR-049**: With no choice on the task and no project setting, a task MUST work in the
  user's checkout (daemon and every client); the picker MUST label that option
  «Por defecto (en tu checkout)»; tasks with a saved choice keep it. A refusal because the
  checkout has uncommitted changes MUST be one human error («Tu checkout tiene cambios sin
  guardar…») with the action «Iniciar en un worktree».
- **FR-050**: The backlog task view MUST use the Nueva tarea form language, actions
  «Iniciar» / «Guardar» / «Eliminar…», show the creation date relatively, and after
  «Iniciar» show the started state and a toast with «Abrir».
- **FR-051**: Proyectos MUST be a table (name with kind icon and «Repo · N tareas activas»,
  Dónde trabaja, Carpeta, actions), «Añadir proyecto» MUST open a dialog, rows MUST offer
  «Ajustes» and a menu (Renombrar, Quitar…), «Quitar» MUST confirm in a dialog and be
  disabled with a note while a task holds a workspace; rows MUST open the project view.
- **FR-052**: Project settings MUST be a real dialog with help text for every field;
  «Comando de preview» MUST be editable there and in the project view.
- **FR-053**: The project view MUST show folder icon + name + path once, «Preguntar a
  Claude» (primary), «Nueva tarea», «Terminal», Conversaciones/Ideas (with last message and
  time), Tareas activas, Comando de preview, README preview and file tree.
- **FR-054**: Ajustes MUST have a section index (General, Preguntas, Modo ahorro,
  Actualizaciones, Atajos de teclado, Diagnóstico), content at most 720 px wide, «Por
  defecto» only on changed shortcuts, «Restaurar todos», an «Buscar actualizaciones
  automáticamente» preference, and Diagnóstico with Logs, daemon version, data folder and
  (desktop only) «Mostrar en Finder».
- **FR-055**: Every destructive action MUST confirm in the shared confirm dialog; Quick Open
  MUST bold matched characters and show key hints.

**Widths, scale, loading, notices (brief §5 Widths, §8 step 6)**

- **FR-056**: Below ~1300 px the sidebar MUST be a rail and the file tree hidden; below
  ~1200 px inbox and Jira details MUST open as overlays/drawers; at ~960 px the task header
  MUST show bars only and icon toggles; at 1024 px nothing but the board columns MAY scroll
  horizontally (board columns min 220 px).
- **FR-057**: At scale the sidebar MUST show the first 6 active tasks per project with «N
  más» and a «Filtrar tareas» field; the inbox MUST group items by task with a count.
- **FR-058**: Loading MUST show skeletons; empty text MUST appear only after a finished load.
- **FR-059**: Toasts, the update banner, quota notices and autopilot pause reasons MUST share
  one style (AvisosGlobales board).

**Release and record (brief §0, §9)**

- **FR-060**: The redesign MUST ship on one branch and one PR with one commit per §8 step and
  one version bump at the end; e2e selectors MUST use roles and text instead of class names.
- **FR-061**: The before/after record MUST be completed: the same 46 screens captured with
  `DELILAH_SCREENS=despues` into `docs/redesign/despues/`.
- **FR-062**: The decisions taken (checkout default, pending permission = Te espera, Chat |
  Terminal per session, inbox removals, glossary) MUST be recorded in `docs/decisions.md`;
  `docs/status.md`, the README and `docs/architecture.md` MUST describe the new behaviour.

### Key Entities *(include if feature involves data)*

- **Task**: gains its creation date (already stored) and the ability to be reopened after
  «Marcar como hecha»; its status becomes «Te espera» while a permission ask is pending.
- **Session**: gains its last activity time, last message, an optional runtime of its own
  (overriding the task's chat mode) and the runtime its next launch will use; the runtime it
  runs now keeps its meaning.
- **Inbox item**: gains a stable identity so it can be removed and restored; new kinds for
  a finished secondary turn and an opened PR.
- **Inbox removal**: which avisos the user removed and when; restorable by identity; the
  only record of a removal (also for PR notices); forgotten after 30 days.
- **PR check**: gains start and finish times. **PR review comment**: author, file, line,
  text, time.
- **Setup run**: command, exit code, start/finish, full log of the last setup of a task.
- **Jira issue**: gains its subtask count and done count; a parent's subtasks list.
- **Task event**: new kinds «reopened», «runtime switched», «permission asked», «permission
  expired».
- **Preferences**: first-run state («Omitir Jira», dismissed), automatic update checks.
- **Daemon health**: gains the data folder path.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: 0 raw colour values outside the token blocks and 0 uses of old token names
  in the UI sources (checked by an automated test).
- **SC-002**: 0 inline style objects outside an explicit allow-list of dynamic values.
- **SC-003**: Body text reaches at least 4.5:1 contrast on every surface it is used on, in
  both themes; dark text no longer exceeds ~12:1 on the background.
- **SC-004**: 4 font sizes (plus mono) in use, down from 13; one treatment per status axis,
  down from ~8.
- **SC-005**: A pending ask is answered from the task screen without opening the Bandeja in
  100 % of the e2e runs.
- **SC-006**: «Limpiar avisos» removes a blocking item in 0 cases, and «Deshacer» restores
  100 % of what it removed (automated tests on both sides).
- **SC-007**: The Nueva tarea dialog height changes by 0 px across origins and advanced
  options, and «Iniciar» is visible without scrolling in 100 % of its states.
- **SC-008**: At 1024 px wide, 0 screens scroll horizontally except the board columns.
- **SC-009**: Stopping the preview daemon shows the disconnected state within one retry
  interval (≤ 5 s), and the app recovers within 5 s of the daemon returning, with no reload.
- **SC-010**: 0 missing English keys (`[i18n] sin traducir`) on every screen of the English
  UI, and 0 English strings in the Spanish UI outside product names and log lines.
- **SC-011**: The theme switch reaches the terminal and the editor in under 1 s without a
  reload.
- **SC-012**: All 46 «después» screenshots exist with the same names, sizes and data as
  the «antes» ones.
- **SC-013**: All checks pass at the end: type check, unit tests, UI and API e2e, Rust
  workspace and shell tests.

## Assumptions

- **Owner decisions are binding** (recorded instead of questions): the scope is everything
  in the brief, including all of §6; a pending permission puts the task in «Te espera»;
  the checkout is the default workspace (`settings.workspace` null = checkout); stopping is
  «Detener»; the task header key never wraps and the title takes the remaining space; one
  branch, one PR, commits per §8 step, one version bump at the end (this overrides the
  brief's §0 «one PR per step»).
- The checkout default also reverses decision 57 (which says null means a worktree); both
  16 and 57 are updated in `docs/decisions.md`.
- «Chat | Terminal per session» is a canvas decision taken with the owner; it is recorded
  as a new decision. A task's chat mode stays the default runtime for its new sessions.
- «Turn finished» avisos apply to **secondary** Claude sessions only: a primary session's
  finished turn already makes the task «Te espera» (blocking Pregunta), and that stays.
- «Ver N más» in the brief's sidebar is not navigation, so it reads «N más» (the «Abrir …»
  rule is for navigation; no «Ver …» anywhere).
- Undo of «Marcar como hecha» reopens the task as «Te espera» (the agent was stopped on
  done); the user resumes the session from there.
- The default merge method is a merge commit unless the repository only allows squash or
  rebase (the daemon reads what the repo allows); the PR tab shows the method used.
- First-run state and the auto-update preference are stored as daemon preferences, so every
  window agrees.
- The Bandeja's «última actividad» line is derived from the sessions' last activity and the
  task events already served; it needs no endpoint of its own.
- Brief appendix values superseded to keep the type and radius scale: small buttons use 12 px
  text and PR chips radius 7.
- Boards marked «being drawn» (Conversacion, Cargando, AvisosGlobales, TareaLibre,
  TareaHecha, TareaCaida) may still be in progress when their task starts; the brief's text
  is followed and the screen is compared when the board lands.
- Colour-blind mode (brief §7) is out of scope.
- Native-only parts (titlebar, notifications, Dock badge, native menu, «Mostrar en Finder»)
  cannot be tried in a preview; they are checked in the desktop app by the owner.
- Mock data in the canvas (signal / delilah / fixture-parent, LIBRE-1…15) is approximated by
  `scripts/seed-preview.sh`, extended to cover the same states.
