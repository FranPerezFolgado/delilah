# Research: Project Claude sessions open in the terminal

## R1. One switch component with a view-only mode, or a sibling component

- **Decision**: add an explicit view-only mode to `SessionViewSwitch`, as a discriminated union on a `mode`
  prop (`"view"` | `"runtime"`, the default).
- **Rationale**: the markup, labels ("Vista de la sesión", Chat with the beta tag, Terminal) and the `.svs`
  CSS stay the same in both places. The header slot is the same on task and project screens.
- **Alternatives**: a `SessionViewToggle` sibling, which duplicates the markup and styles; a plain `Segmented`
  inline in `TaskScreen`, which loses the shared labels and tag and makes the project header drift from the
  task header.

## R2. Default view for idea conversations

- **Decision**: idea conversations (`conversation: true`) keep defaulting to Chat. Every other project
  Claude session defaults to Terminal.
- **Rationale**: the idea flow is designed as a chat (decision 36: a chat view over the PTY, fed by hooks,
  with question and proposal cards). The report is about "Ask Claude" sessions.
- **Alternatives**: Terminal for every project session, which would hide the idea flow's cards by default.

## R3. Where the choice lives

- **Decision**: in `TaskScreen`'s existing in-memory `views` map, per session id, for as long as the screen
  is open.
- **Rationale**: task screens keep their per-session choice the same way. The spec does not ask for
  persistence, and storing it needs no migration.
- **Alternatives**: a `projects.settings` field or `localStorage`, left for later if the user asks.

## R4. The busy lock on projects

- **Decision**: in view-only mode, `busy` is ignored.
- **Rationale**: the lock exists because a runtime change restarts Claude mid-turn. A view flip restarts
  nothing, and the terminal stays mounted (`mounted` set), so its scrollback survives.

## R5. Daemon side

- **Decision**: no change. `run.rs::runtime_kind` keeps project sessions on a PTY, and
  `POST /sessions/{id}/runtime` keeps refusing them. The UI never calls it for projects.
