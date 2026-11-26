# Tasks: Close button on every notification

**Input**: Design documents from `specs/016-notification-close-button/`
**Prerequisites**: plan.md, spec.md, research.md, data-model.md, contracts/ui-notice.md, quickstart.md

**Tests**: requested (spec and brief: one RED test per notification that gains the X, one for toasts). Write each test first, see it fail, then implement.

**Checks**: `cd app && npx tsc --noEmit && npx vitest related --run <files>`; at the end `npx vitest run`. Never use the installed daemon (7420); try in `scripts/preview.sh`. No version bump.

All paths are relative to the repo root; UI code is under `app/src/`.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: can run in parallel (different files, no dependency on an unfinished task)
- **[Story]**: user story from spec.md (US1–US5)

---

## Phase 1: Setup

No setup: the app, kit and test tooling already exist (`app/`, vitest + Testing Library).

---

## Phase 2: Foundational (blocks every story)

**Purpose**: the shared close control, wrapper and dismissal hooks (plan §1, contracts/ui-notice.md, data-model.md).

- [X] T001 Write failing tests in `app/src/components/ui/Notice.test.tsx`: `CloseButton` is a button named "Cerrar" with class `ntc-x` and the close icon, and calls `onClick`; `Notice` renders its children then exactly one own "Cerrar" button last, keeps the caller's `className` and `role`, calls `onClose` once; when the X had focus, after closing focus moves to the next focusable element after the notice (and nothing is focused if there is none); `useDismissals` (via a small test component) hides only the dismissed id, keeps it hidden on re-render with the same sig, shows it again when the sig changes, and shows it again after A → B → A in separate renders and after the id disappears and comes back; `useDismissal(null)` forgets the dismissal
- [X] T002 Implement `CloseButton`, `Notice`, `useDismissals`, `useDismissal` in `app/src/components/ui/Notice.tsx` per contracts/ui-notice.md (`IconButton` with `icon="close"`, `label={t("Cerrar")}`; prune effect keyed on the serialised `current`; the focus move as one private helper in this file) until T001 passes
- [X] T003 Add `app/src/components/ui/Notice.css`: `.ntc { display: flex; align-items: center; }` and `.ntc-x { width: 28px; height: 28px; flex-shrink: 0; margin-left: auto; }`, on existing tokens; import it from `Notice.tsx`
- [X] T004 Export `CloseButton`, `Notice`, `useDismissals`, `useDismissal` and their prop types from `app/src/components/ui/index.ts`

**Checkpoint**: `npx vitest related --run app/src/components/ui/Notice.tsx` green; every story below can start (in parallel, files are disjoint).

---

## Phase 3: User Story 1 - Close any global bar (Priority: P1) 🎯 MVP

**Goal**: usage-limit, autopilot-paused and "y N más" bars get the X; closing hides only that bar until what it reports changes; nothing is resumed or changed.

**Independent Test**: `npx vitest related --run app/src/components/GlobalNotices.tsx`; in a preview, a paused-autopilot task's global bar closes and the task stays paused.

- [X] T005 [US1] Add failing tests in `app/src/components/GlobalNotices.test.tsx`: with two windows over 90 %, the 5 h bar's X hides only that bar; rerender at 100 % shows it again; rerender below 90 % then at 92 % shows it again; an autopilot-paused bar's X hides only that task's bar and calls neither `onOpenTask` nor any API; a new pause reason (or `since`) shows it again; the "y N más" bar's X hides it and a different N shows it again; each X is the shared one (`.ntc-x`, name "Cerrar")
- [X] T006 [US1] In `app/src/components/GlobalNotices.tsx`, build `current` (`usage:<window>` → `full`/`hot`; `pause:<key>` → `<reason>|<since>`; `more` → `<n>`), call `useDismissals(current)`, render each bar through `<Notice className="gn-bar …" role="status" onClose={() => dismiss(id)}>` and skip `hidden(id)`; keep the «te espera» toast logic untouched; adjust `app/src/components/GlobalNotices.css` only if the X misaligns

**Checkpoint**: US1 tests green.

---

## Phase 4: User Story 2 - Close the update bar in every state (Priority: P1)

**Goal**: every update state has the X with the effect in FR-004/FR-005.

**Independent Test**: `npx vitest related --run app/src/components/UpdateNotice.tsx`.

