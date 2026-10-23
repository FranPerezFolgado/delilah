# Research: One "Ideas" row per project

## R1. Where "last shown tab" lives

- **Decision**: read `sessionFor(owner).active` (`app/src/editor/session.ts`) in App and turn it into a session id with `sessionOf`.
- **Rationale**: it is what the project's screen already persists per viewer (`delilah.tabs.<owner>` in localStorage) and what `openTask` writes; nothing new to store.
- **Alternatives**: a new "last idea" key per project (duplicates state); reading it inside Sidebar (couples the sidebar to the editor state).

## R2. "Newest" idea

- **Decision**: the last idea in the list the app receives.
- **Rationale**: `Session` has no timestamp; the daemon lists sessions in creation order (the sidebar already relies on the daemon's order for tasks). Adding a timestamp would be an API change the spec rules out.
- **Alternatives**: a `created_at` on `Session` (API + migration for little gain).

## R3. Waiting

- **Decision**: an idea waits when `session.state === "waiting"`, the same test `waitingHere` uses today.
- **Alternatives**: the inbox items (would need another prop; the state already says it).

## R4. Where the choice runs

- **Decision**: a pure `ideaToOpen` in `components/ideas.ts`, called by App; Sidebar passes the project's ideas through a new `onOpenIdeas` prop.
- **Rationale**: testable without rendering; keeps `onOpenSession(owner, id)` unchanged for other rows.
