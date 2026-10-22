# Tasks: Autopilot (a task that runs on its own up to the PR)

**Input**: Design documents from `specs/005-autopilot/`
**Prerequisites**: plan.md, spec.md, research.md, data-model.md, contracts/api.md, quickstart.md

**Tests**: test-first. Write each test task before its implementation and see it fail.

**Checks**: `cargo test -p <crate>`; `cd app && npx tsc --noEmit && npx vitest related --run <files>`; at the end `cargo test --workspace`, `cargo test --manifest-path app/src-tauri/Cargo.toml` and `npx vitest run`.

## Format: `[ID] [P?] [Story] Description`

## Phase 1: Setup

None: no new dependencies (`tokio` already has `process`; check its features in `crates/delilah-daemon/Cargo.toml` in T011 and enable `process` there if missing).

---

## Phase 2: Foundational — core + storage (blocks every story)

**Purpose**: the autopilot state, its events, the project field and their storage. No behaviour change.

- [X] T001 Write failing tests in `crates/delilah-core/src/task.rs`: `Autopilot { state, stage, reason, since, nudged }` round-trips as in data-model.md (`{"state":"paused","reason":"…","since":"…"}`; `stage` serde `preview`/`opening_pr`, `state` `running`/`paused`/`done`); a `Task` without `autopilot` deserialises with `None`; `Event::AutopilotPaused { reason }`, `AutopilotResumed`, `AutopilotAnswered { question, answer }`, `AutopilotNudged { step }`, `AutopilotFinished { prs }` serialise kebab-case (`autopilot-paused`, …) and leave every status unchanged (like `StepStarted`).
- [X] T002 Implement T001 in `crates/delilah-core/src/task.rs`: types, `Task.autopilot: Option<Autopilot>` (`#[serde(default)]`), `Task::new` default, the events and their no-op transitions; re-export from `crates/delilah-core/src/lib.rs`.
- [X] T003 [P] Write failing tests and implement `Project.preview_command: Option<String>` (`#[serde(default)]`, old JSON loads with `None`) in `crates/delilah-core/src/project.rs`.
- [X] T004 Add `crates/delilah-daemon/migrations/0015_autopilot.sql`: `ALTER TABLE tasks ADD COLUMN autopilot TEXT;` and `ALTER TABLE projects ADD COLUMN preview_command TEXT;`.
- [X] T005 Write failing store tests in `crates/delilah-daemon/src/store.rs` (tests module): a created task reads back `autopilot`; `set_autopilot(key, Some(&a))` persists it **and** sets `saving_mode = 1` in the same statement; `set_autopilot(key, None)` clears it and leaves `saving_mode`; `set_saving_mode(key, false)` also clears `autopilot`; the five new events appear in the task's history.
- [X] T006 Implement T005 in `crates/delilah-daemon/src/store.rs`: select the new column in every task query, `set_autopilot` (JSON text or NULL), the extra clear in `set_saving_mode`.
- [X] T007 Write failing tests and implement `preview_command` in `crates/delilah-daemon/src/store_projects.rs`: read in every project query, `set_preview_command(name, Option<&str>)` with an empty or blank string stored as NULL.

**Checkpoint**: `cargo test -p delilah-core && cargo test -p delilah-daemon` green; no behaviour change.

---

## Phase 3: Foundational — PR and preview building blocks (blocks US1, US2, US4)

**Purpose**: a reusable "open the PR" and a daemon-owned preview process, both usable without autopilot.

