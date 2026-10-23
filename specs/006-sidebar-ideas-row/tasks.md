# Tasks: One "Ideas" row per project in the sidebar

**Input**: Design documents from `specs/006-sidebar-ideas-row/`

**Prerequisites**: plan.md, spec.md, research.md, data-model.md, contracts/sidebar-ideas-row.md, quickstart.md

**Tests**: included, test-first (the plan lists the cases; project convention).

**Paths**: all under `app/src/` unless stated. Checks: `cd app && npx tsc --noEmit && npx vitest related --run <files>`; everything: `npx vitest run`.

## Format: `[ID] [P?] [Story] Description`

## Phase 1: Setup

None: no new dependency or config.

---

## Phase 2: Foundational — choosing the idea to open

**Purpose**: the pure choice used by US2; the Sidebar row (US1) does not depend on it.

- [X] T001 [P] Write failing tests in `app/src/components/ideas.test.ts` for `ideaToOpen(ideas: Session[], lastShown: string | null): Session` (contract table in `contracts/sidebar-ideas-row.md`): a single idea → it; `a, b(waiting), c(waiting)` with lastShown `a` → `b` (first waiting wins over last shown); `a, b, c` with lastShown `a` → `a`; lastShown `"x"` (not in list, e.g. a closed idea or a read-only session) → `c`; lastShown `null` → `c`. Build sessions as in `Sidebar.test.tsx` (`{ id, task: "@web", kind: "claude", label: "Idea", primary: false, closed: false, live: true, read_only: false, state, conversation: true, step: null } as Session`).
- [X] T002 Implement `ideaToOpen` in `app/src/components/ideas.ts` (new file): `ideas.find(s => s.state === "waiting") ?? ideas.find(s => s.id === lastShown) ?? ideas[ideas.length - 1]`, with a doc comment saying "newest" is the last listed because the daemon lists sessions in creation order. T001 passes.

**Checkpoint**: `npx vitest related --run src/components/ideas.ts` green.

---

## Phase 3: User Story 1 — a single "Ideas" row (P1) 🎯 MVP

**Goal**: one "Ideas" row with icon and count per project with ideas, first under the project; no row per idea; other sessions unchanged.

**Independent Test**: a project with three ideas and one read-only session shows one "Ideas 3" row followed by one read-only row.

- [X] T003 [US1] Rewrite the "Sidebar ideas" describe in `app/src/components/Sidebar.test.tsx` (keep the existing "starts an idea conversation from the project's second button" test; replace "lists a conversation session with the idea icon"). Add `onOpenIdeas = vi.fn()` to `setup()` and pass it to `<Sidebar>`. Failing tests: no ideas → no `.srow.ideas`; one idea → row text contains "Ideas" and count `1`; three ideas + one read-only session (`read_only: true, conversation: false`) → the first `.srow` under the project is `.srow.ideas` with count `3`, there is exactly one other session `.srow` (the read-only one), and no `.srow` per idea; the row is a `button` with accessible name "Ideas de web (3)" (language `es`, as in the describe); folding the project (click its chevron, `aria-label` "Plegar") → no `.srow.ideas`, and the project's `.pcount` shows `4` (sessions + tasks still counted, ideas included).
- [X] T004 [US1] In `app/src/components/Sidebar.tsx`: add the required prop `onOpenIdeas: (owner: string, ideas: Session[]) => void` with a doc comment ("Shows the project's screen on one of its idea conversations."); inside the project loop compute `ideas = g.sessions.filter(s => s.conversation)` and `others = g.sessions.filter(s => !s.conversation)`, leaving `count` and `waitingHere` on all of `g.sessions`; when `!folded && ideas.length > 0` render, before the other sessions, `<button className="srow ideas" onClick={() => onOpenIdeas(g.owner!, ideas)} title={label} aria-label={label}>` with `<Icon name="idea" className="idea" />`, `<span className="trunc" style={{ flex: 1 }}>{t("Ideas")}</span>` and `<span className="pcount mono">{ideas.length}</span>`, where `label = t("Ideas de {name} ({n})", { name: g.project!.name, n: ideas.length })`; map `others` instead of `g.sessions` for the session rows and drop their `s.conversation` icon branch (keep the `kind === "shell"` / status icon branch).
- [X] T005 [P] [US1] Add to `app/src/i18n/en.ts`, next to "Nueva tarea con Claude en {name}": `"Ideas": "Ideas"` and `"Ideas de {name} ({n})": "Ideas in {name} ({n})"`.
- [X] T006 [US1] In `app/src/App.tsx`, pass a temporary `onOpenIdeas={(owner, ideas) => openTask(owner, ideas[ideas.length - 1].id)}` to `<Sidebar>` so the app compiles (replaced in T009). `npx tsc --noEmit` and T003 green.

