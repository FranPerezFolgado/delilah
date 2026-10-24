# Data model: One "Ideas" row per project

No new stored data. What the feature reads:

- **Session** (`app/src/api.ts`, unchanged): `id`, `task` (owner, `@<project>` for project sessions), `conversation` (true = idea conversation), `state` (`"waiting"` = waits for the user). Listed in creation order.
- **Project screen state** (`app/src/editor/session.ts`, unchanged): `active` = the tab the owner's screen last showed; a session tab is `SESSION + id`.
- **Ideas row** (derived, per project): `ideas = sessions of @project with conversation`; shown when `ideas.length > 0`; `count = ideas.length`; `waiting = ideas.some(state === "waiting")`.

Choice when opened (`ideaToOpen`): first `waiting` → `lastShown` if in `ideas` → last of `ideas`.
