import { t } from "../i18n";
import { phaseLabel, type HeaderPhase } from "../phases";

/** The task's current phase in the task screen header: its step and name. Not clickable. */
export function PhaseBadge({ phase }: { phase: HeaderPhase }) {
  const name = phaseLabel(phase.column);
  return (
    <span
      className="pill phase-pill"
      title={t("Fase: {phase}", { phase: name })}
      aria-label={t("Fase {step}: {phase}", { step: phase.step, phase: name })}
    >
      <span className="dot-step">{phase.step}</span>
      {name}
    </span>
  );
}
