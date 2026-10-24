# Implementation Plan: One "Ideas" row per project in the sidebar

**Branch**: `feature/LIBRE-14` | **Date**: 2026-10-23 | **Spec**: [spec.md](spec.md)

**Input**: Feature specification from `specs/006-sidebar-ideas-row/spec.md`

## Summary

The sidebar stops drawing one row per idea conversation and draws one "Ideas" row
per project (idea icon, count, waiting highlight), first under the project. Clicking
it opens the project's screen on the idea chosen by a small pure function: the first
waiting idea, else the one the project's screen last showed, else the newest.
Web UI only; no daemon, API or migration change.

## Technical Context

**Language/Version**: TypeScript, React (Vite) in `app/`

**Primary Dependencies**: none new

**Storage**: none new; reads the per-viewer tab state already kept in `localStorage` (`app/src/editor/session.ts`, `delilah.tabs.<owner>`)

**Testing**: vitest + @testing-library/react (jsdom), `cd app && npx tsc --noEmit && npx vitest related --run <files>`

**Target Platform**: Delilah desktop app (Tauri webview) and the browser preview

**Project Type**: desktop app, web front end

**Performance Goals**: n/a (a filter over the sessions already in memory)

**Constraints**: text keys are the Spanish originals translated in `app/src/i18n/en.ts`

**Scale/Scope**: 3 source files, 2 test files, 1 translation file, docs

## Constitution Check

The constitution is the unfilled template; the project's rules are in `CLAUDE.md`:

- Contracts (`api.rs` ↔ `api.ts`): untouched. PASS
- Migrations: none. PASS
- Try changes in a preview (`scripts/preview.sh`), never on the installed daemon: in quickstart. PASS
- Docs kept true (`docs/status.md`, `README.md`) and a version bump (minor: new user-facing behaviour): in the tasks. PASS

Re-checked after design: no change.

## Design

### 1. Choosing the idea to open — `app/src/components/ideas.ts` (new)

```ts
/** The idea conversation the "Ideas" row opens: the first one waiting for the user,
 *  else the one the project's screen last showed, else the newest (last listed). */
export function ideaToOpen(ideas: Session[], lastShown: string | null): Session
```

Pure, so it is tested without rendering. `ideas` is non-empty (the row only exists
then). `lastShown` counts only if `ideas.find(s => s.id === lastShown)`: a stale id
(a closed idea, or a non-idea tab) falls through to the newest. "Newest" is the last
in the list: the daemon lists sessions in creation order (research R2).

Tests (`ideas.test.ts`): waiting first (and the first of several waiting), last shown,
stale/foreign/null `lastShown` → newest, a single idea.

### 2. Sidebar — `app/src/components/Sidebar.tsx`

- In `group()`, split the project's sessions: `ideas = sessions.filter(s => s.conversation)`, `others = …!s.conversation`. `count` and `waitingHere` for the folded project keep using all sessions (spec edge case).
- When unfolded and `ideas.length > 0`, draw one `srow` button before the other sessions:
  `<Icon name="idea" />`, `t("Ideas")`, and `<span className="pcount mono {c-wait if any idea waits}">{n}</span>`.
  Accessible name `t("Ideas de {name} ({n})")`; title the same.
- Its click calls a new prop `onOpenIdeas(owner: string, ideas: Session[])`.
- The row's icon keeps today's idea styling, `<Icon name="idea" className="idea" />` (the `.idea` CSS stays); its state is shown by the count's `c-wait`, not by a per-idea status class.
- The other sessions keep today's row; the `s.conversation` branch of the icon goes away (only non-ideas reach it).
- It is a native `<button>`, so it is keyboard-reachable (FR-009).
- `onOpenIdeas` is a new required prop: `App.tsx` and the test `setup()` are its only callers.

Tests (`Sidebar.test.tsx`, "Sidebar ideas" rewritten): no ideas → no row; one idea →
count 1; three ideas + a read-only session → the Ideas row first and one row for the
read-only session, no row per idea; a waiting idea → count has `c-wait`; folded project
→ no row, project count includes the ideas; the row is a button named
"Ideas de web (3)"; click → `onOpenIdeas("@web", ideas)` in list order.

### 3. App — `app/src/App.tsx`

```ts
const openIdeas = (owner: string, ideas: Session[]) =>
  openTask(owner, ideaToOpen(ideas, sessionOf(sessionFor(owner).active)).id);
```

`sessionFor`/`sessionOf` already exist in `editor/session.ts`; App already imports
from it. The Sidebar stays free of the editor state. `sessionFor` creates and caches
the owner's state when absent (restored from localStorage), which is harmless here;
`openTask` then sets `active` to the chosen idea and persists it, which is what makes
"last shown" stick for the next click.

### 4. Text — `app/src/i18n/en.ts`

`"Ideas": "Ideas"`, `"Ideas de {name} ({n})": "Ideas in {name} ({n})"`.

### 5. Docs

`docs/status.md` (built list) and `README.md` (sidebar description): one line each.
`docs/decisions.md`: a new entry — only idea conversations are grouped (read-only
Claude and terminals keep rows), and "newest" relies on the daemon listing sessions
in creation order.

## Project Structure

### Documentation (this feature)

```text
specs/006-sidebar-ideas-row/
├── spec.md
├── plan.md
├── research.md
├── data-model.md
├── quickstart.md
├── contracts/sidebar-ideas-row.md
└── tasks.md            # /speckit-tasks
```

### Source Code (repository root)

```text
app/src/
├── components/
│   ├── ideas.ts            # new: ideaToOpen
│   ├── ideas.test.ts       # new
│   ├── Sidebar.tsx         # Ideas row, onOpenIdeas prop
│   └── Sidebar.test.tsx    # "Sidebar ideas" tests rewritten
├── App.tsx                 # openIdeas
└── i18n/en.ts              # 2 keys
```

**Structure Decision**: front end only, inside the existing `app/src/components` module.

## Complexity Tracking

None.
