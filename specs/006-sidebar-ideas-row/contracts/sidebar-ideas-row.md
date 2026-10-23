# UI contract: the sidebar's "Ideas" row

Internal to the web UI (no HTTP or CLI change).

## Sidebar props

- New: `onOpenIdeas: (owner: string, ideas: Session[]) => void`, called with the project's owner (`@<name>`) and its idea conversations in list order.
- Unchanged: `onOpenSession(owner, sessionId)` for every other session row.

## Row

- Element: `<button class="srow ideas">` inside the project's `.pgroup`, first after the project head.
- Content: idea icon, text `t("Ideas")`, count `<span class="pcount mono [c-wait]">n</span>`.
- Accessible name and title: `t("Ideas de {name} ({n})")`.
- Absent when the project has no idea conversation or is folded.

## `ideaToOpen(ideas, lastShown)` (`app/src/components/ideas.ts`)

| ideas (state) | lastShown | returns |
|---|---|---|
| a, b(waiting), c(waiting) | a | b |
| a, b, c | a | a |
| a, b, c | x (not an idea) / null | c |
