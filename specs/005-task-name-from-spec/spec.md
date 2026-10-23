# Feature Specification: Task name and branch from the first analysis

**Feature Branch**: `feature/LIBRE-11`
**Created**: 2026-10-23
**Status**: Draft
**Input**: User description: "Las tareas a veces las creo con nombres que no tienen sentido y no sé qué hacen; después del specify se puede saber un nombre mejor y actualizarla. Además, las tareas nuevas crean un worktree con un nombre horrible (LIBRE-X): debería crearse la tarea y, cuando hayamos hecho el primer análisis, que entonces se genere la rama. El propio Spec Kit puede crearla."

## Why

A free task is named when it is created, before anyone knows what it is really
about: a few words typed in a hurry, or the title of an idea that changed shape
during the conversation. Its branch and, in worktree mode, its folder are named
after the internal key (`feature/LIBRE-11`, `workspaces/LIBRE-11`), which says
nothing in `git branch`, in the PR, or in Finder. After the first analysis (the
specify step) the feature has a clear name and a short slug — Spec Kit already
produces one for its spec folder (`005-task-name-from-spec`). That is the moment
to name the task and to create its branch. The worktree folder can keep any
name: only the branch is seen in git, the PR and the remote.

## Clarifications

### Session 2026-10-23

- Q: In projects where specify doesn't create the branch (no Spec Kit git extension, or its hook off), what happens? → A: Delilah creates the branch itself, named after the new spec folder, as soon as the folder appears and the workspace has no branch yet.
- Q: Which text becomes the new title? → A: The spec's title exactly as written (its `# Feature Specification: <name>` heading), whatever its language; the user can edit or restore it.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - The task takes its name from the analysis (Priority: P1)

The user creates a free task with a vague title ("arreglar lo de las ramas") and
starts it. Once the first analysis is written, the task's title on the board, the
backlog, the task screen and the Bandeja becomes the feature's name from the
analysis. The old title is still visible on the task screen and can be restored.

**Why this priority**: It is the part the user notices every day (the board), it
works on its own without touching git, and it is the cheapest to deliver.

**Independent Test**: On a fixture project, start a free task with a meaningless
title, let the first analysis be written, and check the title changes everywhere
the task is shown, and that restoring brings the old one back.

**Acceptance Scenarios**:

1. **Given** a running free task whose first analysis has just been written, **When** Delilah notices it, **Then** the task's title becomes the feature name from the analysis, without a restart, everywhere the task is shown.
2. **Given** a task renamed this way, **When** the user opens it, **Then** the previous title is shown as such and a "restore" action sets it back.
3. **Given** a task renamed this way, **When** the analysis is edited later (clarify, re-specify), **Then** the title does not change again on its own.
4. **Given** a task whose title the user edited by hand after the automatic rename, **When** anything else happens in the analysis, **Then** the user's title is kept.

---

### User Story 2 - The branch is named after the analysis, not the key (Priority: P1)

The user starts a free task. Its workspace is prepared on the project's base
branch without a branch of its own (a worktree, in worktree mode, under any folder
name). The first analysis (specify) creates the task's branch, with the descriptive
name of the analysis (the same `NNN-short-name` as its spec folder), and Delilah
picks it up and applies it to every repo the task works in (the parent and each
chosen submodule). From then on, commits, the PR,
the archive on merge and deleting the task all use that branch.

**Why this priority**: It is the other half of the user's request, and the branch
name reaches the remote and the PR, where it stays.

**Independent Test**: Start a free task on the fixture repo and on the fixture
parent with a submodule, in checkout mode and in worktree mode; check no branch
exists before the analysis, and after it that `git branch` shows the descriptive
branch in every repo of the task and no `feature/LIBRE-N`; then open a PR and
archive it.

**Acceptance Scenarios**:

1. **Given** a free task just started, **When** its first session begins, **Then** its workspace is on the project's base commit and no branch for the task exists in any of its repos.
2. **Given** the first analysis has created the branch, **When** the user looks at the task's repos, **Then** each is on the branch named like the spec folder, and the task's workspace record names that branch.
3. **Given** the task has its descriptive branch, **When** the user opens a PR, archives the task, or it is merged and archived, **Then** each works on that branch as today with the key-named one.
4. **Given** a checkout-mode task before its analysis, **When** the session works, **Then** nothing is ever committed to the project's base branch.

---

### Edge Cases

