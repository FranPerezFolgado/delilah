# Feature Specification: Per-project `delilah.toml`

**Feature Branch**: `feature/LIBRE-2`

**Created**: 2026-10-20

**Status**: Draft

**Input**: User description: "Per-project `delilah.toml`: a config file at the root of a
project's repo that Delilah reads when it prepares a task's workspace. It sets the
default workspace mode, the branch template, shared files and folders for worktrees, a
setup script run before the agent starts, and submodules to always include or exclude.
PR options are out of scope for now."

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Branch names follow the project's convention (Priority: P1)

A project names its branches `feature/{ticket}-{slug}`. The user puts that template in
the project's `delilah.toml`. When they start a task for ticket PROJ-1243 "Paginación
por cursor en GET /invoices", Delilah creates the branch
`feature/PROJ-1243-paginacion-por-cursor-en-get-invoices` instead of `feature/PROJ-1243`.

**Why this priority**: today every branch is `feature/<key>`, which does not match many
teams' conventions; the user renames branches by hand before opening PRs.

**Independent Test**: register a project whose `delilah.toml` sets a branch template;
start a task; the workspace is on the branch the template gives.

**Acceptance Scenarios**:

1. **Given** a project with `branch = "feature/{ticket}-{slug}"`, **When** a task for
   PROJ-1243 starts, **Then** its branch is `feature/PROJ-1243-<slug of the title>`.
2. **Given** a project without `delilah.toml`, **When** a task starts, **Then** its
   branch is `feature/<key>`, as today.
3. **Given** a task started with an explicit branch, **When** it starts, **Then** the
   explicit branch wins over the template.
4. **Given** a parent project, **When** a task starts, **Then** every selected submodule
   is on the branch the template gives.

---

### User Story 2 - Worktrees are ready to work in (Priority: P1)

A project needs a `.env` file and installed dependencies before the agent can run its
tests. The project's `delilah.toml` lists `.env` and `node_modules` as shared, and a
setup script. When a worktree task starts, Delilah brings the shared files into the
worktree, runs the setup script, and only then launches the agent.

**Why this priority**: today a worktree task starts without the untracked files the
project needs, so the agent fails its first build or test, or spends its time installing.

**Independent Test**: register a project whose `delilah.toml` shares `.env` and runs a
setup script that writes a marker file; start a worktree task; the worktree has `.env`
and the marker before the agent's first prompt.

**Acceptance Scenarios**:

1. **Given** `.env` listed as shared and present in the project's checkout, **When** a
   worktree task starts, **Then** the worktree has `.env` with the same content.
2. **Given** a shared entry marked as a link, **When** a worktree task starts, **Then**
   the worktree's entry points at the checkout's, and changes in one show in the other.
3. **Given** a setup script, **When** the workspace is ready, **Then** the script runs
   in the workspace before the agent launches, and its output is kept with the task's
   logs.
4. **Given** a setup script that fails, **When** a task starts, **Then** the agent is
   not launched, the task shows that setup failed with the end of the script's output,
   and the user can retry the start or open the workspace.
5. **Given** a shared entry missing from the checkout, **When** a task starts, **Then**
   the task starts anyway and the task's log notes the missing entry.

---

### User Story 3 - Submodules the project always needs (Priority: P2)

In a parent project, every change needs the `shared-config` submodule, and the `legacy`
submodule must never be checked out. The `delilah.toml` lists them. When the user starts
a task, `shared-config` is always included and `legacy` is never offered or included,
whatever the user or the suggestion picked.

**Why this priority**: it saves a repeated manual step on parent projects, but only
parent projects benefit.

**Independent Test**: register the fixture parent with `always = ["a"]` and
`never = ["c"]`; start a task selecting only `b`; the workspace has `a` and `b`, not `c`.

**Acceptance Scenarios**:

1. **Given** `always` submodules, **When** a task starts with any selection, **Then**
   the workspace includes them.
2. **Given** `never` submodules, **When** the user picks submodules or asks for
   suggestions, **Then** those submodules are not offered nor suggested, and a request
   that names one is refused with a message that names it.
3. **Given** a submodule in `always` or `never` that the parent does not have, **When**
   a task starts, **Then** the start is refused with a message naming the unknown
   submodule and the file.

---

### User Story 4 - The project's default workspace mode (Priority: P2)

A project whose checkout the user keeps working in sets `workspace = "worktree"`. New
tasks for it default to a worktree; the user can still choose the checkout for one task.

**Why this priority**: it removes one choice per task, but the choice already exists.

**Independent Test**: register a project with `workspace = "worktree"`; open "new task"
for it; the worktree option is preselected; start without changing it; the task gets a
worktree.

**Acceptance Scenarios**:

1. **Given** `workspace = "worktree"`, **When** the user starts a task without choosing,
   **Then** the task runs in a worktree.
2. **Given** the same project, **When** the user chooses the checkout for one task,
   **Then** that task runs in the checkout.
3. **Given** no `delilah.toml`, **When** a task starts without choosing, **Then** it runs
   in the checkout, as today.

---

### User Story 5 - See and check the project's config (Priority: P3)

The user wants to know what Delilah will do for a project before starting a task, and
to find mistakes in the file early. The project's details show the effective config (or
that there is none), and an invalid file shows its error with the line.

**Why this priority**: it helps, but the earlier stories work without it.

