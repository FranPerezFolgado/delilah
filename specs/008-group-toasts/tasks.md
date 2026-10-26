# Tasks: Group toasts

**Input**: Design documents from `specs/008-group-toasts/`

**Prerequisites**: plan.md, spec.md, research.md, data-model.md, contracts/ui.md, quickstart.md

**Tests**: included — the project implements test-first (orchestrated packages); every story starts
with failing Vitest tests (jsdom, fake timers).

**Checks**: `cd app && npx tsc --noEmit && npx vitest related --run <files>`; at the end `npx vitest run`.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: can run in parallel (different files, no dependency on unfinished tasks)
- **[Story]**: US1–US4 from spec.md

---

## Phase 1: Setup

- [X] T001 Create `app/src/i18n/en/Toast.ts` exporting `EN` (empty record for now, same shape as `app/src/i18n/en/GlobalNotices.ts`); it is picked up by the `./en/*.ts` glob in `app/src/i18n/en.ts`

---

## Phase 2: Foundational (toast store — blocks every story)

**Purpose**: the store features every story builds on: keyed toasts, user-dismiss callback, store-level timers, eviction rule. No visible change yet.

- [X] T002 Write failing tests in `app/src/components/ui/Toast.test.tsx`: (a) `pushToast({key, …})` twice keeps one toast, updates its text/action in place and returns the same id; (b) `dismissToastByKey(key)` removes it without calling `onDismiss`; (c) the × button calls `onDismiss` once, auto-expiry and `dismissToast(id)` don't; (d) info toasts still expire after 6 s / `ttl` with timers owned by the store (they expire even if `ToastHost` is not mounted); errors, `wait` and keyed toasts never expire; (e) eviction over 3 drops the oldest info toast without action or key first, else the oldest non-keyed toast, and never a keyed toast. Replace per-toast `getAllByRole("status")` counts with text queries (the host gets a single announcer in US3)
- [X] T003 In `app/src/components/ui/Toast.tsx` extend `ToastInput` with `key?: string` and `onDismiss?: () => void`, and `ToastItem` with `count`, `aliases`, `deadline?`, `remaining?` (data-model.md); implement keyed in-place update in `pushToast`, `dismissToastByKey(key)`, export `WAITING_TOAST_KEY = "waiting"`, and internal `closeToast(id)` (calls `onDismiss`, then removes) used by the × button
- [X] T004 In `app/src/components/ui/Toast.tsx` move auto-dismiss timers from `ToastView` into the store: one timeout for the earliest `deadline`, removing expired items and rescheduling; add `setToastsPaused(paused: boolean)` converting `deadline` ↔ `remaining`; `clearToasts()` also clears the timeout; `ToastView` becomes presentational
- [X] T005 In `app/src/components/ui/Toast.tsx` implement the eviction rule (research R6, keyed never evicted) and make `dismissToast(id)` resolve ids in `aliases`
- [X] T006 Export `dismissToastByKey` and `WAITING_TOAST_KEY` from `app/src/components/ui/index.ts` (keep every existing export)

**Checkpoint**: `npx vitest related --run src/components/ui/Toast.tsx` green; all existing call sites compile unchanged (`npx tsc --noEmit`).

---

## Phase 3: User Story 1 — One «te espera» toast that follows the tasks (P1) 🎯 MVP

**Goal**: at most one waiting toast, «{key} te espera · Responder» or «{n} tareas te esperan · Ver», updated in place and gone when its tasks stop waiting.

**Independent Test**: three tasks start waiting while another is open → one toast «3 tareas te esperan»; two stop waiting → «X te espera · Responder»; the last stops → no toast.