- [ ] T008 Write failing tests in `crates/delilah-daemon/src/pr_api/open.rs` (tests module, `FakeGh`): `open_for(app, key, repo, title, body, draft)` pushes if needed, calls `gh pr create` with `--draft` when asked, links the PR with source `opened` and returns the row; returns a `has-pr` conflict when the repo already has an open linked PR; every existing test of the `POST /tasks/{key}/prs` handler still passes.
- [ ] T009 Implement T008: extract `pub(crate) async fn open_for` from `open()` in `crates/delilah-daemon/src/pr_api/open.rs` (claim, open-PR check, `push_if_needed`, `create_and_link`), the handler calling it.
- [ ] T010 Write failing tests in a new `crates/delilah-daemon/src/preview.rs` (tests module, temp dir as workspace): `Previews::start(task, dir, command)` with `echo http://127.0.0.1:9/x; sleep 30` → `running`, `url = http://127.0.0.1:9/x`; with `echo a; echo http://one; echo http://two; sleep 30` → the first URL; with `echo oops; exit 3` → `failed`, error containing `oops`; with `sleep 30` and the URL timeout set to 300 ms (a constructor parameter, default 10 min) → `failed`; `stop(task)` kills the whole process group (a `sleep` grandchild is gone) and the view becomes `stopped`/removed; `start` while starting or running → error `running`; output beyond 200 lines keeps only the last 200.
- [ ] T011 Implement T010 in `crates/delilah-daemon/src/preview.rs`: `Previews` (mutex map task → entry), `PreviewView`/`PreviewState` (data-model.md), spawn `/bin/zsh -lc <command>` with `current_dir`, `process_group(0)`, piped stdout/stderr read line by line into a ring buffer, URL regex `https?://[^\s)'"]+`, timeout, `stop` = SIGTERM to the group then SIGKILL after 5 s (`libc::killpg` or `nix`, whichever the workspace already has), `stop_all`; emit `Topic::Tasks` on every state change. Add `previews: Arc<Previews>` to `crates/delilah-daemon/src/state.rs`, `mod preview` in `crates/delilah-daemon/src/lib.rs`, and `stop_all` on shutdown in `crates/delilah-daemon/src/main.rs`.
- [ ] T012 Write failing API tests in `crates/delilah-daemon/src/preview.rs` (router tests as in `pr_api/support.rs`): `GET /api/v1/tasks/{key}/preview` → `null` then the view; `POST` → `201` with `starting`; `409` reasons `no-preview-command`, `no-workspace`, `running`; `DELETE` → `204`; and in `crates/delilah-daemon/src/projects.rs`: `PATCH /api/v1/projects/{name}` with `preview_command` sets it, `""` clears it, `GET /api/v1/projects` lists it.
- [ ] T013 Implement T012: handlers in `crates/delilah-daemon/src/preview.rs` (workspace root from `store.workspace(key)`, command from the task's project), routes in `crates/delilah-daemon/src/api.rs`, the field in `projects::update_project` in `crates/delilah-daemon/src/projects.rs`.
- [ ] T014 Write failing tests for stopping a task's preview when the task ends: `MarkDone` and `Fail` through `AppState::transition` (`crates/delilah-daemon/src/transitions.rs`), archive and delete (`crates/delilah-daemon/src/actions.rs`) each leave no preview for the task.
- [ ] T015 Implement T014: in `AppState::change` stop the preview when the new status is terminal; in `actions.rs` archive/delete paths call `previews.stop(key)`.
- [ ] T016 [P] `scripts/preview.sh`: print `PREVIEW_URL=<ui url>` (or the API URL with `--no-ui`) as the first line of the summary, and the UI line before the API line; update its header comment.

**Checkpoint**: `cargo test -p delilah-daemon` green; a preview can be started and stopped by hand through the API.

---

## Phase 4: User Story 1 — From idea to open PR without stops (P1) 🎯 MVP

**Goal**: an autopilot task goes plan → implementation → review → (preview) → draft PR with no user action.

**Independent Test**: with the stub `Launcher`, fake PTYs and `FakeGh` (as in `handoff.rs` tests): start an autopilot task; write `tasks.md` and end the plan turn → the implementation session starts with no offer; check every task and end the turn → the review starts with the autopilot review prompt; write `review-<session>.json` with `ready` and commit on the branch, end the turn → a draft PR is opened and linked with the agent's title/body, `autopilot.state = done`, `AutopilotFinished` in the history.

- [ ] T017 [US1] Write failing tests in `crates/delilah-daemon/src/run.rs` (tests): `RunRequest.autopilot = true` stores `autopilot = {state: running}` and `saving_mode = true`; `autopilot: true` with `saving_mode: false` → `400`; the first session's prompt ends with the autopilot plan note (research R3) instead of the saving-mode note, in Spanish and English; kept for backlog tasks like `saving_mode`.
- [ ] T018 [US1] Implement T017: `RunRequest.autopilot` in `crates/delilah-daemon/src/run.rs` (and `crates/delilah-daemon/src/api.rs` where `RunRequest` lives); `autopilot_plan_note(lang, key)` next to `plan_note` in `crates/delilah-daemon/src/handoff.rs`.
- [ ] T019 [US1] Write failing tests in a new `crates/delilah-daemon/src/autopilot.rs`: `shape(task, Some(Boundary::Offer(Implement)))` → `Start(Implement)` when running; unchanged when autopilot is `None`, `paused` or `done`; `step_note(lang, step, outcome_path)` for implement (no stops between phases/packages) and review (checks, fixes, commit, PR title/body, write the outcome JSON at the given path, never push nor open a PR), both languages; `outcome_path(app, key, session)` = `<tasks dir>/<KEY>/autopilot/review-<session>.json`; `read_outcome` parses `ready`/`blocked` with optional fields, and a missing or invalid file is `blocked` with reason "the review did not report an outcome" (localised).
- [ ] T020 [US1] Write failing handoff integration tests in `crates/delilah-daemon/src/handoff.rs` (tests module): with autopilot running, a plan turn end with `tasks.md` written starts the implementation session (`/orchestrate <dir>` + the autopilot implement note), the handoff never passes through `offered`, and no inbox `handoff` item appears; all tasks checked → the review session starts with the autopilot review prompt including its outcome path; tasks without autopilot keep today's behaviour (all existing handoff tests pass).
- [ ] T021 [US1] Write failing tests for the finish in `crates/delilah-daemon/src/autopilot.rs` (`FakeGh`, temp git repo with a commit beyond base, no preview command): the review turn end with a `ready` outcome writes `stage = opening_pr`, calls `pr_loop::check_task(…, true)`, opens one **draft** PR per repo with commits and no open PR with the agent's `pr_title`/`pr_body`, then `state = done` and `AutopilotFinished { prs }`; without `pr_title`/`pr_body` the PR uses `proposal()`'s title/body; calling finish again opens nothing; with `stage = opening_pr` and a PR that `gh` already has for the branch (unlinked), recovery links it and never calls `gh pr create`.
- [ ] T022 [US1] Implement T019–T021: `crates/delilah-daemon/src/autopilot.rs` (`shape`, `step_note`, `outcome_path`, `read_outcome`, `finish`, `recover`), `Boundary::Finish` for a review turn end in `crates/delilah-daemon/src/handoff.rs` (calls `autopilot::finish` only when autopilot is running), the autopilot notes appended in `step_prompt` when the task has autopilot; `mod autopilot` in `crates/delilah-daemon/src/lib.rs`.
- [ ] T023 [US1] Write failing tests and implement the preview step of the finish in `crates/delilah-daemon/src/autopilot.rs`: a project with a preview command → `stage = preview`, the preview starts, finish waits for `running`/`failed` (bounded by the preview timeout), the PR body gets a `Preview: <url>` line; a `failed` preview → the PR still opens and autopilot does not pause; no command → straight to the PR.
- [ ] T024 [US1] Write failing tests and implement recovery on the scheduler tick in `crates/delilah-daemon/src/scheduler.rs`: a task with autopilot running and `stage = opening_pr` (or `preview`, whose process was lost) re-runs the PR part of `autopilot::recover`; nothing happens for other tasks.

**Checkpoint**: the US1 independent test passes; tasks without autopilot are unchanged (`cargo test -p delilah-daemon`).

---

## Phase 5: User Story 2 — Pause into the Bandeja when the agent gets stuck (P1)

**Goal**: every way the chain can get stuck ends in a paused autopilot with a reason, a Bandeja item and a resume that picks up where it left.

**Independent Test**: drive an autopilot task to each pause cause with the stub launcher; check `autopilot.state = paused`, its reason, the `autopilot` inbox item, that nothing else starts, and that `POST /tasks/{key}/autopilot {resume}` continues from the right step.

- [ ] T025 [US2] Write failing tests in `crates/delilah-daemon/src/autopilot.rs` for pause and resume: `pause(app, key, reason)` sets `paused`, `reason`, `since`, records `AutopilotPaused`, emits the inbox topic and sends one notification (none when the app is listening); `POST /api/v1/tasks/{key}/autopilot {resume}` → `running`, `nudged = None`, `AutopilotResumed`, then `advance`: plan step idle with `tasks.md` → implementation starts; review idle with a `ready` outcome → finish; `stage = opening_pr` → reopen; primary session working → only `running`; `{off}` → `autopilot = None`, saving mode kept; `resume` when not paused → `409 not-paused`.
- [ ] T026 [US2] Write failing tests for stalls and the nudge in `crates/delilah-daemon/src/handoff.rs`/`autopilot.rs`: a plan turn end without `tasks.md` types the nudge into the primary session (`actions::type_prompt`), records `AutopilotNudged { plan }` and sets `nudged = plan`; the next plan turn end without `tasks.md` pauses with "tasks.md not written" (localised); implement with unchecked tasks → nudge, then pause with the count; review `blocked` → pause with its reason and no nudge; a turn end that reaches a boundary after a nudge goes on normally.
- [ ] T027 [US2] Write failing tests for failure pauses: a handoff that ends `failed` (launch error) pauses with the error; `open_for` failing (push or `gh`) pauses with the error; a `ready` outcome with uncommitted changes in the workspace pauses before any push; on a parent workspace, a failure on the second repo pauses naming it and keeps the first PR linked; a later resume opens only the missing one.
- [ ] T028 [US2] Implement T025–T027 in `crates/delilah-daemon/src/autopilot.rs` (`pause`, `resume`, `advance`, stall handling, `act` handler) and `crates/delilah-daemon/src/handoff.rs` (stall at turn end, pause where a handoff becomes `failed`); route `POST /api/v1/tasks/{key}/autopilot` in `crates/delilah-daemon/src/api.rs`.
- [ ] T029 [US2] Write failing tests in `crates/delilah-daemon/src/asks.rs` for recommended answers: autopilot running and the asking session is the task's primary → every question with exactly one option ending in "(Recommended)" or "(Recomendado)" (case-insensitive) is answered at once through `updatedInput.answers`, with one `AutopilotAnswered` per question, no `AgentAsked`, no inbox item; multi-select picks only the recommended one; a question with none or two recommended → `pause` with the question's text and the ask goes on as today; autopilot off/paused or another session → unchanged.
- [ ] T030 [US2] Implement T029: `autopilot::recommended(&questions) -> Option<Map>` and the early answer in `asks::hook` in `crates/delilah-daemon/src/asks.rs`.
- [ ] T031 [US2] Write failing tests in `crates/delilah-daemon/src/inbox.rs`: a `permission_prompt` attention on a task with autopilot running pauses it ("the session is waiting for a permission", localised); a paused autopilot task lists one `autopilot` item with the reason; none when running or done; a usage-limit pause (`LimitReached` then `ResetDue`) leaves `autopilot` untouched and the next turn end continues the chain.
- [ ] T032 [US2] Implement T031: `ItemKind::Autopilot` derived from `task.autopilot` and the pause on permission attention in `crates/delilah-daemon/src/inbox.rs`.

**Checkpoint**: US1 + US2 complete through the API: the daemon side of the feature is usable with `curl`.

---

## Phase 6: User Story 3 — Choose autopilot when creating or starting a task (P2)

**Goal**: the option next to saving mode everywhere, its state visible on the task and the board.

**Independent Test**: create tasks with and without autopilot from the dialog, the backlog and the CLI; the stored flag forces saving mode; the header shows running/paused/done; the Bandeja item resumes or turns it off.

- [ ] T033 [US3] Write failing API tests in `crates/delilah-daemon/src/api.rs`/`handoff.rs`: `NewTask.autopilot` and `BacklogChange.autopilot` store it (forcing saving mode, `400` with `saving_mode: false`); `POST /tasks/{key}/autopilot {on}` on a running task with a pending `offered` handoff confirms it at once; `{on}` on a task that cannot have saving mode → `409 not-spec-workflow`; `POST /tasks/{key}/saving-mode {on:false}` clears autopilot.
- [ ] T034 [US3] Implement T033 in `crates/delilah-daemon/src/api.rs` (`build_task`, backlog PATCH) and `crates/delilah-daemon/src/autopilot.rs` (`on` → `handoff::confirm` when offered).
- [ ] T035 [P] [US3] Write failing tests and implement in `crates/delilah-cli/src/main.rs`: `--autopilot`/`--no-autopilot` on `new` and `run` (conflicting with `--no-saving-mode`), sent as `autopilot` in the body; `delilah autopilot <KEY> on|off|resume`.
- [ ] T036 [P] [US3] `app/src/api.ts`: `Autopilot` type and `Task.autopilot`, `autopilot?` on `RunRequest`/`NewBacklogTask`/`BacklogChange`, `setAutopilot(key, action)`, `InboxKind` `"autopilot"`, the new history event types (contracts/api.md).
- [ ] T037 [US3] Write failing tests in `app/src/components/NewTaskDialog.test.tsx` (and a new `BacklogTaskView.test.tsx`): ticking "Autopiloto" ticks and disables "Modo ahorro"; unticking re-enables it; the request carries `autopilot: true, saving_mode: true`.
- [ ] T038 [US3] Implement T037 in `app/src/components/NewTaskDialog.tsx` and `app/src/components/BacklogTaskView.tsx`.
- [ ] T039 [US3] Write failing tests in a new `app/src/components/AutopilotMark.test.tsx`: running → "Autopiloto"; paused → "Autopiloto en pausa" with the reason; done → "PR abierta por autopiloto"; nothing when `autopilot` is null; the "Reanudar autopiloto" / "Seguir a mano" buttons call `setAutopilot`.
- [ ] T040 [US3] Implement T039 in `app/src/components/AutopilotMark.tsx`, shown in the task header in `app/src/components/TaskScreen.tsx` and on the board card in `app/src/components/BoardView.tsx`; the paused banner with both actions in `app/src/components/HandoffActions.tsx`.
- [ ] T041 [US3] Write failing tests in `app/src/components/InboxView.test.tsx` and implement in `app/src/components/InboxView.tsx`: an `autopilot` item shows its reason with "Reanudar autopiloto" and "Seguir a mano".
- [ ] T042 [US3] Write failing tests in `app/src/chain.test.ts` and implement in `app/src/chain.ts` (and `app/src/components/SessionChain.tsx`): with autopilot on the chain adds "Preview" (only when the project has a preview command) and "PR" steps, `done` once the PR is linked, `waiting` with the pause reason when paused.
- [ ] T043 [P] [US3] English strings for everything above in `app/src/i18n/en.ts`.

**Checkpoint**: the feature is usable from the app without the preview UI and the idea card.

---

## Phase 7: User Story 4 — A preview of the branch, ready when the PR opens (P2)

**Goal**: preview command per project, preview URL and controls on any task.

**Independent Test**: set a fixture project's preview command, "Lanzar preview" on a task → URL shown; "Parar preview" → gone; archive → gone.

- [ ] T044 [P] [US4] `app/src/api.ts`: `PreviewView`, `getPreview/startPreview/stopPreview`, `Project.preview_command`, `updateProject` accepting it.
- [ ] T045 [US4] Write failing tests in a new `app/src/components/PreviewControl.test.tsx`: hidden when the task's project has no preview command; "Lanzar preview" → starting; running → the URL as a link that opens in the browser and "Parar preview"; failed → the error with "Reintentar".
- [ ] T046 [US4] Implement T045 in `app/src/components/PreviewControl.tsx`, placed in the task screen header area in `app/src/components/TaskScreen.tsx`, refreshed on the tasks topic.
- [ ] T047 [US4] Write failing tests and implement the "Comando de preview" field in `app/src/components/ProjectView.tsx` (save on blur/enter, empty clears), with its strings in `app/src/i18n/en.ts`.

**Checkpoint**: previews work for any task, and autopilot's preview step shows in the UI.

---

## Phase 8: User Story 5 — Create and launch on autopilot from an idea (P3)

**Goal**: "Crear y lanzar en autopiloto" on the idea's proposal card.

**Independent Test**: answer a proposal with `launch: "autopilot"` → the task exists with the brief as notes, autopilot and saving mode on, and is running; a failed start leaves it in the backlog with the error on the card.

- [ ] T048 [US5] Write failing tests in `crates/delilah-daemon/src/proposals.rs` (and `asks.rs` for the body field): `Answer.launch = "autopilot"` with "Crear" creates the task with autopilot and saving mode on, then starts it through `run::start_run` in the conversation's project and its default work mode; a start failure returns `{task, error}` and leaves the backlog task with autopilot on; `launch` with any other answer is ignored.
- [ ] T049 [US5] Implement T048 in `crates/delilah-daemon/src/asks.rs` (`Answer.launch`) and `crates/delilah-daemon/src/proposals.rs` (`on_answer`).
- [ ] T050 [US5] Write failing tests in `app/src/components/ProposalCard.test.tsx` and implement in `app/src/components/ProposalCard.tsx` (+ `app/src/api.ts` answer `launch`, `app/src/i18n/en.ts`): a "Crear y lanzar en autopiloto" button next to "Crear"; on success the card links to the running task; on error it shows the error.

---

## Phase 9: Polish & cross-cutting

- [ ] T051 [P] `docs/architecture.md`: an "Autopilot" subsection under saving mode (shape, stall/nudge, outcome file, finish, pause/resume, recovery) and a "Previews" section (process group, URL, lifecycle); hook→effect rows for AskUserQuestion and permission attention.
- [ ] T052 [P] `docs/decisions.md`: new decisions — autopilot as a policy over saving mode (and what was ruled out: a second engine, editing the global skills); one nudge per step and in-hook recommended answers (stretching #15/#33); autopilot pushes and opens a draft PR with the user's own `gh`; the daemon owns previews (vs the agent).
- [ ] T053 [P] `docs/status.md` (built; known limits: previews lost on restart, no time/token caps), `docs/development.md` (setting this repo's preview command to `scripts/preview.sh`), `README.md` (autopilot, previews, the idea card button).
- [ ] T054 Bump the **minor** version from `main` in `Cargo.toml`, `app/src-tauri/Cargo.toml`, `app/package.json`, `app/src-tauri/tauri.conf.json` and the lockfiles (`cargo check --workspace`, `(cd app/src-tauri && cargo check)`, `(cd app && npm install --package-lock-only)`); commit `chore: bump version to X.Y.0`.
- [ ] T055 Run every check (`cargo test --workspace`, `cargo test --manifest-path app/src-tauri/Cargo.toml`, `cd app && npx tsc --noEmit && npx vitest run`) and walk quickstart.md in a preview.

---

## Dependencies & Execution Order

- Phase 2 blocks everything (T001→T002; T004→T005→T006; T003→T007).
- Phase 3 needs Phase 2. T008→T009 and T010→T011→T012→T013→T014→T015 are independent chains; T016 anytime.
- US1 needs Phases 2–3 (`open_for`, `Previews`). Inside: T017–T018, then T019–T021 → T022, then T023, T024.
- US2 needs US1's `autopilot.rs` (T022). T025–T027 → T028; T029→T030 and T031→T032 can follow T028 in either order (different files).
- US3 daemon part (T033–T035) needs T028; its app part (T036–T043) needs only the contract, so it can start right after Phase 2.
- US4 app part needs T013 (endpoints); US5 needs T018 (`RunRequest.autopilot`).
- Polish after the stories; T054 last but one, T055 last.

## Parallel Opportunities

- Phase 3: the PR chain (T008–T009) alongside the preview chain (T010–T015) and T016.
- The app work of US3/US4 (T036–T047) alongside the daemon work of US1/US2.
- T035 (CLI), T043 (strings), T044 alongside other work.
- T051–T053 together.

## Implementation Strategy

1. **MVP**: Phases 2–4 (US1): an autopilot task reaches a draft PR through the API.
2. US2: the safety net, without which autopilot shouldn't be used daily.
3. US3: the option and its state in the app and CLI.
4. US4: preview UI and project setting.
5. US5: the idea card shortcut.
6. Docs, version bump, full check.
