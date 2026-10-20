import type { CSSProperties } from "react";
import { t } from "../i18n";
import { phaseLabel, type HeaderPhase } from "../phases";

/** The task's current phase in the task screen header: its step and name. Not clickable. */
export function PhaseBadge({ phase, style }: { phase: HeaderPhase; style?: CSSProperties }) {
  const label = t("Fase: {phase}", { phase: phaseLabel(phase.column) });
  return (
    <span className="pill phase-pill" title={label} aria-label={label} style={style}>
      <span className="dot-step">{phase.step}</span>
      {phaseLabel(phase.column)}
    </span>
  );
}