- [X] T007 [US1] Write failing tests in `app/src/components/GlobalNotices.test.tsx` (rework «toasts «te espera» when a task starts waiting…»): first load only seeds; one new waiting task → «A-0 te espera» with «Responder» calling `onOpenTask("A-0")`; a second → one toast «2 tareas te esperan» with «Ver» calling `onOpenInbox`; a task that stops waiting, leaves the list or becomes `current` (rerender with `current`) is removed and the text updates; empty group → no toast; closing it (×) empties the group and a later new waiting task brings it back with only that task; `current` task is never added; unmount removes the toast. Pass the new `onOpenInbox` prop in the test's `ui()` helper
- [X] T008 [US1] In `app/src/components/GlobalNotices.tsx` replace the per-task `pushToast` with a waiting group (`useRef<string[]>`): keep seeding in the `tasks` effect, add an effect over `[tasks, current]` that adds newly waiting keys (not `current`), drops keys no longer waiting / not in `tasks` / equal to `current`, then renders: empty → `dismissToastByKey(WAITING_TOAST_KEY)`; 1 → `pushToast({key: WAITING_TOAST_KEY, kind: "wait", text: t("{key} te espera", {key}), action: {label: t("Responder"), run}, onDismiss})`; N → `text: tn(n, "{n} tarea te espera", "{n} tareas te esperan")`, `action: {label: t("Ver"), run: onOpenInbox}`; `onDismiss` clears the group; dismiss on unmount; add required prop `onOpenInbox: () => void`
- [X] T009 [US1] In `app/src/App.tsx` (GlobalNotices at ~line 390) pass `onOpenInbox={() => setView("inbox")}`
- [X] T010 [P] [US1] Add English entries «{n} tarea te espera» → «{n} task is waiting for you», «{n} tareas te esperan» → «{n} tasks are waiting for you», «Ver» → «View» in `app/src/i18n/en/GlobalNotices.ts` (skip any key that already exists in another `en/*.ts` file)

**Checkpoint**: US1 tests green; `src/i18n/coverage.test.ts` green.

---

## Phase 4: User Story 2 — Repeated messages merge (P2)

**Goal**: same kind + same text, no action, no key → one toast with «×N».

**Independent Test**: push the same error three times → one toast with «×3».

- [X] T011 [US2] Write failing tests in `app/src/components/ui/Toast.test.tsx`: same text+kind ×3 → one toast showing «×3», moved to newest position; merged info toast restarts its full ttl; different kind → not merged; toasts with an action (two «Deshacer») → not merged; `dismissToast` with the id of any repeat removes the merged toast; count above 99 shows «×99+»
- [X] T012 [US2] In `app/src/components/ui/Toast.tsx` implement the merge branch of `pushToast` (count++, reset deadline to full ttl, append new id to `aliases`, move to end, return new id) and render the count badge `<span className="toast-count">{t("×{n}", {n})}</span>` (n = count or «99+») in `ToastView`
- [X] T013 [P] [US2] Add `.toast-count` style in `app/src/components/ui/Toast.css` (muted, tabular numbers, same tokens as today) and the «×{n}» entry in `app/src/i18n/en/Toast.ts`

**Checkpoint**: US2 tests green.

---

## Phase 5: User Story 3 — A collapsed stack (P2)

**Goal**: newest toast in full, older ones as «+N», expand on hover/focus/click, «Cerrar todo».

**Independent Test**: three different persistent toasts → one visible with «+2»; hover/focus/click expands to three; leaving collapses; «Cerrar todo» removes all.

- [ ] T014 [US3] Write failing tests in `app/src/components/ui/Toast.test.tsx`: 1 toast → no «+N», no «Cerrar todo»; 3 toasts collapsed → only the newest text rendered, a button «+2» with `aria-expanded="false"` and accessible name «Mostrar 2 avisos más»; `mouseEnter` on the host, focusing a button inside it, or clicking «+2» → all three rendered; `mouseLeave` + focus out → collapsed again, also after a click-pin (pinned resets); «Cerrar todo» removes all and calls each `onDismiss`; a hidden announcer with `role="status"` contains the text of the last push/merge/keyed update and the visible stack has no live role
- [ ] T015 [US3] In `app/src/components/ui/Toast.tsx` rework `ToastHost`: `hovered`, `focused` (focus-within via `onFocus`/`onBlur` with `relatedTarget` check) and `pinned` state; `expanded = hovered || focused || pinned`; reset `pinned` when neither hovered nor focused; collapsed with >1 → newest `ToastView` + `<button className="toast-more" aria-expanded aria-label={t("Mostrar {n} avisos más", {n})}>{t("+{n}", {n})}</button>`; expanded → all toasts oldest first; `<button className="toast-clear">{t("Cerrar todo")}</button>` when >1 calling an internal `closeAll()` (calls `onDismiss` of each, then empties); visually hidden announcer `<div role="status" aria-live="polite" className="sr-only">` fed by a store field `lastAnnounced` updated on push, merge («text ×N») and keyed update; remove `role="status"` from `ToastView`
- [ ] T016 [P] [US3] Add `.toast-more`, `.toast-clear` and stacked-card styles in `app/src/components/ui/Toast.css` (reuse an existing visually-hidden class if the app has one, else add `.toast-sr`), keeping `.toast-host` position and the shared notice tokens (007 FR-059)
- [ ] T017 [P] [US3] Add English entries «Cerrar todo» → «Close all», «+{n}» → «+{n}», «Mostrar {n} avisos más» → «Show {n} more notices» in `app/src/i18n/en/Toast.ts` (skip keys already present elsewhere)