- [X] T007 [P] [US2] Add failing tests in `app/src/components/UpdateNotice.test.tsx`: `upToDate` and `error` X dispatch `{type:"close"}` and no text "Cerrar" button remains (only the icon X named "Cerrar"); `available` X dispatches `{type:"later"}` and "Más tarde" is still there; `confirmSessions` X dispatches `{type:"cancel"}` and "Cancelar" is still there; `checking` (manual), `downloading`, `installing` and `waiting` X dispatch nothing and hide the bar; a progress update while hidden keeps it hidden; moving to another kind (e.g. downloading → installing) shows it again. Update the existing test at line ~52 (`getByText("Cerrar")`) to `getByRole("button", { name: "Cerrar" })`
- [X] T008 [US2] In `app/src/components/UpdateNotice.tsx`, render every state through `<Notice>` (keep `gn-bar …` classes and roles); wire the X per state as in plan §3; remove the text "Cerrar" buttons in `upToDate` and `error`; for the hide-only kinds use `useDismissal(<kind>:<version>)` (sig `null` for the other kinds) and return `null` while hidden. Do not change `app/src/updates.ts`

**Checkpoint**: US2 tests green; US1 + US2 = every global bar closable.

---

## Phase 5: User Story 3 - Toasts keep their close, with the shared X (Priority: P2)

**Goal**: toasts use `CloseButton`; behaviour unchanged.

**Independent Test**: `npx vitest related --run app/src/components/ui/Toast.tsx`.

- [X] T009 [P] [US3] Add a failing test in `app/src/components/ui/Toast.test.tsx`: a pushed toast's close is the shared X (`.ntc-x`, name "Cerrar", no "×" text), clicking it removes the toast and runs its `onDismiss`; with several toasts "Cerrar todo" is still offered; existing auto-close tests for info toasts still pass unchanged
- [X] T010 [US3] In `app/src/components/ui/Toast.tsx` (`ToastView`, ~line 193), replace the text `×` button with `<CloseButton onClick={() => closeToast(item.id)} />`; check the 28 px X fits the row in the toast stylesheet next to `Toast.tsx` and adjust only there

**Checkpoint**: Toast tests green (old and new).

---

## Phase 6: User Story 4 - Close banners inside the task screen and the Jira view (Priority: P2)

**Goal**: N6–N12 banners get the X; each hides until what it reports changes; base-stale keeps its persisted dismissal.

**Independent Test**: `npx vitest related --run` on each file below.

- [X] T011 [P] [US4] Add failing test in `app/src/components/TaskScreen.test.tsx`: a base-stale bar with a `notice` id has the X and no "Descartar" button; clicking X calls `api.dismissNotice(id)` then `onChanged`; "Actualizar rama" still shows when behind; a bar without a `notice` id hides locally. Update the existing test at line ~607 (`getByText("Descartar")`) to click the X by role
- [X] T012 [US4] In `app/src/components/TaskScreen.tsx` (`BaseStaleBars`, ~line 83), render each bar via `<Notice className="ts-bar base-stale">` with `useDismissals` (id `n.notice ?? n.text`, sig `n.text`); X → `api.dismissNotice(n.notice).then(onChanged, () => {})` when `n.notice != null`, else `dismiss(id)`; remove the "Descartar" button
- [X] T013 [P] [US4] Add failing tests in `app/src/components/SetupBanner.test.tsx`: running and failed banners have the X; clicking hides the banner without calling cancel/retry APIs; rerender running → failed shows it again
- [X] T014 [US4] In `app/src/components/SetupBanner.tsx`, `useDismissal(failed ? "failed" : running ? "running" : null)`, render through `<Notice>` keeping its classes/role, return `null` while hidden
- [X] T015 [P] [US4] Add failing tests in `app/src/components/HandoffActions.test.tsx`: the autopilot-paused banner has the X; clicking hides it with no resume/off call; a new reason or `since` shows it again
- [X] T016 [US4] In `app/src/components/HandoffActions.tsx` (`ha-banner`, ~line 52), `useDismissal(paused ? \`${reason}|${since}\` : null)` and render through `<Notice className="ha-banner">`; "Reanudar autopiloto" and "Seguir a mano" stay
- [X] T017 [P] [US4] Add failing tests in `app/src/components/TaskView.test.tsx`: each `tv-notice` (paused-by-limit, failed, waiting-for-user, review) has the X; clicking hides only that bar with no API call; a status change (or new `reason`/`resume_at`) shows it again
- [X] T018 [US4] In `app/src/components/TaskView.tsx` (~lines 190–245), one `useDismissals` with ids `limit`/`failed`/`waiting`/`review` and sigs from status type + `resume_at`/`reason`, or `phase` for review; render each bar through `<Notice className="tv-notice …">`; leave `SetupBanner` and `AskCard` as they are
- [X] T019 [P] [US4] Add failing tests in `app/src/components/SubtaskList.test.tsx`: the "N subtareas nuevas de Jira" banner has the X; clicking hides it; a different count shows it again; "Añadir" still works
- [X] T020 [US4] In `app/src/components/SubtaskList.tsx` (~line 79), `useDismissal(fresh > 0 ? String(fresh) : null)` and `<Notice className="subtasks-banner" role="status">`
- [X] T021 [P] [US4] Add failing tests in `app/src/components/PrView.test.tsx`: the problem notice, the subtask-commits warning, the wrong-base warning and the ready-to-leave-draft hint each have the X and hide on click with no API call; another problem kind, commit count, base or PR shows each again; their own buttons (Reintentar, Abrir una terminal, Marcar como lista) stay
- [X] T022 [US4] In `app/src/components/PrView.tsx` (~lines 241, 264, 436 and the ready hint), render the four notices through `<Notice>` keeping their classes and roles; per-PR notices call the hook inside the PR card component (never inside a `.map`) with sigs per plan table N11
- [X] T023 [P] [US4] Add failing tests in `app/src/components/JiraView.test.tsx`: the 401 banner has the X; clicking hides it while the list keeps its `stale` class; after a good load and a new 401 it shows again
- [X] T024 [US4] In `app/src/components/JiraView.tsx` (~line 336), `useDismissal(expired ? "expired" : null)` and `<Notice className="jv-banner" role="alert">`; `jv-body` keeps `stale` from `expired` alone