- **No analysis**: the task never runs specify (a quick fix, a non-Spec Kit workflow, the user goes straight to code). Before the first change is made the task still needs a branch: it gets one named from its current title (slug), never from the key, and never the base branch.
- **Analysis run twice / several spec folders**: only the first analysis of the task names it; later spec folders don't rename again.
- **Name taken**: a branch with the descriptive name already exists (an old task, a remote branch). The task gets a free variant (the next number) and says so; it never reuses or overwrites the other one.
- **Branch already exists for the task**: once the task has its branch (from the analysis or the title fallback), later analyses never change it.
- **Checkout not clean / held by another task**: same rules as today; the start fails back to the backlog before anything is named.
- **Branch can't be applied** (a submodule refuses to switch): the task keeps working where it is, the title still updates, and the Bandeja shows the failure with a retry.
- **Tasks that already exist** with `feature/LIBRE-N` (started before this feature): left as they are.
- **Task created from an idea** (Claude proposed the title): still renamed after its analysis, like any free task.
- **Spec in a submodule** of a parent project: the analysis may be written in the parent or in a submodule; the name is the same and applies to every repo of the task.
- **Daemon restarted** between the analysis and Delilah picking it up: it is picked up on the next start.
- **Worktree folder**: named however is convenient (today, after the key); its name doesn't matter to the user and never changes.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: Starting a free task MUST NOT create a branch for it; its workspace (checkout or worktree) starts on the project's base branch commit. The worktree folder's name is not constrained.
- **FR-002**: The task's branch MUST be created by the first analysis (specify), and Delilah MUST detect it and the analysis from the repo itself (spec folder and git state), in both checkout and worktree mode, for repo and parent projects.
- **FR-002a**: When the first spec folder appears and the workspace still has no branch (the project has no branch hook, or it is off), Delilah MUST create the branch itself with the spec folder's name, carrying over the uncommitted spec files.
- **FR-003**: On that detection, Delilah MUST set the task's title to the feature name of the analysis, keep the previous title, and show the change on every view of the task without a restart.
- **FR-004**: The user MUST be able to restore the previous title, and to edit the title of a running task by hand.
- **FR-005**: The automatic rename MUST happen at most once per task, and never after the user has edited the title by hand following it.
- **FR-006**: After the analysis, every repo of the task MUST be on a branch named like the analysis' spec folder (`NNN-short-name`), and the task's workspace record MUST name that branch. The task's key MUST NOT appear in it.
- **FR-007**: Before a descriptive branch exists, no change of the task MUST be committed to the project's base branch; if work begins without an analysis (the session's first edit of a file outside `specs/`), Delilah MUST give the task a branch named from its current title first.
- **FR-008**: Once the task has a branch, it MUST NOT change: later analyses don't create or rename branches for it.
- **FR-009**: The running session MUST NOT be interrupted or moved: the branch appears inside the workspace it is already working in.
- **FR-010**: Name collisions with existing branches MUST be resolved with a free variant, never by reusing or overwriting.
- **FR-011**: Failing to apply the branch to a repo of the task MUST leave the task working where it was, and raise a Bandeja item with a retry; the title update does not depend on it.
- **FR-012**: The task's key MUST stay the same: URLs, the CLI, the task's notes folder and its history keep using it.
- **FR-013**: Tasks with a Jira key MUST keep today's behaviour (title from the ticket, branch `feature/<KEY>`).
- **FR-014**: Tasks started before this feature MUST keep their current branch and folder.
- **FR-015**: PR creation, archive, archive on merge, delete and the spec progress MUST work with the descriptive branch and folder exactly as they do today with the key-named ones.
- **FR-016**: An analysis written while the daemon was down MUST be picked up on the next start.

### Key Entities

- **Task**: gains the previous title (when renamed automatically) and whether the automatic rename has happened; key unchanged.
- **Workspace**: has no branch at first; it gets the descriptive one once, after the analysis. Its folder doesn't change.
- **Analysis**: the first spec folder of the task (`specs/NNN-short-name/` with its spec), which gives the feature name and the slug.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: For 100 % of free tasks that go through specify, the title and branch match the analysis within 10 seconds of the spec being written.
- **SC-002**: No new free task creates a `feature/LIBRE-N` branch.
- **SC-003**: Zero commits of a task land on the project's base branch because the branch did not exist yet.
- **SC-004**: No session is interrupted by the branch appearing; every session of the task can still be reopened with its conversation.
- **SC-005**: Opening a PR, archiving and deleting a renamed task succeed as often as for a task with a key-named branch (no new failure kinds in the fixtures).

## Assumptions

- Only free tasks (`LIBRE-N`) change; Jira keys are meaningful and Jira tools link branches by key.
- The feature name is the title of the analysis' spec, as written there (it may be in English while the original title was in Spanish); no translation; the user can edit or restore it.
- Delilah follows the repo, as it does for phases (decision 17): Spec Kit's own branch hook (`before_specify`) creates the branch inside the workspace, and Delilah reads what it left; it doesn't drive Spec Kit.
- The worktree folder name doesn't matter to the user (decided 2026-10-23): it stays as today and never moves, so sessions are never disturbed.
- The `NNN` prefix is the spec folder's sequential number in that repo.
- The rename is automatic (the user asked for the task to be updated), with restore as the safety net rather than a confirmation card.
- Checkout mode remains the default and keeps its rules (clean to start, one holder).
