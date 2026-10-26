import type { PrSnapshot, PrSummary } from "../api";
import { Icon, type IconName } from "./ui";
import { t } from "../i18n";
import "./PrBadge.css";

const STATES: Record<"draft" | PrSnapshot["state"], { icon: IconName; label: string }> = {
  draft: { icon: "pause", label: "Borrador" },
  open: { icon: "review", label: "Abierta" },
  merged: { icon: "done", label: "Fusionada" },
  closed: { icon: "close", label: "Cerrada" },
};

export const REVIEWS: Record<NonNullable<PrSnapshot["review_decision"]>, string> = {
  approved: "Aprobada",
  "changes-requested": "Cambios pedidos",
  "review-required": "Revisión pendiente",
};

const CI: Record<PrSnapshot["ci"], string> = {
  pending: "CI en curso",
  success: "CI correcto",
  failure: "CI fallido",
  none: "Sin CI",
};

/** The state of a task's PR(s) in a glance: its state icon and a dot for the CI. */
export function PrBadge({ pr, autofix }: { pr: PrSummary; autofix?: boolean }) {
  const state = STATES[pr.draft && pr.state === "open" ? "draft" : pr.state];
  const review = pr.review_decision ? ` · ${t(REVIEWS[pr.review_decision])}` : "";
  const count = pr.count > 1 ? ` (${pr.count})` : "";
  return (
    <span
      className={`pr-badge pr-${pr.state}${pr.draft ? " pr-draft" : ""}`}
      title={`${t(state.label)}${count} · ${t(CI[pr.ci])}${review}`}
    >
      <Icon name={state.icon} />
      {pr.ci !== "none" && <span className={`pr-dot pr-ci-${pr.ci}`} data-ci={pr.ci} />}
      {autofix && (
        <span className="pr-autofix-mark" title={t("Autofix de CI activado")}>
          <Icon name="retry" />
        </span>
      )}
      {pr.count > 1 && <span className="pr-count">{pr.count}</span>}
    </span>
  );
}

export { STATES as PR_STATES, CI as PR_CI };