**Checkpoint**: all task-screen and Jira tests green.

---

## Phase 7: User Story 5 - Close the disconnected banner (Priority: P3)

**Goal**: the "Sin conexión" banner has the X; content stays inert; it returns on the next disconnection.

**Independent Test**: `npx vitest related --run app/src/components/DisconnectedBanner.tsx app/src/App.tsx`.

- [X] T025 [P] [US5] Add failing tests: in `app/src/components/DisconnectedBanner.test.tsx` the banner has the X and it calls `onClose`; in `app/src/App.test.tsx`, offline → X → banner gone while `.shell-content` is still `inert`/`aria-disabled`; online then offline again → banner back
- [X] T026 [US5] In `app/src/components/DisconnectedBanner.tsx` add an `onClose` prop and render through `<Notice className="dcb" role="alert">` ("Reintentar ahora" stays); in `app/src/App.tsx` (~line 402) hold `useDismissal(offline ? "offline" : null)`, render the `.shell-banner` wrapper only when `offline && !hidden`, and keep `is-offline`/`aria-disabled`/`inert` driven by `offline` alone

**Checkpoint**: every notification in the inventory has the X.

---

## Phase 8: Polish & cross-cutting

- [X] T027 Grep `app/src` for leftover close controls in notifications (`>×<`, text "Cerrar"/"Descartar" buttons in the N1–N12 components) and remove any the tasks above missed; Inbox cards stay as they are (out of scope)
- [X] T028 [P] Update `docs/usage.md`: every notification has an X; where the X is more than "hide" (toasts, update up to date/error/available/confirm, base-stale); closed bars come back when what they report changes; in-screen closes may reset when the screen reopens
- [X] T029 [P] Update `docs/status.md` (feature built) and add a decision to `docs/decisions.md`: client-side dismissal by signature, in memory; X maps to the existing close-like action where there is one; Inbox ask/permission cards and native notifications excluded; disconnected banner included with content still inert; global bars not reachable while offline; A → B → A within one React batch not observed
- [X] T030 Run `cd app && npx tsc --noEmit && npx vitest run`; all green
- [ ] T031 Try it in a preview following `specs/016-notification-close-button/quickstart.md` (`scripts/preview.sh --fresh`, never 7420), or hand the scenarios to `ui-verifier` with the preview URL

---

## Dependencies & Execution Order

- **Phase 2 (T001–T004)** blocks everything.
- **US1–US5** (Phases 3–7) depend only on Phase 2 and touch disjoint files: they can run in parallel.
- Inside each story: the test task first (RED), then its implementation task (GREEN).
- **Phase 8** after all stories; T028/T029 can run in parallel with T027.

### Parallel opportunities

- After T004: T005, T007, T009, T011, T013, T015, T017, T019, T021, T023, T025 (all test files differ).
- Within US4, each banner pair (T011→T012, T013→T014, …) is independent of the others.

### Parallel example: User Story 4

```text
T013 SetupBanner.test.tsx → T014 SetupBanner.tsx
T015 HandoffActions.test.tsx → T016 HandoffActions.tsx
T019 SubtaskList.test.tsx → T020 SubtaskList.tsx
T023 JiraView.test.tsx → T024 JiraView.tsx
```

## Implementation Strategy

1. **MVP**: Phase 2 + US1 (global bars): the bars that sit above every screen become closable.
2. Then US2 (update bar), which completes the global area.
3. Then US3 (toasts) and US4 (in-screen banners), in parallel.
4. Then US5 (disconnected banner), then Phase 8.

Each checkpoint leaves the app working; stop at any of them to try it in a preview.
