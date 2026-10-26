# Research: Session tabs named after their phase

No open unknowns in the Technical Context. These are the decisions taken from reading the code.

## R1. Where the wrong name comes from

- **Decision**: In two places in the UI: `TaskScreen.tsx` (tab items:
  `label: s.primary ? t("Claude principal") : s.label`) and `TaskView.tsx` (Sessions list:
  `x.primary && x.kind === "claude" ? t("Claude principal") : x.label`).
- **Rationale**: The daemon already stores the phase's name as the session's label
  (`handoff.rs` `step_label`) and sends `step` on every session (`api.ts` `Session.step`).
  The UI throws it away for the primary session, which in saving mode is always the
  current phase's.
- **Alternatives considered**: Changing the daemon's labels: not needed, they are already
  right.

## R2. Name from `step` (translated in the UI) or from the stored `label`

- **Decision**: Use `step` through the existing `stepLabel()` in `app/src/chain.ts`.
- **Rationale**: The stored label is written once, in the daemon's language, when the
  session starts. `stepLabel` gives the same names the session chain shows (FR-005) and
  follows the UI language (SC-004). The stored label stays in the tooltip (FR-006).
- **Alternatives considered**: Showing `s.label` for primaries with a step. It is simpler,
  but the name stays in the language the session was created in, and it differs from
  the chain's names.

## R3. Where the helper lives

- **Decision**: `sessionName(session)` exported from `app/src/chain.ts`.
- **Rationale**: `chain.ts` owns the step names and already imports `Session`. One helper
  keeps the tab bar and the Sessions list in step (FR-004).
- **Alternatives considered**: A new `sessionName.ts` module adds a file for one line.
  Inlining the rule in each component is how the two places drifted apart in the
  first place.

## R4. Non-saving-mode tasks

- **Decision**: Keep «Claude principal» for a primary session without a step.
- **Rationale**: That session works through every phase. Naming it after the current
  phase is not asked for (spec Assumptions) and would need the phase logic of the
  header in the tab bar.
- **Alternatives considered**: Labelling it with `headerPhaseOf(task)`. Out of scope.
