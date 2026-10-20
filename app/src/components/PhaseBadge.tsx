import { t } from "../i18n";
import { phaseLabel, type HeaderPhase } from "../phases";

/** The task's current phase in the task screen header: its step and name, and the
 *  items done out of total when the phase has a count. Not clickable. */
export function PhaseBadge({ phase }: { phase: HeaderPhase }) {
  const name = phaseLabel(phase.column);
  const count = phase.total ? ` · ${phase.done ?? 0}/${phase.total}` : "";
  return (
    <span
      className="pill phase-pill"
      title={t("Fase: {phase}", { phase: name }) + count}
      aria-label={t("Fase {step}: {phase}", { step: phase.step, phase: name }) + count}
    >
      <span className="dot-step">{phase.step}</span>
      {name}
      {count && <span className="muted">{count}</span>}
    </span>
  );
}
