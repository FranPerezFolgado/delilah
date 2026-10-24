# Feature Specification: Task branch names that describe the task

**Feature Branch**: `feature/LIBRE-6`

**Created**: 2026-10-22

**Status**: Draft

**Input**: User description: "Las tareas deben crearse con nombre de rama con sentido en base a la tarea que se esta haciendo. Por ejemplo esta deberiamos ver la forma de ponerle un nombre relacionado a branch names. Siempre con feature/ delante"

## User Scenarios & Testing *(mandatory)*

### User Story 1 - A started task gets a branch named after what it does (Priority: P1)

The developer starts a task (a Jira ticket or a free task). Today the branch is
`feature/<KEY>` (`feature/LIBRE-6`), which says nothing in `git branch`, on GitHub or
in the PR list. From now on Delilah names the branch after what the task is about,
always starting with `feature/`. This task, for example, would get something like
`feature/task-branch-names`; a Jira ticket `PROJ-12 "Retry failed payments"` would
get `feature/PROJ-12-retry-failed-payments`.

**Why this priority**: it is the whole feature. Without it nothing changes.

**Independent Test**: start a free task and a Jira task in a fixture repo and
check with `git branch` that each branch starts with `feature/`, describes the task
and that the agent, the PR loop and archiving all work on that branch.

**Acceptance Scenarios**:

1. **Given** a free task titled "Las tareas deben crearse con nombre de rama con sentido…",
   **When** it starts, **Then** its branch is `feature/<short-name>`: lowercase, words
   joined by `-`, only `a-z`, `0-9` and `-`, about what the task does (e.g.
   `feature/task-branch-names`), without the internal `LIBRE-n` key.
2. **Given** a Jira task `PROJ-12` with summary "Retry failed payments", **When** it
   starts, **Then** its branch is `feature/PROJ-12-<short-name>` (e.g.
   `feature/PROJ-12-retry-failed-payments`), so Jira can still link the branch to the ticket.
3. **Given** a task started in the project's checkout or in a worktree, **When** it
   runs, **Then** the agent, `Abrir PR`, the PR loop, archive and delete all use the
   new branch, as they use `feature/<KEY>` today.
4. **Given** a task that already ran before this feature (branch `feature/PROJ-1`),
   **When** it is resumed, archived or its PR is opened, **Then** it keeps its
   existing branch; nothing is renamed.

---

### User Story 2 - See and change the name before starting (Priority: P2)

When starting a task (new task dialog, starting a backlog task, turning a session
into a free task), the developer sees the branch name Delilah proposes and can edit
it before confirming. Whatever they write still ends up with `feature/` in front.

**Why this priority**: the automatic name will sometimes be wrong or too long; the
developer must be able to fix it without going to git by hand afterwards.

**Independent Test**: open the start dialog for a task, check the proposed name,
change it to "my fix", start, and check the branch is `feature/my-fix`.

**Acceptance Scenarios**:

1. **Given** the start dialog of a task, **When** it opens, **Then** it shows the
   proposed branch name, editable.
2. **Given** the developer types `Arreglo del login` (or `feature/arreglo-del-login`),
   **When** they start, **Then** the branch is `feature/arreglo-del-login`: never
   `feature/feature/…`, never without the prefix.
3. **Given** the developer types a name that is not valid for git (spaces, `..`, `~`,
   only symbols), **When** they start, **Then** it is normalised like an automatic
   name, or, if nothing usable is left, the dialog says why and does not start.
4. **Given** a task started from the CLI or API with an explicit branch, **When** it
   starts, **Then** that branch is used (with the `feature/` prefix added if missing).

---

### User Story 3 - The branch name is visible on the task (Priority: P3)

In the task view the developer can see which branch the task works on, so they can
find it in their editor, terminal or GitHub.

**Why this priority**: with names that are no longer the key, the branch can't be
guessed from the task any more.

**Independent Test**: open a started task and read its branch name in the task view.

**Acceptance Scenarios**:

1. **Given** a started task, **When** the developer opens it, **Then** the task view
   shows its branch name.

---

### Edge Cases

- **Branch already exists** (an earlier task, a deleted task whose branch stayed, or
  one made by hand): Delilah does not reuse or overwrite it; it adds `-2`, `-3`, … until
  the name is free, in every repo the workspace touches (submodules included).
- **Accents and other alphabets**: `á é í ó ú ü ñ ç` become `a e i o u u n c`; characters
  with no ASCII equivalent are dropped; emojis and punctuation are dropped.
- **Long titles**: the part after `feature/` is at most 50 characters, cut at a word
  boundary, without a trailing `-`.
- **Title with nothing usable** (only symbols or emojis) and no name from Claude:
  `feature/<KEY>` lowercased for free tasks (`feature/libre-6`), `feature/<KEY>` for Jira.
