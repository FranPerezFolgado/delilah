import type { TaskStatus } from "../../api";
import { t } from "../../i18n";
import { phaseLabel, type HeaderPhase } from "../../phases";
import "./PhaseBars.css";

export interface PhaseBarsProps {
  phase: HeaderPhase | null;
  status: TaskStatus;
  mini?: boolean;
  /** Compact / rail: the text goes to the tooltip only. */
  labelInTooltip?: boolean;
}

export function PhaseBars({ phase, status, mini, labelInTooltip }: PhaseBarsProps) {
  if (status.type === "done") return null;
  let on = 0;
  let label: string;
  if (status.type === "setup-failed") label = t("Sin iniciar");
  else if (phase) {
    on = phase.step;
    label = `${phaseLabel(phase.column)} · ${phase.step}/4`;
    if (phase.total) label += ` · ${t("{done}/{total} tareas", { done: phase.done ?? 0, total: phase.total })}`;
  } else return null;
  return (
    <span className="phasebars" title={label} aria-label={label}>
      <span className={mini ? "bars mini" : "bars"} aria-hidden="true">
        {[1, 2, 3, 4].map((n) => (
          <i key={n} className={n <= on ? "on" : undefined} />
        ))}
      </span>
      {!labelInTooltip && <span className="lbl">{label}</span>}
    </span>
  );
}
