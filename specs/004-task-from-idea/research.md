# Research: Create a task from an idea, with Claude

Checked against Claude Code 2.1.292 (`claude --help`) and the code on `feature/LIBRE-4`.

## R1. Who creates the task: the daemon, on the confirmation card's answer

- **Decision**: Claude never calls Delilah to create a task. Its confirmation card is
  an AskUserQuestion with a fixed shape: one question whose `header` is `Nueva tarea`,
  whose `question` is the title, and whose first option `Crear` carries the brief as
  its `preview`. `asks::hook` recognises that shape when the asking session is a
  project's (`@project`). When the user answers `Crear` (in the conversation view or
  the Bandeja), the daemon itself creates the backlog task from the card's title and
  brief. It then returns the answer as `Crear — creada LIBRE-7 en el backlog` (or
  `Crear — no se pudo crear: <reason>`), so Claude can tell the user the key.
- **Rationale**:
  - SC-004 holds by construction: only a "Crear" on a card creates anything; "sí" in
    prose cannot (CHK016).
  - An ask resolves once (a `oneshot`), so a double click or answering in two places
    cannot create two tasks (CHK017).
  - FR-014 needs no token: a task's sessions are not `@project`, so their cards are
    ordinary questions.
  - Claude needs no `DELILAH_URL`, CLI or MCP in its clean env (decision 9 and `env.rs`
    stay untouched).
  - The task gets exactly the brief the user saw.
- **Alternatives**:
  - A `delilah` CLI or a `curl` to `POST /api/v1/tasks` from the skill. Rejected: the
    agent's env has no daemon URL, a task's agent could do the same (breaks FR-014),
    and nothing ties the call to the user's yes.
  - An MCP tool. Rejected for the same reasons as the `ask_user` MCP in decision 13.
- **If the daemon is away**: the hook falls back, and the card shows in the terminal.
  "Crear" there creates nothing. The skill tells Claude that an answer without
  `creada <KEY>` means no task was created, so it says so (spec edge case).

## R2. Handing the instructions to Claude: a session-only plugin

- **Decision**: the daemon writes a small plugin to `$DELILAH_HOME/plugins/delilah/`
  when it starts. It holds `.claude-plugin/plugin.json` and
  `skills/task-from-idea/SKILL.md`, embedded in the binary with `include_str!`. The
  daemon passes it with `--plugin-dir` to **project sessions and idea conversations
  only**. Idea conversations also get
  `--append-system-prompt "<one paragraph: you are in a Delilah idea conversation; follow
  the task-from-idea skill from the first message>"`. Both flags are passed again on
  every launch, including `--resume`.
- **Rationale**: `--plugin-dir` loads a plugin "for this session only" (CLI help). Nothing
  goes in `~/.claude` or the repo. Project sessions get the skill too (US3), and the user
  can also invoke it as `/delilah:task-from-idea`. Task sessions never get it.
- **Alternatives**:
  - Only `--append-system-prompt`. Rejected: project sessions would carry the whole
    protocol in every system prompt, and the user could not invoke it.
  - `--add-dir` with a `.claude/skills` folder. Rejected: it is not documented as
    loading skills.

## R3. Read-only without plan mode: `dontAsk` and an allow list

- **Decision**: idea conversations start with `--permission-mode dontAsk`. Their settings
  file (`tasks/@<project>/claude-<id>.json`) adds `permissions.allow`:
  - `Read`, `Grep`, `Glob`, `AskUserQuestion`, `Skill`, `TodoWrite`;
  - read-only Bash: `Bash(git status:*)`, `Bash(ls:*)`, plus their `rtk` forms.

  Final list and reasons (no `rg`, no `git log/show/diff`; `--setting-sources user`):
  decisions.md #26. It also adds `permissions.deny` for `Edit`, `MultiEdit`, `Write` and
  `NotebookEdit`. Anything else
  is denied without a prompt. Project sessions keep `--permission-mode plan`, unchanged.
- **Rationale**: in plan mode Claude can call ExitPlanMode, whose approval only shows in
  the TUI (FR-005). In `dontAsk` nothing waits on a terminal prompt. CHK011: Claude
  reads and searches, and only runs `git status` and `ls`.
- **To verify in WP-1's spike**:
  1. `dontAsk` with the allow list never prompts.
  2. AskUserQuestion still reaches the PreToolUse hook.
  3. `Skill` loads the plugin's skill.

  If (2) fails, AskUserQuestion stays allowed and the hook's `permissionDecision: allow`
  covers it.
- **Alternatives**: `--disallowedTools` alone. Rejected: tools outside the list (web,
  MCP) would still prompt.

## R4. The conversation's history: a table fed by hooks and the view

