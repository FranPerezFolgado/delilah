import { t } from "../i18n";
import type { Autopilot } from "../api";
import { Icon } from "./Icon";

/** Autopilot state of a task: running, paused (reason in the tooltip) or done. Nothing
 *  when autopilot is off. */
export function AutopilotMark({ autopilot }: { autopilot?: Autopilot | null }) {
  if (!autopilot) return null;
  const { state, stage, reason } = autopilot;
  const label =
    state === "paused" ? t("Autopiloto en pausa") : state === "done" ? t("PR abierta por autopiloto") : t("Autopiloto");
  const title = state === "paused" && reason ? `${label}: ${reason}` : label;
  const suffix = state === "running" && stage === "preview" ? t("Preview") : state === "running" && stage === "opening_pr" ? t("Abriendo la PR") : "";
  return (
    <span className={`pill autopilot-pill autopilot-${state}`} title={title}>
      <Icon name={state === "paused" ? "pause" : state === "done" ? "done" : "run"} />
      {label}
      {suffix && <span className="muted">{suffix}</span>}
    </span>
  );
}
