# Research: Task phase in the task screen header

## R1. Where the phase comes from

- **Decision**: `columnOf(task, spec)` from `app/src/phases.ts`, with the task's
  `SpecSummary` from `App`'s `specs` map (`specs[owner]`).
- **Rationale**: the board (`BoardView`, via `boardColumnOf`) and the spec panel
  (`SpecPanel`) already use it. It goes through the recorded phase, then the spec's
  phase, then `specify`, so one rule keeps all three views in agreement (FR-002).
- **Alternatives**: `task.phase` alone. It is null until a phase is recorded, so the
  header and the board would disagree. Fetching `SpecProgress` in the header: an extra
  request, when the summary is already in memory.

## R2. When to hide it

- **Decision**: hidden for `queued`, `done`, and when there is no `SpecSummary`.
- **Rationale**: the status pill already says "Backlog" or "Hecha". `TaskScreen` isn't
  even rendered for queued tasks (`App.tsx`). Without a summary the spec panel shows
  nothing either, so hiding it in the header matches.
- **Alternatives**: falling back to "Especificación" with no spec. That would show a
  phase the spec panel doesn't.

## R3. Look

- **Decision**: a pill with the spec panel stepper's current-step look (accent border and
  background), the step number and the label. No icon.
- **Rationale**: it ties the badge to the stepper the user already knows. The status
  pill uses coloured text and an icon, so the two read as different things (FR-006).
- **Alternatives**: plain muted text, which is easy to miss next to the terminal. A
  mini-stepper with all four steps, which takes too much room in the header.

## R4. Progress

- **Decision**: `· done/total` only in Implementación with `total > 0`.
- **Rationale**: that is the spec's FR-005. The board card shows progress whenever
  `total > 0`, but outside implement the count is 0/N and tells you nothing.

## R5. Freshness

- **Decision**: no new subscription. `tasks` and `specs` are already refreshed by `App`
  (SSE topics, with 30 s polling as the fallback).
- **Rationale**: the header changes as fast as the board (SC-003).