- **Decision**: a new table `conversation_entries`, filled only for sessions with
  `conversation = 1`, from these sources:
  - **User message**:
    - sent from the view: stored at once (`queued` if Claude is working, `sent` if not);
    - `UserPromptSubmit` (`prompt`): marks the matching `sent` entries delivered, or
      stores a new user entry if it was typed in "Ver terminal" (CHK030).
  - **Claude's reply**: `Stop`'s `last_assistant_message` (already used by the Bandeja,
    `inbox.rs:72`).
  - **Card**: stored when the ask arrives. Updated with the answer, with the created
    key, or marked withdrawn (Detener) or answered in the terminal (released).
  - **Activity**: `PostToolUse` (tool name plus file or pattern), kept in memory on the
    live session and not stored. The view shows only the latest one ("Leyendo
    `api.rs`", CHK002).
- **Rationale**: every source already reaches `hooks.rs` and `asks.rs`. The terminal
  scrollback is useless for a chat view, and the transcript JSONL is Claude's private
  format. The entries live in SQLite, so they survive restarts (FR-016). Claude itself
  resumes with `--resume`, as project sessions already do.
- **Alternatives**:
  - Reading Claude's transcript JSONL. Rejected: an undocumented format that changes
    between versions.
  - `stream-json`. Rejected: a whole new launch path (it was discussed with the user
    and left for later).

## R5. Queue and "Detener": the daemon holds the queue

- **Decision**:
  - While the session is `working`, a message sent from the view is stored `queued` and
    **not** typed into the PTY.
  - When the turn ends (`Stop`, or after Detener), the daemon types every queued
    message as one prompt, joined by blank lines, in order (CHK005), and marks them
    `sent`.
  - A queued message can be removed before then (CHK004). Editing one means removing
    it and writing it again.
  - "Detener" writes `ESC` to the PTY, releases the session's pending asks (their cards
    show as withdrawn, CHK006) and sets the session to `waiting`. If nothing is queued,
    Claude waits. If something is, it is delivered at once.
- **Rationale**: the TUI's own queue (how it merges messages, whether `UserPromptSubmit`
  fires on typing or on processing) is not documented. Holding the queue in the
  daemon makes the order and the state deterministic and testable. Whether `Stop`
  fires after `ESC` is not documented either, so the daemon sets the state itself.
- **Alternatives**: typing straight into the TUI while it works. Rejected for the
  reasons above.

## Spike results (2026-10-21)

WP-1 / T001, for R2, R3 and R5. `claude --version`: `2.1.292 (Claude Code)`. Run in preview 3
(API 7503) against the fixture repo `fixtures/signal`, registered as project `signal`.
The settings file `fixtures/preview-home-3/tasks/@signal/claude-spike.json` is a copy of a
real `@signal` project session's settings (same hooks, session id `spike`, no statusLine).
It adds R3's `permissions.allow`/`deny` lists verbatim. The AskUserQuestion PreToolUse
command is prefixed with `tee -a …/spike-ask-input.jsonl |` because the ask endpoint does
not write `hooks.jsonl`. A minimal plugin (`.claude-plugin/plugin.json` named `delilah`,
plus `skills/task-from-idea/SKILL.md`) sits in a tmp dir. Every run used a clean env
(`env -i HOME USER LANG PATH=/opt/homebrew/bin:/usr/local/bin:/usr/bin:/bin`) and this
base command, with `$S` the tmp dir and `$D` the task dir:

```
claude --settings $D/claude-spike.json --permission-mode dontAsk --plugin-dir $S/plugin \
  --append-system-prompt "You are in a Delilah idea conversation; …"
```

(a)/(b)/(f-listing) added `-p --output-format json "<try Write, Edit, npm install,
git log; list delilah skills>"`. (c)/(d)/(e)/(f-invocation) ran it interactively in a
PTY driven by a Python `pty.fork()` script. That script answers through
`POST /api/v1/asks/{id}/answer` and reads `hooks.jsonl`.

- **(a) PASS. Edit/Write and `npm install` are refused with no prompt.** With `deny` on Edit
  and Write, those tools are removed from the session altogether: Claude reports "no Write
  tool in this session". It is not a per-call denial. `npm install` came back as
  `Permission to use Bash has been denied because Claude Code is running in don't ask mode`
  and is listed in `permission_denials`. The run ended normally and no file was created.
- **(b) PASS, with a caveat. `git log` runs** under `--setting-sources project,local`:
  `a2b8b17 Initial svc-orders`, and `ls` ran too. With the default sources (user settings
  loaded), a **user-level PreToolUse hook that rewrites the command** broke the allow
  match. On this machine RTK rewrote `git log` to `rtk git log`, and that call was denied
  (`permission_denials: [{"command":"rtk git log --oneline -3"}]`). User hooks still run
  next to `--settings`, and allow rules match the command *after* the rewrite.
