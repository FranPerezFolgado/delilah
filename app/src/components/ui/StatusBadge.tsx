import type { TaskStatus } from "../../api";
import { statusOf } from "../../vocab";
import "./StatusBadge.css";

export interface StatusBadgeProps {
  status: TaskStatus | "backlog";
  /** Show the detail (reason, resume time) after the label; it is always in the tooltip. */
  detail?: boolean;
}

export function StatusBadge({ status, detail }: StatusBadgeProps) {
  const v = statusOf(status);
  return (
    <span className={`badge ${v.tone}`} title={v.detail}>
      {v.label}
      {detail && v.detail ? ` · ${v.detail}` : ""}
    </span>
  );
}
