# Work Package 6: Setup, Handoff and Task View Banners

**Files**: `app/src/components/SetupBanner.test.tsx/.tsx`, `HandoffActions.test.tsx/.tsx`, `TaskView.test.tsx/.tsx`

**Task scope**: T013–T018 (3 banner pairs; each test first, then impl)

## Render Map

### SetupBanner (~93 L)
- **When**: `task.status.type === "setup-failed"` OR `running === true`
- **Element**: `<div className="notice err setup-panel">` with icon, title, buttons (Reintentar/Iniciar sin setup or Cancelar while running)
- **Props**: `task, running, onChanged, onOpenTerminal?, onOpenSettings?`
- **State**: `busy, error, info` (fetched from `api.setupInfo(task.key)`)
- **Pattern**: Returns `null` if neither failed nor running; otherwise renders conditional blocks inside the main div

### HandoffActions (~86 L)
- **When**: `autopilot?.state === "paused"`
- **Element**: `<div className="ha-banner">` with strong title, reason span, since span, Resume and Manual buttons (~line 52)
- **Props**: `task, handoff, autopilot?, onDone?, hideError?`
- **State**: `busy, error` (from `api.setAutopilot` calls)
- **Pattern**: Returns `<>` fragment; ha-banner is first conditional block; handoff stages follow

### TaskView (~500+ L, focus 190–245)
- **When**: 4 separate conditions; each is a `<div className="tv-notice tv-*">` bar
- **Bars**: 
  - `tv-pause` (status.type==="paused-by-limit"): icon, text, resume button
  - `tv-err` (status.type==="failed"): icon, reason (selectable), resume button
  - `tv-wait` (status.type==="waiting-for-user" && !current): icon, text, reply button
  - `tv-notice` (task.phase==="review" && isActive && hasWorkspace): icon, text, 2 buttons
- **Props**: `task, sessions, asks, projectSettings, onOpenSession, onNewSession, onChanged, onViewChanges`
- **State**: `busy, error, at, current` (from inbox pending, asks filtering); 2 existing buttons per bar
- **Pattern**: Each bar in its own if-block; no wrapping; SetupBanner and AskCard also here but not modified

## Signatures & Dismissal Strategy

### SetupBanner
- **Id**: None (renders conditionally)
- **Signature**: `failed ? "failed" : running ? "running" : null`
- **Dismissal**: `useDismissal(sig)` → return `null` while hidden
- **Reappear**: running→failed, failed→!failed, running→!running

### HandoffActions
- **Id**: `autopilot.since` (the pause start time, unique per pause event)
- **Signature**: `paused ? \`${autopilot.reason}|${autopilot.since}\` : null`
- **Dismissal**: `useDismissal(sig)` → return `null` while hidden
- **Reappear**: reason change, since change (pause event changed), paused→!paused

### TaskView
- **Id**: 4 separate: `limit`, `failed`, `waiting`, `review`
- **Signature per bar**:
  - `paused-by-limit`: `\`limit:${s.resume_at}\``
  - `failed`: `\`failed:${s.reason}\``
  - `waiting-for-user`: `"waiting"`
  - `review`: `\`review:${s.phase ?? ""}\`` (or variant with isActive/hasWorkspace)
- **Dismissal**: `useDismissals(sigs)` with all 4 at once; each calls `dismiss(id)` on close
- **Reappear**: status type change, resume_at/reason/phase change

## Test Patterns

**All use**: `vitest + @testing-library/react` (jsdom env); `beforeEach(setLanguage("es")); afterEach(cleanup)`

### SetupBanner.test.tsx (~131 L)
- Structure: 7 it-blocks in one describe
- **New tests for WP-06** (line T013):
  - Running banner has X (not Cancelar button, now .ntc-x)
  - Failed banner has X (not visible)
  - X hides banner; rerender running→failed shows it; rerender !failed hides it
  - X calls no API (verify retry/skip/cancel NOT called)
  - Check X is shared: `.ntc-x` class, name "Cerrar"
- **Existing tests**: remain unchanged (Reintentar/skip/cancel logic, error 409, reason translation)

### HandoffActions.test.tsx (~30 L)
- Structure: 1 describe with 2 it-blocks (simple tests)
- **New tests for WP-06** (line T015):
  - `autopilot?.state === "paused"` renders ha-banner with X
  - X hides banner, calls no resume/off API
  - rerender with new reason/since shows banner again
  - X is `.ntc-x`, name "Cerrar"
  - Buttons "Reanudar"/"Seguir a mano" stay
- **Existing tests**: "start without buttons", "offer uses kit buttons"

### TaskView.test.tsx (~500+ L, focus new tests around 190–245)
- Structure: Many describes; TaskView renders inside <> with <ToastHost />
- **New tests for WP-06** (line T017):
  - Each tv-notice bar has X (.ntc-x, name "Cerrar")
  - Paused-by-limit: X hides, rerender with new resume_at shows it
  - Failed: X hides, status type change shows it
  - Waiting-for-user: X hides (render with empty current), status change shows it
  - Review: X hides, phase change or isActive/hasWorkspace change shows it
  - No API calls on close (verify no api.resume, etc called by just the X)
  - Each bar's own buttons (Reanudar now, Reanudar, Contestar, etc) stay
- **Existing tests**: History, meta line, sessions, errors, etc (all pass through; not modified)

## Watch Out

1. **SetupBanner + TaskView**: SetupBanner renders separately at line ~201 in TaskView; T018 must not wrap it in a Notice (it stays separate)
2. **AskCard**: Also renders in TaskView (~214); not a notification, stays as-is
3. **tv-notice bars**: 4 separate `<div className="tv-notice tv-*">` elements (lines 190, 204, 225, 234); use **one** `useDismissals()` call with all 4 ids/sigs, not 4 separate hooks
4. **Props state dependency**: 
   - SetupBanner: depends on task.status type/reason change
   - HandoffActions: depends on autopilot.reason/since change
   - TaskView: depends on task.status type/reason/resume_at, task.phase, sessions change (to isActive), or the presence of `current` (AskCard) for waiting bar
5. **Button focus**: After X close, focus must move; existing "Reanudar"/"Contestar" buttons or next focusable stays reachable (Notice wrapper handles this)
6. **Persist vs memory**: None of these 3 components persist dismissals (unlike base-stale bars); only in-memory, lost on re-render/reload

## Related Symbols
- `useDismissal` / `useDismissals` from WP-01 (`app/src/components/ui/Notice.tsx`)
- `<Notice>` wrapper from WP-01 (className, role, onClose, children)
- `<CloseButton>` from WP-01 (auto-wired, no need to call it; Notice renders it)
- `api.dismissNotice` (NOT used here; only in TaskScreen for base-stale)
- Existing dismissal in `GlobalNotices.tsx` (WP-02) as pattern reference