- **Claude can't name it** (offline, signed out, usage limit, slow or unusable
  answer): the name is a mechanical slug of the title (accents removed, filler words
  like `de`, `la`, `the` dropped, first 4–6 words), or `feature/<KEY>` if that is empty
  too. The task still starts, and never waits more than a few seconds for a name.
- **Claude's answer is not a clean name** (a sentence, quotes, a `feature/` of its
  own): it goes through the same normalisation as any name.
- **Session promoted to a free task** (works in the checkout, on a branch there): it
  gets a meaningful branch the same way.
- **`parent` projects with submodules**: the same branch name in every submodule the
  task touches, as today.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: Every branch Delilah creates for a task MUST start with `feature/`.
- **FR-002**: Without an explicit name, the branch MUST be derived from what the task
  is about: the Jira summary, or the free task's title (and notes when the title is
  not enough): Claude summarises the task in 2–4 English words, proposed when the
  start dialog opens (or when a task starts without the dialog: CLI, API).
- **FR-003**: Jira tasks MUST keep their ticket key right after the prefix
  (`feature/PROJ-12-<short-name>`); free tasks MUST NOT include their internal
  `LIBRE-n` key.
- **FR-004**: The part after `feature/` (and after the key, for Jira) MUST be
  lowercase ASCII letters, digits and single `-`, with no leading or trailing `-`, and
  at most 50 characters, cut at a word boundary.
- **FR-005**: Accented Latin letters MUST be transliterated (`ñ`→`n`, `á`→`a`, …);
  other characters MUST be dropped.
- **FR-006**: If the branch already exists in any repo of the workspace, Delilah MUST
  pick the first free `-2`, `-3`, … suffix instead of reusing it.
- **FR-007**: The start flows in the app (new task, backlog start, session → free
  task) MUST show the proposed name and let the developer edit it before starting.
- **FR-008**: A name typed by the developer or passed in the CLI/API MUST get the
  `feature/` prefix if it doesn't have it, never twice, and MUST be normalised by the
  same rules; if nothing valid is left, the start MUST be refused with a clear message.
- **FR-009**: The chosen name MUST be the one stored for the task, and the agent, the
  PR flow, archive and delete MUST keep using that stored name, never recompute it.
- **FR-010**: Tasks started before this feature MUST keep their current branch.
- **FR-011**: The task view MUST show the branch the task works on.
- **FR-012**: If Claude can't produce a name within a few seconds, Delilah MUST fall
  back to a mechanical slug of the title (accents removed, filler words dropped,
  first 4–6 words); if that is empty too, to `feature/<KEY>` (lowercased for free
  tasks). The task MUST start anyway.
- **FR-013**: Asking Claude for a name MUST NOT start an agent session, touch the
  task's workspace or show up as a task or session; it only reads the task's title
  and notes.

### Key Entities

- **Task branch name**: `feature/` + (Jira key + `-`, for Jira tasks) + short name +
  (optional `-N` to be unique). Decided once, when the task starts, and kept for the
  task's whole life.
- **Task**: key, title (Jira summary or free title), notes, kind (Jira or free); the
  source of the short name.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: 100% of branches created for tasks after this feature start with `feature/`.
- **SC-002**: For new tasks, the developer can tell what a task is about from its
  branch name alone, without opening Delilah or Jira (checked on 10 sample tasks:
  at least 9 of 10 names judged meaningful).
- **SC-003**: No task fails to start because of its branch name; a collision or a
  failed name falls back to a working name every time.
- **SC-004**: The proposed name appears in the start dialog within 3 seconds, and
  starting a task takes no more than 3 seconds longer than today.
- **SC-005**: No branch name exceeds `feature/` + key + 50 characters (+ suffix).

## Assumptions

- The `feature/` prefix is fixed for every task and workflow in this feature; a
  per-project template (`delilah.toml`, `feature/{ticket}-{slug}` in the vision) is
  out of scope and can come later on top of this.
- Keeping the Jira key in Jira branches is wanted, so Jira's development panel and
  team conventions keep linking branches to tickets.
- Branches are never renamed after the task starts (renaming would break open PRs
  and the stored run); a wrong name is fixed in the start dialog, not later.
- The task's key (`LIBRE-n`, `PROJ-1`) still names the workspace folder and
  everything else inside Delilah; only the git branch changes.
- Existing tests that write `feature/PROJ-1` into their setup keep doing so: they
  build the branch themselves.

## Clarifications

### Session 2026-10-22

- Q: How is the short name produced? → A: Claude summarises the task in 2–4 English
  words when the start dialog opens; a mechanical slug of the title is the fallback
  (option B).