**Checkpoint**: US1 works on its own: the row replaces the idea rows and opens the newest idea.

---

## Phase 4: User Story 2 — open the right idea (P1)

**Goal**: clicking "Ideas" opens the waiting idea, else the last shown idea, else the newest.

**Independent Test**: click "Ideas" with one waiting idea → the project's screen opens on it.

- [X] T007 [US2] Add a failing test to the "Sidebar ideas" describe in `app/src/components/Sidebar.test.tsx`: with ideas `a, b, c` and a read-only session, clicking `.srow.ideas` calls `onOpenIdeas("@web", [a, b, c])` (ideas only, list order).
- [X] T008 [US2] Make T007 pass in `app/src/components/Sidebar.tsx` if T004 did not already (the row passes exactly the filtered `ideas`).
- [X] T009 [US2] In `app/src/App.tsx`, replace T006's handler with `const openIdeas = (owner: string, ideas: Session[]) => openTask(owner, ideaToOpen(ideas, sessionOf(sessionFor(owner).active)).id);` next to `openConversation` (doc comment: "The project's screen on one of its ideas: see `ideaToOpen`."), importing `ideaToOpen` from `./components/ideas` and `sessionOf` from `./editor/session` (already imports `sessionFor`, `persist`, `sessionTab`, `CHANGES`); pass `onOpenIdeas={openIdeas}`. `npx tsc --noEmit` green.

**Checkpoint**: US1 + US2 = the feature.

---

## Phase 5: User Story 3 — see that an idea waits (P2)

**Goal**: the row's count has the waiting highlight when any idea waits.

**Independent Test**: with one of two ideas `state: "waiting"`, the count has `c-wait`; with none, it does not.

- [ ] T010 [US3] Add failing tests to the "Sidebar ideas" describe in `app/src/components/Sidebar.test.tsx`: ideas with states `working` and `waiting` → `.srow.ideas .pcount` has class `c-wait`; both `working` → no `c-wait`.
- [ ] T011 [US3] In `app/src/components/Sidebar.tsx`, give the row's count `className={\`pcount mono ${ideas.some(s => s.state === "waiting") ? "c-wait" : ""}\`}`. T010 green.

---

## Phase 6: Polish & cross-cutting

- [ ] T012 [P] `docs/status.md`: in the idea-conversation bullet of what is built, add that the sidebar groups a project's idea conversations in one "Ideas" row (count, waiting highlight) that opens the waiting, last shown or newest idea.
- [ ] T013 [P] `README.md`: where the sidebar / "Nueva tarea con Claude" is described, say ideas appear as one "Ideas" row per project and as tabs on the project's screen.
- [ ] T014 [P] `docs/decisions.md`: new numbered entry (next number after the last one, dated 2026-10-23): the sidebar groups only idea conversations into "Ideas" (read-only Claude and terminals keep rows, their purpose differs); the row opens waiting → last shown → newest, and "newest" relies on the daemon listing sessions in creation order (no timestamp on `Session`).
- [ ] T015 Run `cd app && npx tsc --noEmit && npx vitest run`; all green.
- [ ] T016 Try it in a preview following `specs/006-sidebar-ideas-row/quickstart.md` (`scripts/preview.sh --fresh`, never the installed daemon on 7420), or hand the quickstart scenarios to the `ui-verifier` agent with the preview URL.
- [ ] T017 Version bump, minor (new user-facing behaviour): from `main`'s version (0.5.0 now; re-check) to 0.6.0 in `Cargo.toml` (workspace), `app/src-tauri/Cargo.toml`, `app/package.json`, `app/src-tauri/tauri.conf.json`; refresh lockfiles with `cargo check --workspace`, `(cd app/src-tauri && cargo check)`, `(cd app && npm install --package-lock-only)`; commit alone as `chore: bump version to 0.6.0`.

---

## Dependencies & Execution Order

- Phase 2 (T001→T002) and US1 (T003→T004→T006, T005 any time) can run in parallel: different files.
- US2 needs T002 (ideaToOpen) and T004 (the row): T007→T008→T009.
- US3 needs T004: T010→T011.
- Polish after US1–US3; T017 last.

### Parallel example

```text
T001 (ideas.test.ts)   ‖  T003 (Sidebar.test.tsx)  ‖  T005 (en.ts)
T012 (status.md)       ‖  T013 (README.md)         ‖  T014 (decisions.md)
```

## Implementation Strategy

- **MVP**: Phase 3 (US1) alone already removes the pile-up and opens the newest idea.
- Then US2 (right idea), US3 (waiting highlight), polish, bump. Commit per phase.
