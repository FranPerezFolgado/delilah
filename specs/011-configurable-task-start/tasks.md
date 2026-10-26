# Tasks: Configurable workflow commands per project

**Input**: Design documents from `specs/011-configurable-task-start/`
**Prerequisites**: plan.md, spec.md, research.md, data-model.md, contracts/api.md, quickstart.md

**Tests**: test-first (project convention for orchestrated features): each "Write failing tests"
task comes before the task that implements it.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: can run in parallel (different files, no dependency on unfinished tasks)
- **[Story]**: US1 built-in start · US2 pick a skill · US3 phased workflow · US4 per-task override

---

## Phase 1: Setup

- [X] T001 Create `crates/delilah-core/src/workflow.rs` (empty module with doc comment) and export it from `crates/delilah-core/src/lib.rs`

---

## Phase 2: Foundational (core model, storage, migration)

**⚠️ Blocks every user story.**

- [X] T002 Write failing tests in `crates/delilah-core/src/workflow.rs`: serde of `ProjectWorkflow` (`single`/`builtin|message|nothing`, `phased`) matching data-model.md JSON; `TaskWorkflow::parse` (`project`, `prompt`, `none`, `feature`→`Project`, unknown → error) and `as_str`; `render` with `{ticket}` `{title}` `{task}` `{spec_dir}` and `{{`/`}}` literals; `{task}` is the ticket for a Jira key and the title for a free key; `validate` codes `workflow.blank`, `workflow.unknown_placeholder`, `workflow.placeholder_not_allowed` (`{spec_dir}` outside `implement`), `workflow.unbalanced_brace` with field names `workflow.start.template`, `workflow.plan|implement|review`
- [X] T003 Implement T002 in `crates/delilah-core/src/workflow.rs` (`ProjectWorkflow`, `Start`, `TaskWorkflow`, `Vars`, `render`, `validate`)
- [X] T004 Write failing tests in `crates/delilah-core/src/workflow.rs` for `is_phased(TaskWorkflow, &ProjectSettings)` and `resolve(request, task, project, builtin_text)` covering every row of the precedence table in data-model.md (request prompt / `prompt` / `none` / `project` / `feature`; stored `prompt` with and without `start_message`; project single builtin/message/nothing and phased; no project), asserting `prompt: Option<String>`, `phased` and the stored `(workflow, start_message)`
- [X] T005 Implement T004 in `crates/delilah-core/src/workflow.rs` (`is_phased`, `ResolvedStart`, `resolve`)
- [X] T006 Write failing tests in `crates/delilah-core/src/project.rs`: `ProjectSettings` with no `workflow` key deserialises to `None` and `workflow()` returns `single{builtin}`; `validate` reports the T002 workflow errors next to the existing ones
- [X] T007 Implement T006 in `crates/delilah-core/src/project.rs` (`workflow: Option<ProjectWorkflow>` with `#[serde(default)]`, `workflow()`, `validate` calls `workflow::validate`)
- [X] T008 Add `start_message: Option<String>` (`#[serde(default)]`) to `Task` in `crates/delilah-core/src/task.rs`, update the `workflow` doc (`project|prompt|none`) and the JSON fixtures in its tests
- [X] T009 Add `feature_preset(Lang) -> ProjectWorkflow` and `builtin_start(Lang) -> &'static str` to `crates/delilah-daemon/src/lang.rs` (texts from research R5/R6; the preset's review is today's `step_prompt` review text), with unit tests
- [X] T010 Write a failing migration test in `crates/delilah-daemon/src/store.rs` (tests module): seed a DB migrated to 0021 with projects (`settings` `{}` and with a branch template) and tasks (`feature`, `prompt`, `bugfix`), and `pref:language` unset / `"en"`; after migrating, projects have `$.workflow` equal to `feature_preset(Lang::Es)` / `(Lang::En)`, existing keys kept; tasks are `project`, `prompt`, `prompt`; `start_message` is NULL; running the migration SQL twice changes nothing
- [X] T011 Implement T010: `crates/delilah-daemon/migrations/0022_workflow_commands.sql` per data-model.md §Migration (`CASE` on `settings` `pref:language`)
- [X] T012 Read and write `start_message` in `crates/delilah-daemon/src/store.rs` (task SELECTs, insert, update/`change` paths) with a round-trip test
- [X] T013 Replace every hard-coded `"feature"` task workflow with `TaskWorkflow::Project.as_str()` in `crates/delilah-daemon/src/actions.rs`, `conversation.rs`, `session_api.rs`, `asks.rs`, `pr_loop/mod.rs`, `proposals.rs` and `store.rs` (including test fixtures); `rg '"feature"' crates/delilah-daemon/src` must only show the preset and the alias parser