**Independent Test**: register a project with a `delilah.toml` that has an unknown key;
the project shows the error naming the key and the line; fix it; the project shows the
effective config.

**Acceptance Scenarios**:

1. **Given** a valid `delilah.toml`, **When** the user looks at the project (app or
   `delilah project ls`), **Then** they see the effective settings.
2. **Given** an invalid `delilah.toml`, **When** the user looks at the project or starts
   a task, **Then** they see the error with the file and the line, and no task starts
   with a half-applied config.

---

### Edge Cases

- **No file**: everything behaves as today.
- **File changed between tasks**: each task start reads the file as it is then; tasks
  already running are not affected.
- **Unknown keys or a `[pr]` table**: unknown keys are an error naming them, except the
  reserved `[pr]` table, which is accepted and ignored until PR options exist.
- **Template gives an invalid branch name** (spaces, `..`, empty slug): the slug is
  cleaned to letters, digits and hyphens; a name git still refuses stops the start with
  a message showing the name.
- **Branch from the template already exists**: the same behaviour as today for an
  existing `feature/<key>` branch.
- **Long titles**: the slug is cut to a reasonable length (assumed 40 characters) at a
  word boundary.
- **Free tasks** (`LIBRE-n`): `{ticket}` is the free task's key.
- **Shared entries in checkout mode**: nothing is copied or linked; the checkout already
  has them.
- **Shared entry path outside the project** (`../x`, absolute paths): refused as an
  invalid file.
- **Setup script that hangs**: stopped after a timeout (assumed 10 minutes) and treated
  as failed.
- **Restarted task / resumed session**: the setup script does not run again when the
  same workspace is reused; it runs again only when the workspace is created again.
- **Backlog task started later**: the config is read when it starts, not when it was
  saved to the backlog.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: Delilah MUST read an optional `delilah.toml` from the root of the
  project's checkout each time a task for that project starts.
- **FR-002**: Without the file, task starts MUST behave exactly as they do today.
- **FR-003**: The file MUST be able to set the default workspace mode (checkout or
  worktree); a choice made for one task MUST win over it.
- **FR-004**: The file MUST be able to set a branch template with the placeholders
  `{ticket}` (the task key) and `{slug}` (the task title, lower-case, with only
  letters, digits and hyphens); an explicit branch for one task MUST win over it.
- **FR-005**: The file MUST be able to list shared entries (files or folders, relative
  to the project root), each copied or linked into a new worktree; linking MUST be the
  default for folders and copying the default for files.
- **FR-006**: The file MUST be able to name a setup script that runs inside the new
  workspace, after shared entries are in place and before the agent launches, with the
  same clean environment agents get.
- **FR-007**: A setup failure (non-zero exit or timeout) MUST stop the start before the
  agent launches, show the failure on the task with the end of the output, and let the
  user retry.
- **FR-008**: The setup script's full output MUST be kept with the task's logs.
- **FR-009**: For parent projects, the file MUST be able to list submodules to always
  include and to never include; `always` is added to every selection, and `never` is
  removed from what is offered and suggested, and refused when requested.
- **FR-010**: An invalid file (syntax error, unknown key, wrong type, path outside the
  project, unknown submodule) MUST stop the task start with a message naming the file,
  the problem and, where it applies, the line; no setting MUST be half-applied.
- **FR-011**: A `[pr]` table MUST be accepted and ignored, reserved for PR options.
- **FR-012**: The project's details in the app and the CLI MUST show the effective
  config, or the file's error.
- **FR-013**: The new-task form MUST preselect the project's default workspace mode and
  its `always` submodules, and not offer its `never` submodules.
- **FR-014**: Messages MUST be translatable like the rest of Delilah (Spanish in the
  code, English in the translations).

### Key Entities

- **Project config**: the parsed `delilah.toml` of one project: workspace mode, branch
  template, shared entries (path and copy-or-link), setup script, submodules always and
  never; read at each task start, never stored as the source of truth.
- **Shared entry**: a path relative to the project root and how it reaches a worktree
  (copy or link).
- **Setup run**: one execution of the setup script for one workspace: start, end, exit
  status, output.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: For a project that sets a branch template, 100% of new tasks are on a
  branch that follows it, with no manual renaming.
- **SC-002**: For a project that shares its untracked files and has a setup script, the
  agent's first build or test in a new worktree needs no manual setup.
- **SC-003**: A mistake in `delilah.toml` is reported before any workspace is created,
  in one message that names the file and the problem.
- **SC-004**: Projects without the file see no change in how tasks start.
- **SC-005**: Starting a task with a config but no setup script takes no noticeably
  longer than today (under 1 second more).

## Assumptions

- The file lives in the repo and is versioned with it, so it is shared by everyone who
  uses Delilah on that project; Delilah only reads it.
- It is read from the project's checkout as it is on disk, not from a branch.
- For parent projects the file is at the parent's root, not in the submodules.
- The setup script is trusted like the rest of the project's code: the user registered
  the project, and the agent already runs commands in it.
- The setup script also runs in checkout mode (e.g. installing dependencies after the
  branch switch); shared entries do not.
- The slug is cut at 40 characters and the setup times out after 10 minutes; neither is
  configurable in this feature.
- The workflow, Jira settings and PR options stay where they are today; `[pr]` comes
  with the PR handling work (`feature/LIBRE-1`).
- The project's `workflow`, kind and path stay in Delilah's database; the file does not
  replace them.
