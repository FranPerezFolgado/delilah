# WP-07 Code Researcher Map: PR view notices and Jira expired banner

**Tasks**: T021–T024 | **Files**: PrView.test.tsx, PrView.tsx, JiraView.test.tsx, JiraView.tsx

---

## Notices: Location, Component Boundary & Hooks

| Notice | File | Lines | Scope | Hook Call | Dismissal ID | Signature |
|--------|------|-------|-------|-----------|---|---|
| **N11a: Problem** | PrView.tsx | 241–262 | Top-level PrView, not in loop | `useDismissal("problem")` | `"problem"` | `problem.kind` (string, e.g. `"signed-out"`) |
| **N11b: Commits** | PrView.tsx | 264–268 | Top-level PrView, not in loop | `useDismissal("commits")` | `"commits"` | `base.subtask_commits` (number) |
| **N11c: Wrong base** | PrView.tsx | 436–440 | Inside `PrCard` component | `useDismissal(id)` per PR | `pr.id:base` | `${pr.base_branch}→${pr.expected_base}` |
| **N11d: Ready hint** | PrView.tsx | 441–448 | Inside `PrCard` component | `useDismissal(id)` per PR | `pr.id:ready` | `${pr.id}:ready:${s.draft}:${s.state}` |
| **N12: Jira expired** | JiraView.tsx | 336–342 | Top-level JiraView, not in loop | `useDismissal(expired)` | (implicit from sig) | `"expired"` or `null` |

---

## Critical Boundary Rule: Hooks in PrCard, Never in .map

- **N11c & N11d** are inside `prs.map((pr) => <PrCard …>)` at **PrView.tsx:301**.
- Hooks **must** be called inside `PrCard` component function, **not** inside the `.map` callback.
- Each PR gets its own dismissal state keyed by `pr.id`.
- Use `useDismissal` per notice (one-notice form).

---

## Available Signatures & State Transitions

### PrView (top-level notices: problem, commits)

```ts
// N11a: problem.kind changes → notice reappears
problem.kind === "signed-out" | "missing" | "no-access" | "unsupported-remote" | "network" | "rate-limited" | null

// N11b: base.subtask_commits changes → notice reappears
base?.subtask_commits: number | undefined
```

### PrCard (per-PR notices: wrong base, ready hint)

```ts
// N11c: misbased when pr.base_branch !== pr.expected_base
const misbased = !!pr.base_branch && !!pr.expected_base && pr.base_branch !== pr.expected_base;
// sig = `${pr.base_branch}→${pr.expected_base}` (or null when not misbased)

// N11d: readyHint when readyHint prop && s?.draft && s.state === "open"
// sig = `${pr.id}:draft` (or null when not ready)
```

### JiraView (expired)

```ts
// N12: expired boolean drives sig
expired: boolean  // from API
// sig = "expired" when expired, null otherwise
```

---

## Test Patterns (from plan §7, line 173, and existing tests)

### PrView.test.tsx (T021)

**Setup**: Mock `api.taskPrs()` with PrView-level data (problem, base).  
**Test pattern**:
1. Render with notice visible.
2. Assert notice shows (by role or text) **and** has the shared X (`.ntc-x` class, name "Cerrar").
3. Existing action buttons stay (e.g., "Reintentar").
4. Click the X → notice hides.
5. Rerender with changed signature (e.g., `problem.kind` changes) → notice reappears.

**Key assertion**: `getByRole("button", { name: /Cerrar/i })` (not by class alone; vitest role queries are preferred).

### PrCard (inside PrView, T021 extended)

**Setup**: Render `PrView` with a task list containing one PR with `misbased === true` and another with `readyHint === true`.  
**Test pattern**:
1. Assert both notices show (one per PR) with the X.
2. Click X on wrong-base notice → only that one hides; ready hint stays.
3. Rerender same PR data → both still hidden.
4. Rerender with wrong base PR corrected (`pr.base_branch === pr.expected_base`) → notice reappears; ready hint still hidden until its state changes.

### JiraView.test.tsx (T023)

**Setup**: Mock `api.jira()` with `expired: true`.  
**Test pattern**:
1. Render with banner showing.
2. Assert `.jv-banner[role="alert"]` exists with the X.
3. Assert `.jv-body` has `stale` class (driven by `expired` alone, not dismissal).
4. Click X → banner hides; `.jv-body` **still has** `stale` class.
5. Rerender with successful load (`expired: false`) → banner stays hidden (dismissal forgotten).
6. Rerender with `expired: true` again → banner reappears.

---

## Implementation Hints

### PrView.tsx (T022)

1. **N11a** (problem, line 241): Wrap `<div className="notice pr-problem" role="alert">` in `<Notice>` with `onClose={() => dismiss()}`. Hook at component top: `const [hidden, dismiss] = useDismissal(problem ? problem.kind : null);` → return `null` while hidden.

2. **N11b** (commits, line 264): Same pattern with signature `base?.subtask_commits ? String(base.subtask_commits) : null`.

3. **N11c & N11d** (inside PrCard): Add two `useDismissal` calls at the top of `PrCard`, one per notice:
   ```ts
   const [hiddanBase, dismissBase] = useDismissal(misbased ? `${pr.base_branch}→${pr.expected_base}` : null);
   const [hiddenReady, dismissReady] = useDismissal(readyHint && s?.draft && s.state === "open" ? `${pr.id}:draft` : null);
   ```
   Then wrap the divs at lines 436 and 441 in `<Notice>` with respective `onClose` handlers.

### JiraView.tsx (T024)

1. Add hook at top: `const [hidden, dismiss] = useDismissal(expired ? "expired" : null);`.
2. Return `null` for the banner while hidden (line 336–342 logic).
3. Keep `.jv-body` driven by `expired` alone: `className={`jv-body ${expired ? "stale" : ""}`}` (no change).

---

## Existing Tests to Update (from plan §7)

- None listed for PR or Jira in this WP specifically; both are new tests (T021, T023).
- The old patterns (line ~52 in UpdateNotice.test.tsx, line ~607 in TaskScreen.test.tsx) are in other packages.

---

## Style Classes & Roles

| Notice | Class | Role |
|--------|-------|------|
| N11a: Problem | `notice pr-problem` | `alert` |
| N11b: Commits | `pr-base-warning` | `status` |
| N11c: Wrong base | `pr-base-warning` | `alert` |
| N11d: Ready hint | `pr-ready-hint` | (none; keep existing) |
| N12: Jira expired | `jv-banner` | `alert` |

All keep their existing classes; the `Notice` wrapper adds `ntc` for layout, never removes or replaces caller's classes.