**Checkpoint**: US3 tests green; existing Toast tests still green.

---

## Phase 6: User Story 4 — Timers that respect the user (P3)

**Goal**: info timers pause while the stack is hovered/focused/expanded and resume with remaining time; undo toasts tied to a pending action never outlive it.

**Independent Test**: info toast, hover 10 s, leave → still there, gone after its remaining time.

- [ ] T018 [US4] Write failing tests in `app/src/components/ui/Toast.test.tsx`: info toast with 2 s left, `mouseEnter` host, advance 10 s → still there; `mouseLeave` → gone after ~2 s, not before; hidden (collapsed) older info toasts keep expiring while not hovered
- [ ] T019 [US4] In `app/src/components/ui/Toast.tsx` call `setToastsPaused(expanded)` from `ToastHost` in an effect (and `setToastsPaused(false)` on unmount)
- [X] T020 [P] [US4] Write a failing test in `app/src/components/ProposalCard.test.tsx`: after «Descartar», hovering the toast stack past `UNDO_MS` still sends «Descartar» and the «Tarea descartada» toast is gone
- [X] T021 [US4] In `app/src/components/ProposalCard.tsx` keep the id returned by `pushToast` for «Tarea descartada» and call `dismissToast(id)` in the discard timer callback

**Checkpoint**: US4 tests green.

---

## Phase 7: Polish & cross-cutting

- [ ] T022 Run `cd app && npx tsc --noEmit && npx vitest run`; fix any test that relied on one `role="status"` per toast or on per-task «te espera» toasts (e.g. `App.test.tsx`, `TaskView.test.tsx`, `InboxView.test.tsx`, `BacklogTaskView.test.tsx`, `TaskScreen.test.tsx`), without changing what they assert about behaviour
- [ ] T023 [P] Update `README.md` (toasts / «te espera» section near «Limpiar avisos»): one grouped waiting toast, merged repeats «×N», collapsed stack with «+N» and «Cerrar todo», pause on hover
- [ ] T024 [P] Update `docs/status.md` (Kit/Toast entry and built features) and `docs/architecture.md` (UI kit note: toast store with keyed/merged toasts, GlobalNotices owns the waiting group)
- [ ] T025 [P] Add a decision to `docs/decisions.md`: one grouped «te espera» toast owned by GlobalNotices instead of one per task; merge rule (exact text+kind, no action); collapsed stack; alternatives ruled out (lower MAX, notification centre, moving waiting to gn-bar)
- [ ] T026 Try it in a preview per `specs/008-group-toasts/quickstart.md` (`scripts/preview.sh --fresh`, never the installed daemon on 7420); check scenarios 1–5 (ui-verifier)
- [ ] T027 Bump the version (minor: new user-facing behaviour, backwards compatible) with `scripts/bump-version.sh minor --commit` as its own commit `chore: bump version to X.Y.Z`, from the version on `main`

---

## Dependencies & Execution Order

- Phase 1 → Phase 2 → stories. Phase 2 blocks all stories (store API).
- US1 (Phase 3) only needs Phase 2: touches `GlobalNotices.tsx`, its test, `App.tsx`, `i18n/en/GlobalNotices.ts` — it can run **in parallel** with US2/US3, which touch `Toast.tsx`/`Toast.css`/`Toast.test.tsx`/`i18n/en/Toast.ts`.
- US2 → US3 → US4 are sequential: all three edit `Toast.tsx` and `Toast.test.tsx`. US4's ProposalCard tasks (T020–T021) can run in parallel with US2/US3.
- Polish after all stories; T023–T025 in parallel; T027 last.

### Parallel opportunities

```text
After Phase 2:
  Worker A: T007 → T008 → T009 (+ T010)            # US1, GlobalNotices/App
  Worker B: T011 → T012 (+T013) → T014 → T015 (+T016, T017) → T018 → T019   # Toast.tsx
  Worker C: T020 → T021                              # ProposalCard
Polish: T023 ‖ T024 ‖ T025
```

## Implementation Strategy

- **MVP**: Phase 1 + Phase 2 + US1 — removes the main source of accumulation (one toast per waiting task, never cleaned up).
- Then US2 (merge) and US3 (collapse) for the remaining noise, US4 as refinement, then polish, docs and version bump.
