import type { PrSnapshot } from "../../api";
import { t } from "../../i18n";
import { prStateOf, type PrState } from "../../vocab";
import { Icon } from "../Icon";
import "./PrChip.css";

export interface PrChipProps {
  /** A linked PR, or just its number and state. */
  pr: { number: number; snapshot?: PrSnapshot | null; state?: PrState };
}

const LABEL: Record<PrState, string> = {
  draft: "Borrador",
  open: "Abierta",
  failing: "Checks fallan",
  merged: "Fusionada",
};

export function PrChip({ pr }: PrChipProps) {
  const v = pr.state ? { state: pr.state, label: t(LABEL[pr.state]) } : prStateOf({ snapshot: pr.snapshot ?? null });
  return (
    <span className={`prchip ${v.state}`}>
      <Icon name={v.state === "merged" ? "merge" : "pr"} />
      <span className="num">#{pr.number}</span>
      {v.label}
    </span>
  );
}
