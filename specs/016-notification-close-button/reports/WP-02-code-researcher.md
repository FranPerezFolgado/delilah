# Work Package Map: WP-02 (T005–T006)

**Task**: Add close button to global bars (usage, autopilot-pause, overflow).  
**Files**: `app/src/components/GlobalNotices.tsx`, `.test.tsx`, `.css`

## Bar Rendering Location

| Bar | Lines | Trigger | Rendered as |
|---|---|---|---|
| Usage bar | 84–99 | `hot.map()` → `used_percentage >= 90%` | `<div className="gn-bar gn-wait" role="status">` |
| Paused autopilot bar | 101–109 | `paused.slice(0, MAX_PAUSED).map()` | `<div className="gn-bar gn-pause" role="status">` with "Abrir" button |
| Overflow bar ("y N más") | 111–113 | `paused.length > MAX_PAUSED` | `<div className="gn-bar gn-pause" role="status">` text-only |

## `current` Signature IDs for `useDismissals()`

```ts
// Build current from hot, paused, overflow:
const current: Record<string, string> = {};
hot.forEach(([name]) => {
  const full = (w.used_percentage ?? 0) >= 100;
  current[`usage:${name}`] = full ? "full" : "hot"; // changes when % crosses 90/100
});
paused.slice(0, MAX_PAUSED).forEach(k => {
  current[`pause:${k.key}`] = `${k.autopilot?.reason ?? ""}|${k.autopilot?.since ?? ""}`;
});
if (paused.length > MAX_PAUSED) {
  current["more"] = String(paused.length - MAX_PAUSED); // changes when count changes
}
```

## Test Patterns (GlobalNotices.test.tsx)

**Setup**:
- Factory fns: `task(key, status, autopilot?)` → Task, `usage(five, resets_at?)` → UsageReport
- Render harness: `setup()` returns `{ onOpen, onInbox, to, ...view }` where `to(tasks, current)` rerenders
- Utilities: `inToasts()` (sugar for `.getByRole("status")`), `toastsText()` for content checks

**Patterns**:
- Threshold tests: render at 89%, 93%, 100% with assertions per value → reuse for dismissal re-show
- State transitions: `to([...])` changes props → check visibility before/after (e.g., lines 69–96)
- Toast close by label: `fireEvent.click(screen.getByLabelText("Cerrar"))` (line 107, existing pattern)

**For T005 (new tests)**:
- Test "X hides usage bar": render two 95% windows → X on first → verify hidden; rerender 100% → verify shown
- Test "X hides paused bar": paused task → X → verify no `onOpenTask` call; new reason → shown
- Test "X hides overflow": 4+ paused → overflow bar → X → verify hidden; overflow count changes → shown
- Test "X shares class": verify all use `.ntc-x` class name from `<CloseButton>`

**For T006 (integration)**:
- Keep toast logic (lines 44–74) untouched
- Call `const { hidden, dismiss } = useDismissals(current)` after building current
- Render bars via `<Notice className="gn-bar gn-wait …" role="status" onClose={() => dismiss(id)}>`
- Wrap each bar's children and skip if `hidden(id)`

## Watch Out

- Toast key `WAITING_TOAST_KEY` and logic in lines 44–74 must stay exact (not part of Notice wrapping)
- `open.current` ref pattern keeps callbacks stable across renders
- `MAX_PAUSED = 3` is a constant; overflow bar is separate (don't combine in one entry)
- Signature for usage includes `full`/`hot` state, not just window name
- Tests use `vi.fn()` spies; verify dismissal does *not* call `onOpenTask` (lines 44–51 existing pattern)
