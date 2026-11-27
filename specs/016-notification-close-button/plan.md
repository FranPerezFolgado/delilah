# Implementation Plan: Close button on every notification

**Branch**: `016-notification-close-button` | **Date**: 2026-11-26 | **Spec**: [spec.md](spec.md)
**Input**: Feature specification from `specs/016-notification-close-button/spec.md`

## Summary

Every notification in the web UI (inventory N1–N12 in the spec) gets the same X: a new
`CloseButton` (the kit's `IconButton` with `icon="close"` and label `t("Cerrar")`). Bars
and banners render through a small shared `Notice` wrapper that appends that X after the
caller's own content and actions. Closing is client-side: a `useDismissals` hook keeps,
per notice id, the *signature* of what it reported when it was closed; the notice stays
hidden only while its current signature is the same, and an entry is forgotten as soon as
the signature changes or the notice goes away, so it shows again by itself (FR-006).
Where a close-like action already exists, the X calls it (toast dismiss, update
`close`/`later`/`cancel`, `api.dismissNotice`). Web UI only: no daemon, HTTP API,
migration or Tauri change.

## Technical Context

**Language/Version**: TypeScript + React 18 (`app/`, Vite)
**Primary Dependencies**: the app's UI kit (`app/src/components/ui/`: `IconButton`, `Icon`), `i18n` (`t`)
**Storage**: none (in-memory React state); base-stale dismissal keeps using the existing `api.dismissNotice`
**Testing**: vitest + Testing Library (`cd app && npx tsc --noEmit && npx vitest related --run <files>`; all: `npx vitest run`)
**Target Platform**: Tauri desktop app (WebKit) and the browser preview
**Project Type**: web UI of the desktop app
**Performance Goals**: no extra requests; the hook prunes on effect, O(number of notices)
**Constraints**: one close style everywhere; keep each notification's other actions; no new user-facing strings beyond `Cerrar` (already translated); no version bump; try in `scripts/preview.sh`
**Scale/Scope**: 12 notification kinds across ~12 components

## Constitution Check

`.specify/memory/constitution.md` is the unfilled template: no gates. The project's
ground rules (CLAUDE.md) apply and are met: no contract change (`api.ts` ↔ `api.rs`
untouched), no migration, test-first per notification, previews for trying it, docs
updated (`status.md`, `usage.md`, `decisions.md`). Post-design re-check: still no
violations.

## Starting point (from the code)

| # | Notification | Where | Today | X does | Signature (shows again when it changes) |
|---|---|---|---|---|---|
| N1 | Toasts | `components/ui/Toast.tsx:193` (`ToastView`) | text `×` button, `closeToast(id)` | `closeToast(id)` (unchanged) | — |
| N2 | Usage bar per window | `GlobalNotices.tsx:84-99` | none | hide | id `usage:<window>`, sig `full`/`hot` |
| N3 | Autopilot paused per task; "y N más" | `GlobalNotices.tsx:100-113` | none | hide | id `pause:<key>`, sig `<reason>\|<since>`; id `more`, sig `<n>` |
| N4 | Update bar | `UpdateNotice.tsx` | text `Cerrar` in `upToDate`, `error` | `upToDate`/`error`: `dispatch({type:"close"})`; `available`: `later`; `confirmSessions`: `cancel`; `checking`/`downloading`/`installing`/`waiting`: hide | sig `<kind>:<version>` (only for the hide-only kinds) |
| N5 | Disconnected banner | `DisconnectedBanner.tsx`, mounted in `App.tsx:402-406` | none | hide | sig `offline` while offline; forgotten on reconnect |
| N6 | Base-stale bars | `TaskScreen.tsx:83-152` (`BaseStaleBars`) | text `Descartar` → `api.dismissNotice` | `api.dismissNotice(n.notice)` when `n.notice != null`, else hide | id `<n.notice ?? n.text>`, sig `n.text` |
| N7 | Setup banner | `SetupBanner.tsx:43` | none | hide | sig `failed`/`running` |
| N8 | Autopilot paused (task screen) | `HandoffActions.tsx:51-63` | none | hide | sig `<reason>\|<since>` |
| N9 | Task status bars | `TaskView.tsx:190-245` (`tv-notice`: limit, failed, waiting, review) | none | hide | id per bar, sig the status type plus `resume_at`/`reason`, or `phase` for review |
| N10 | New Jira subtasks | `SubtaskList.tsx:79-84` | none | hide | sig `<fresh count>` |
| N11 | PR notices | `PrView.tsx:241` (problem), `:264` (subtask commits), `:436` (wrong base), ready hint | none | hide | problem: `<kind>`; commits: `<n>`; wrong base: `<base>→<expected>`; ready hint: `<pr id>` (id per PR where the notice is per PR) |
| N12 | Jira token rejected | `JiraView.tsx:336-342` | none | hide | sig `expired` while `expired`; forgotten when it clears |

Existing tests that find the old text buttons and must move to the X by role:
`UpdateNotice.test.tsx:52` (`getByText("Cerrar")`), `TaskScreen.test.tsx:607`
(`getByText("Descartar")`). The toast test (wherever `closeToast` is covered) keeps
working because the label stays `Cerrar`.

## Design

### 1. Kit: `CloseButton`, `Notice`, `useDismissals` (new `components/ui/Notice.tsx`, `Notice.css`)

Contract in [contracts/ui-notice.md](contracts/ui-notice.md); state in
[data-model.md](data-model.md).

- `CloseButton({ onClick, className? })`: `<IconButton icon="close" label={t("Cerrar")} className="ntc-x …" />`.
  `ntc-x`: 28 × 28 (bars use 28 px buttons), `flex-shrink: 0`, `margin-left: auto` so it
  sits last even without a `grow` spacer. The only close control used by notifications.
- `Notice({ className, role, onClose, children })`: renders
  `<div className={"ntc " + className} role={role}>{children}<CloseButton …/></div>`.
  Callers keep their own class (`gn-bar gn-wait`, `ts-bar base-stale`, `tv-notice tv-err`,
  `dcb`, `jv-banner`, `ha-banner`, `subtasks-banner`, `pr-base-warning`, `notice pr-problem`,
  `pr-ready-hint`), so their look does not change; `.ntc` only sets `display: flex;
  align-items: center` for the few that are not flex yet (`pr-base-warning`), and the
  caller's class wins where it already sets layout.
- Focus (spec edge case): if the X had focus when clicked, `Notice` moves focus, after
  `onClose`, to the next focusable element after the notice in document order
  (computed before closing); if there is none, nothing is focused. One private helper
  inside `Notice.tsx` (not a kit-wide focus util), ~10 lines, covered by a test.
- `useDismissals(current: Record<string, string>)` → `{ hidden(id): boolean; dismiss(id): void }`:
  state `Map<id, sig>`; `hidden(id)` is `map.get(id) === current[id]`; `dismiss(id)`
  stores `current[id]`; an effect keyed on the serialised `current` deletes every entry
  whose id is absent or whose sig differs, so A → B → A shows again (spec edge case)
  when B is its own render, which is how polled data arrives; a B that only lives inside
  one React batch is never observed (accepted, documented in decisions.md).
- `useDismissal(sig: string | null)` → `[hidden, dismiss]`: the one-notice form
  (`useDismissals(sig == null ? {} : { "": sig })`); `null` means "not showing" and
  forgets the dismissal.
- Export all four (and their prop types) from `components/ui/index.ts`; import
  `IconButton` the way other kit files do.

### 2. Toasts (`ui/Toast.tsx`)

Replace the text `×` button in `ToastView` with `<CloseButton onClick={() => closeToast(item.id)} />`.
Nothing else changes: `closeAll`/"Cerrar todo", `onDismiss`, timers and pausing stay.
Toast CSS: make sure the 28 px X fits the toast row (check `Toast.css`).

### 3. Global bars (`GlobalNotices.tsx`, `UpdateNotice.tsx`)

- `GlobalNotices`: build `current` from `hot`, `paused.slice(0, MAX_PAUSED)` and the
  overflow count (ids/sigs from the table); render each bar as `<Notice>` with
  `onClose={() => dismiss(id)}`, skipping `hidden(id)`. The toasts part is untouched.
- `UpdateNotice`: every state renders `<Notice>`. `upToDate`/`error` X dispatches
  `close` and their text `Cerrar` buttons go; `available` X dispatches `later` ("Más
  tarde" stays); `confirmSessions` X dispatches `cancel` ("Cancelar" stays);
  `checking`/`downloading`/`installing`/`waiting` use `useDismissal(<kind>:<version>)`
  (sig `null` in the other kinds) and return `null` while hidden. Progress updates do not
  change the sig. The reducer (`updates.ts`) is unchanged.

### 4. Disconnected banner (`DisconnectedBanner.tsx`, `App.tsx`)

`DisconnectedBanner` gains an `onClose` prop and renders through `Notice` (keeps `dcb`,
`role="alert"`). `App` holds `useDismissal(offline ? "offline" : null)`; it renders the
`.shell-banner` wrapper only when `offline && !hidden`. The content keeps `is-offline`,
`aria-disabled` and `inert` from `offline` alone (FR-009); reconnecting sets the sig to
`null`, which forgets the dismissal, so the next loss shows it again. The global and
update bars sit inside the inert content, so their X is not reachable while offline;
that is accepted (nothing in them can be acted on offline either) and noted in
decisions.md.

### 5. Task screen and Jira banners

Each component keeps its own `useDismissal`/`useDismissals` (in-memory, scoped to the
mounted screen, which matches the spec's assumption that in-screen closes may reset when
the screen is reopened):

- `BaseStaleBars` (`TaskScreen.tsx`): `<Notice>` per notice; X → `api.dismissNotice(n.notice).then(onChanged, () => {})`
  when `n.notice != null` (the text `Descartar` button goes), else local hide.
  "Actualizar rama" stays.
- `SetupBanner`: `useDismissal(failed ? "failed" : running ? "running" : null)`; while
  hidden return `null` (no cancel, no retry).
- `HandoffActions`: the `ha-banner` via `Notice` with `useDismissal(paused ? reason|since : null)`.
- `TaskView`: each `tv-notice` (limit, failed, waiting, review) via `Notice` with one
  `useDismissals` (ids `limit`/`failed`/`waiting`/`review`). `SetupBanner` and `AskCard`
  are not touched here (AskCard is out of scope).
- `SubtaskList`: `useDismissal(fresh > 0 ? String(fresh) : null)`.
- `PrView`: problem notice, subtask-commits warning, wrong-base warning and ready hint
  via `Notice`; the per-PR ones key their id by PR id in the component that renders the
  PR card (the hook is called inside that card component, never inside a `.map`).
- `JiraView`: `useDismissal(expired ? "expired" : null)`; `jv-body` keeps its `stale`
  class from `expired` alone.

### 6. Docs

- `docs/usage.md`: one paragraph under the notifications / UI section: every
  notification has an X; what each X does where it is not just "hide"; closed bars come
  back when what they report changes.
- `docs/status.md`: the feature under built.
- `docs/decisions.md`: new decision: client-side dismissal by signature, in memory; X
  maps to the existing close-like action where there is one; Inbox ask/permission cards
  and native notifications excluded; disconnected banner included but content stays
  inert.

## Testing (test-first)

RED first, one test per notification that gains the X, plus one for toasts:

| File | Test |
|---|---|
| `ui/Notice.test.tsx` (new) | `Notice` renders children then a button named "Cerrar" and calls `onClose`; focus moves to the next focusable after closing; `useDismissals` hides by id, shows again when the sig changes, forgets on absence (A → B → A, absent → back) |
| toast test (existing file, or `ui/Toast.test.tsx`) | the toast's close is the shared X (`.ntc-x`, name "Cerrar"), runs `onDismiss`; "Cerrar todo" still there |
| `GlobalNotices.test.tsx` | usage bar X hides only that window; shows again at 100 %; shows again after leaving and re-entering the threshold; paused bar X hides only that task, task not acted on; "y N más" X and back on N change |
| `UpdateNotice.test.tsx` | X per state: `close` (upToDate, error; no text "Cerrar" button left), `later` (available, "Más tarde" still there), `cancel` (confirmSessions, "Cancelar" still there), no dispatch and hidden (checking, downloading, installing, waiting); back on kind change. Update line 52 to the X by role |
| `DisconnectedBanner.test.tsx` | X calls `onClose` |
| `App.test.tsx` | offline → close → banner gone, content still `inert`; online → offline → banner back |
| `TaskScreen.test.tsx` | base-stale X calls `dismissNotice`, no "Descartar" button; update line 607 |
| `SetupBanner.test.tsx` | X hides, no cancel/retry call; running → failed shows again |
| `HandoffActions.test.tsx` | X hides the paused banner, no resume/off call; new reason shows again |
| `TaskView.test.tsx` | X hides each `tv-notice`; status change shows again |
| `SubtaskList` test (existing or new) | X hides; count change shows again |
| `PrView.test.tsx` | X hides problem notice / commits warning / wrong base / ready hint; change shows again |
| `JiraView.test.tsx` | X hides the 401 banner, list stays `stale`; back after a good load and a new 401 |

Checks: `cd app && npx tsc --noEmit && npx vitest run`. Manual: `scripts/preview.sh`,
trigger each bar where the fixture allows it (usage and update states are covered by
tests; see [quickstart.md](quickstart.md)).

## Project Structure

### Documentation (this feature)

```text
specs/016-notification-close-button/
├── spec.md
├── plan.md
├── research.md
├── data-model.md
├── quickstart.md
├── contracts/ui-notice.md
├── checklists/requirements.md
└── tasks.md            # /speckit-tasks
```

### Source Code (repository root)

```text
app/src/
├── App.tsx                         # disconnected banner dismissal
├── components/
│   ├── ui/Notice.tsx, Notice.css   # new: CloseButton, Notice, useDismissals, useDismissal
│   ├── ui/index.ts                 # exports
│   ├── ui/Toast.tsx (+ Toast.css)
│   ├── GlobalNotices.tsx, UpdateNotice.tsx, DisconnectedBanner.tsx
│   ├── TaskScreen.tsx (BaseStaleBars), TaskView.tsx, SetupBanner.tsx, HandoffActions.tsx
│   ├── SubtaskList.tsx, PrView.tsx, JiraView.tsx
│   └── *.test.tsx for each
docs/usage.md, docs/status.md, docs/decisions.md
```

**Structure Decision**: web UI only; one new kit module, then each notification adopts it.
The kit step (`Notice.tsx`, `Notice.css`, `index.ts`) lands first and blocks the rest; after it, the global bars (§3–4), task-screen banners (§5)
and toasts (§2) touch disjoint files and can run in parallel.

## Plan review

`architecture-reviewer`: no CRITICAL/HIGH. Folded in: the A → B → A within one batch
limitation (§1, decisions.md), the inert global bars while offline (§4), the focus helper
kept private to `Notice.tsx` (§1), per-PR hooks inside the card component (§5), the kit
lands before the parallel steps. Noted: Inbox "Descartar" buttons are inbox cards and stay
out of scope (spec).

## Complexity Tracking

None: one wrapper and one hook replace twelve hand-made X's.