- **(c) PASS. AskUserQuestion reaches the PreToolUse hook in `dontAsk`.** The hook got
  `hook_event_name: PreToolUse`, `tool_name: AskUserQuestion`, `permission_mode: dontAsk`,
  and option 0 was `{"label":"Crear","description":"Crea la tarea","preview":"## Idea\nPrueba de spike"}`.
  The daemon held it as an ask. `/api/v1/inbox` listed it, with `preview` intact.
- **(d) PASS. The answer reaches Claude verbatim.** The answer
  `"Crear — creada LIBRE-9 en el backlog"` produced the transcript `tool_result`
  `The user answered: "¿Creo esta tarea?"="Crear — creada LIBRE-9 en el backlog". …`
  and `Stop`'s `last_assistant_message` was `<<<Crear — creada LIBRE-9 en el backlog>>>`.
  The em dash and accents survived. `PostToolUse` fired for AskUserQuestion.
- **(e) PASS for stopping. `Stop` does NOT fire after ESC.** The test typed a long turn
  ("count to 400 in Spanish words"), waited 6 s after `UserPromptSubmit`, then wrote
  `\x1b`. The TUI showed `Interrupted · What should Claude do instead?`. No `Stop`,
  `StopFailure` or other hook event followed within 10 s. The next prompt ("PONG") ran
  normally and produced a single `Stop`. Event sequence: `UserPromptSubmit`,
  (ESC: nothing), `UserPromptSubmit`, `Stop`. This confirms R5: after Detener the daemon
  must set the state and deliver the queue itself.
- **(f) PASS. The plugin's skill is listed and invocable.** `-p` listed
  `delilah:task-from-idea`. Typing `/delilah:task-from-idea …` in the TUI ran it
  and led to the AskUserQuestion above. In `dontAsk`, Claude's own `Skill` tool call
  `{"skill":"delilah:task-from-idea"}` was allowed (`permission_denials: []`) and printed
  the skill's marker.

Consequences: neither (a), (c) nor (d) failed, so the R3 fallback is not needed.
Watch for user-level hooks that rewrite Bash commands (R3's read-only Bash allow list
won't match them). Either add the rewritten forms, or accept that read-only Bash may be
denied on such machines. Do not pass `--setting-sources`, which would also drop the
user's other settings.

## R6. Question cards with detail: `preview`

- **Decision**:
  - `AskQuestion` options gain `preview?: string`, which Claude's AskUserQuestion
    already supports.
  - `AskCard` renders the selected or first option's preview as Markdown (the existing
    `Markdown` component), open and scrollable, up to 60 % of the view's height
    (CHK015). This also improves the Bandeja and task screens.
  - A card whose `proposal` is set is the **confirmation card**: title as heading,
    brief open, fixed buttons Crear / Cambiar algo / Descartar (CHK007). After the
    answer it shows the key as a link to the task (CHK010).

## R7. Smaller decisions (checklist items accepted)

| Item | Decision |
|---|---|
| CHK001 empty state | A hint: "Cuéntale tu idea. Claude mirará el código, te preguntará lo que necesite y te propondrá una tarea." |
| CHK003 states | `starting`, `working` (with activity), `waiting`, waiting on a card, `paused` (limit), stopped (not live: "Reabrir", CHK028), `needs terminal` (a `Notification` of type `permission_prompt`: "Claude espera en la terminal" with "Ver terminal"). |
| CHK008 label | "Idea" until the first message, then its first 40 characters. |
| CHK009 close | As project sessions: closing hides it, and the history is kept. Deleting is out of scope. |
| CHK012 FR-005 | Checked by the spike and by a test: the idea conversation's command line has `dontAsk` and no `plan`. |
| CHK013 long brief | Always passed as notes (`notes.md`, the existing pointer), never inline. No threshold. |
| CHK014 mark | The notes start with `> Brief escrito por Claude en la conversación «<label>», <date>.`, which the user may edit. The history event stays (FR-015). |
| CHK018/019 | Extra task sessions are task-owned, so cards are plain. A project session after "Permitir cambios" can still create tasks. After "Convertir en tarea" it is task-owned and relaunched without the plugin, so it cannot. |
| CHK020 threat | Unchanged: the API is loopback and Origin-guarded (`api.rs` guard). Any local process could already `POST /api/v1/tasks`. |
| CHK021 where | `in_checkout: None`, as `delilah new`: the project's default applies when the task is started. |
| CHK022 title | The same check as `create_task` (non-empty after trim). Refused cards answer `no se pudo crear: …`. |
| CHK024 history | Event `created-from-conversation { session, label }`, shown as "Creada desde la conversación «label»". |
| CHK025 Bandeja | Idea conversations' cards also appear in the Bandeja. Answering in either place resolves the same ask. |
| CHK026/027 | SC-002 is measured on the main path without "Ver terminal". SC-001 counts from the click on "Crear". |
| CHK029 project removed | Its sessions and entries are deleted with it (`ON DELETE CASCADE`), as project sessions today. |