**Checkpoint**: `cargo test -p delilah-core -p delilah-daemon` green; the daemon starts on a migrated copy of a pre-upgrade state with unchanged behaviour (still the old start code).

---

## Phase 3: User Story 1 - A new user's first task works without any skill (Priority: P1) 🎯 MVP

**Goal**: start resolution in the daemon; new projects get the built-in prompt; non-phased tasks get no saving mode, no note, no autopilot; migrated projects keep `/feature`.

**Independent Test**: quickstart §1 and §2.

- [X] T014 [US1] Write failing tests in `crates/delilah-daemon/src/run.rs` (tests module, fake launcher as existing start tests do): project without `workflow` + free task with notes → first prompt is `builtin_start` text + notes pointer, single "Principal" session, `saving_mode=false` with the saving-mode preference on; project with the `/feature` preset → `/feature <title>` for a free task and `/feature PROJ-1` for a Jira task, plus the saving-mode note when on (today's output byte for byte); `autopilot: true` on a non-phased start → 409 `not-spec-workflow`; no project → built-in prompt
- [X] T015 [US1] Implement T014 in `crates/delilah-daemon/src/run.rs`: replace the `match (&req.prompt, workflow)` block (~L321-338) with `workflow::resolve` (built-in text from `lang::builtin_start`); force `saving_mode=false`/`autopilot=false` when not phased before they are stored; store the resolved `workflow`/`start_message` on the task; `first_session` takes `Option<String>` and adds the saving-mode/autopilot note only when phased; `point_to_notes` only when a prompt is sent
- [X] T016 [US1] Change `allowed(key, task_workflow, &ProjectSettings)` in `crates/delilah-daemon/src/autopilot/mod.rs` to delegate to `workflow::is_phased`, with tests; update its callers in `crates/delilah-daemon/src/run.rs`, `api.rs` (task create, `PATCH`, autopilot endpoint) and `proposals.rs`
- [X] T017 [US1] Add a test in `crates/delilah-daemon/src/autopilot/notes.rs` that no autopilot note contains a slash command (FR-010)
- [X] T018 [US1] Add an e2e flow in `crates/delilah-e2e/tests/` (fake Claude, `e2e` feature, run with `scripts/e2e.sh api`): fresh project, start a free task with notes → the fake Claude's prompt argument is the built-in text + notes pointer with no `/`

**Checkpoint**: a fresh install's first task is usable without any skill; the author's projects behave as before.

---

## Phase 4: User Story 2 - Pick the skill a project starts its tasks with (Priority: P1)

**Goal**: settings validation for single starts, "nothing" launch, skill discovery endpoint, the settings UI.

**Independent Test**: quickstart §3.

- [X] T019 [P] [US2] Write failing tests in a new `crates/delilah-daemon/src/skills.rs` against a temp `HOME` and project dir: user `skills/*/SKILL.md` and `commands/**/*.md` (subfolder → `a:b`), project ones, plugins from `plugins/installed_plugins.json` filtered by `settings.json` `enabledPlugins` and named `<plugin>:<name>`; frontmatter `name`/`description`, fallback to file stem / first line; an unreadable file is skipped; same name in two sources listed twice with their source; sort order project, user, plugin then name
- [X] T020 [US2] Implement T019 in `crates/delilah-daemon/src/skills.rs` (`Skill`, `SkillSource`, `list(home, project_path)`) and register the module in `crates/delilah-daemon/src/lib.rs`
- [X] T021 [US2] Add `GET /api/v1/projects/{name}/skills` in `crates/delilah-daemon/src/projects.rs` (route in `api.rs`), running `skills::list` in `spawn_blocking`; tests: 200 with entries, 404 unknown project
- [X] T022 [US2] Localise the `workflow.*` validation codes in `crates/delilah-daemon/src/projects.rs` as the branch codes are, with a `PUT /settings` test returning field errors for `/fix {foo}` and a blank custom start
- [X] T023 [US2] Write failing tests in `crates/delilah-daemon/src/run.rs`: project `single{message:"/fix {ticket}"}` → `/fix PROJ-12` + notes pointer, no note; project `single{nothing}` → launch with no prompt, task status after launch is waiting for the user, notes not sent
- [X] T024 [US2] Implement T023 in `crates/delilah-daemon/src/run.rs` (launch with `None` through the existing no-prompt path, `launch_session_settled` for stream agents; status as after a finished turn)
- [X] T025 [P] [US2] Add `ProjectWorkflow`, `Start`, `Skill`, `Task.start_message` and `api.projectSkills(name)` to `app/src/api.ts`; `Task.workflow` doc values `project|prompt|none`
- [X] T026 [P] [US2] Create `app/src/workflowPresets.ts` (presets "Prompt integrado", "Nada", "Spec Kit (`/feature`)", "Spec Kit (oficial)" per research R6; `isPhased(taskWorkflow, settings)`) with `app/src/workflowPresets.test.ts`
- [X] T027 [US2] Write failing tests `app/src/components/SkillPicker.test.tsx`: lists skills from a mocked `api.projectSkills` with source badges, filters by search, picking `fix` returns `/fix {ticket}`; empty/failed list still allows free text
- [X] T028 [US2] Implement T027 in `app/src/components/SkillPicker.tsx`
- [X] T029 [US2] Write failing tests `app/src/components/WorkflowEditor.test.tsx`: single mode with built-in / message / nothing; applying a preset fills the fields; the "not found" warning when the template's leading `/name` is not in the list (still savable); daemon `FieldError`s shown next to `workflow.*` fields
- [X] T030 [US2] Implement T029 in `app/src/components/WorkflowEditor.tsx` (single-start part)
- [X] T031 [US2] Add the "Flujo de trabajo" section to `app/src/components/ProjectSettings.tsx` using `WorkflowEditor`, with the existing save/discard and error mapping; extend `app/src/components/ProjectSettings.test.tsx`
- [X] T032 [P] [US2] English strings for the new UI in `app/src/i18n/en/` (SkillPicker, WorkflowEditor, ProjectSettings); `app/src/i18n/coverage.test.ts` green

**Checkpoint**: a project can be switched to `/fix {ticket}` or "Nada" from settings and the next task uses it.

---

## Phase 5: User Story 3 - A phased workflow with the user's own skills (Priority: P2)

**Goal**: hand-offs send the project's commands; phased editing in the UI; saving mode/autopilot only for phased.

**Independent Test**: quickstart §4.

- [ ] T033 [US3] Write failing tests in `crates/delilah-daemon/src/handoff.rs`: `step_prompt` renders the project's `implement` with `{spec_dir}` and `review` from a phased workflow (custom commands, not `/orchestrate`/`/review`); with the `/feature` preset the output equals today's (`/orchestrate specs/x`, the review text); `try_start` on a project that is no longer phased fails with the localised "el flujo del proyecto ya no es por fases" error through the existing hand-off error path; update the existing `step_prompt` tests
- [ ] T034 [US3] Implement T033 in `crates/delilah-daemon/src/handoff.rs` (`step_prompt(lang, step, dir, &ProjectWorkflow, vars)`, project lookup in `try_start`)
- [ ] T035 [US3] Add a run test in `crates/delilah-daemon/src/run.rs`: phased project with custom plan command → first prompt is the rendered plan + saving-mode note; `PUT /settings` with a phased workflow missing `review` → `workflow.blank` (test in `projects.rs`)
- [ ] T036 [US3] Extend `app/src/components/WorkflowEditor.tsx` and its tests with the phased mode (three fields, each with the `SkillPicker`, `{spec_dir}` hint on implement)
- [ ] T037 [US3] In `app/src/components/NewTaskDialog.tsx` and `app/src/components/BacklogTaskView.tsx`, compute `autopilotAllowed`/saving-mode visibility with `isPhased` (replacing `workflow === "feature"`), with a short reason when hidden; update `NewTaskDialog.test.tsx` and `BacklogTaskView.test.tsx`
- [ ] T038 [US3] Offer "Create and launch on autopilot" in `app/src/components/ProposalCard.tsx` only when the project is phased (`isPhased`), with a test in `app/src/components/ProposalCard.test.tsx`

**Checkpoint**: a phased project with Spec Kit's official commands hands off with them; single-start projects never show saving mode or autopilot.

---

## Phase 6: User Story 4 - See and override the start for one task (Priority: P2)

**Goal**: per-task start in the new-task form, backlog and CLI; backlog keeps the message.

**Independent Test**: quickstart §5.

- [ ] T039 [US4] Write failing tests in `crates/delilah-daemon/src/api.rs`: `POST /api/v1/tasks` with `workflow:"prompt", prompt:"Investiga {title}"` stores `start_message`; `workflow:"none"`; `workflow:"feature"` stored as `project`; unknown workflow → 400; invalid template → 400 with field errors; `POST /api/v1/runs` with a `prompt` containing `{foo}` → 400 with the same field error (request prompts are templates too: literal braces are doubled); `PATCH` edits `workflow`/`start_message`; starting that backlog task later after changing the project's workflow sends "Investiga <title>" with saving mode off
- [ ] T040 [US4] Implement T039 in `crates/delilah-daemon/src/api.rs` (`CreateTask.prompt` → `start_message`, `TaskWorkflow::parse`, validation via `workflow::validate`), the `PATCH` path, and the `RunRequest.prompt` validation in `crates/delilah-daemon/src/run.rs`
- [ ] T041 [US4] Add a run test for the legacy case in `crates/delilah-daemon/src/run.rs`: a stored `prompt` task with no `start_message` and no request prompt starts with no message instead of a 400
- [ ] T042 [P] [US4] CLI in `crates/delilah-cli/src/main.rs`: `--no-prompt` on `run`/`new` (sends `workflow:"none"`), `--workflow` values `project|prompt|none|feature`, `project show` prints the workflow (kind and commands), `project unset <name> workflow`, `project add --workflow` prints a deprecation warning; parser tests in the same file
- [ ] T043 [US4] Write failing tests in `app/src/components/NewTaskDialog.test.tsx`: the "Al empezar" field shows what the project's workflow resolves to (filled with the typed ticket/title); choosing another message (via `SkillPicker`) or "Nada" sends `workflow`/`prompt` accordingly; a non-phased choice in a phased project hides saving mode and autopilot
- [ ] T044 [US4] Implement T043 in `app/src/components/NewTaskDialog.tsx` (replace the "Primer mensaje" textarea, reuse `WorkflowEditor`'s single-start part)
- [ ] T045 [US4] Show and edit the stored start (`workflow`, `start_message`) in `app/src/components/BacklogTaskView.tsx` before starting, saving via `PATCH`; tests in `BacklogTaskView.test.tsx`
- [ ] T046 [P] [US4] English strings for the new-task and backlog changes in `app/src/i18n/en/`

**Checkpoint**: all four stories work independently.

---

## Phase 7: Polish & Cross-Cutting

- [ ] T047 [P] `docs/decisions.md`: amend 15 (not always `/feature`) and 46 (autopilot ⇔ phased workflow), add a decision for per-project workflow commands (presets, `{task}`, migration to the `/feature` preset, `Project.workflow` deprecated)
- [ ] T048 [P] `docs/usage.md`: "Flujo de trabajo" settings, presets, placeholders, skill list, per-task start, `--no-prompt`, doubling literal braces in `--prompt`, the official Spec Kit preset's limit (R6); `README.md`: a newcomer line saying tasks start with a built-in prompt and how to plug in a skill
- [ ] T049 [P] `docs/status.md` (feature done; known debt: deprecated `Project.workflow`) and `docs/architecture.md` (workflow module, skills endpoint, start resolution)
- [ ] T050 Run `cargo test --workspace`, `cargo test --manifest-path app/src-tauri/Cargo.toml`, `cd app && npx tsc --noEmit && npx tsc --noEmit -p e2e && npx vitest run`, and `scripts/e2e.sh api`
- [ ] T051 Walk quickstart.md §1–§5 in a preview (`scripts/preview.sh --fresh`, and a copy of a pre-upgrade state for §2)

---

## Dependencies & Execution Order

- **Setup (T001)** → **Foundational (T002–T013)** → stories.
- **US1 (T014–T018)** needs Foundational. It is the MVP and owns `run.rs` start resolution.
- **US2**: daemon part T019–T022 needs only Foundational (parallel with US1); T023–T024 need T015 (same file `run.rs`); UI T025–T032 need only the contract.
- **US3**: T033–T034 (`handoff.rs`) need Foundational + T009; T035 needs T015; UI T036–T038 need T030.
- **US4**: T039–T041 need T015/T016; CLI T042 is independent; UI T043–T045 need T030 and T037 (same files).
- **Polish** after the stories.

Files touched by more than one story (run those tasks in order): `run.rs` (T015, T024, T035, T041), `api.rs` (T016, T040), `NewTaskDialog.tsx`/`BacklogTaskView.tsx` (T037, T044, T045), `WorkflowEditor.tsx` (T030, T036).

## Parallel Opportunities

- Foundational: T002–T005 (core `workflow.rs`) ∥ T009 (`lang.rs`) ∥ T010–T011 (migration); T013 after T003.
- After Foundational: US1 daemon (`run.rs`, `autopilot/`) ∥ `skills.rs` + endpoint (T019–T021) ∥ `handoff.rs` (T033–T034) ∥ CLI (T042) ∥ UI foundations (T025–T028).
- Docs T047–T049 in parallel at the end.

## Implementation Strategy

1. **MVP**: Setup + Foundational + US1 → new projects work without skills, the author's projects unchanged. Shippable alone (the setting is not editable yet, but the default is right).
2. **+ US2**: users plug in their own skill → the request in LIBRE-25 is met.
3. **+ US3**: saving mode and autopilot with anyone's skills.
4. **+ US4**: per-task overrides and the backlog fix.
