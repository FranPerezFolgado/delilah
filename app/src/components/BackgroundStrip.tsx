import { useEffect, useState } from "react";
import type { BackgroundItem } from "../api";
import { t } from "../i18n";
import "./BackgroundStrip.css";

/** Seconds as "12 s" or "3 min 05 s". */
function duration(secs: number): string {
  const s = Math.max(0, Math.floor(secs));
  if (s < 60) return `${s} s`;
  return `${Math.floor(s / 60)} min ${String(s % 60).padStart(2, "0")} s`;
}

function stateLabel(state: string): string {
  switch (state) {
    case "running":
      return t("en curso");
    case "completed":
      return t("terminado");
    case "failed":
      return t("fallido");
    case "stopped":
      return t("detenido");
    default:
      return t("estado desconocido");
  }
}

function clock(item: BackgroundItem, now: number): string {
  const start = item.started_at ? Date.parse(item.started_at) : NaN;
  if (item.state === "running") {
    return t("desde hace {time}", { time: Number.isNaN(start) ? "?" : duration((now - start) / 1000) });
  }
  const end = item.ended_at ? Date.parse(item.ended_at) : NaN;
  if (Number.isNaN(start) || Number.isNaN(end)) return "";
  return duration((end - start) / 1000);
}

/** Commands and sub-agents left running in the background, one row each. */
export function BackgroundStrip({ items }: { items: BackgroundItem[] }) {
  const [now, setNow] = useState(() => Date.now());
  const ticking = items.some((i) => i.state === "running");
  useEffect(() => {
    if (!ticking) return;
    setNow(Date.now());
    const timer = setInterval(() => setNow(Date.now()), 1000);
    return () => clearInterval(timer);
  }, [ticking]);
  if (items.length === 0) return null;
  return (
    <div className="bg-strip" data-testid="background-strip">
      <div className="bg-title muted">{t("En segundo plano")}</div>
      {items.map((item) => (
        <div key={item.task_id} className={`bg-row ${item.state}`}>
          <span className="bg-kind" aria-hidden="true">{item.kind === "agent" ? "◇" : "›_"}</span>
          <span className="bg-desc">{item.description}</span>
          <span className="bg-clock muted">{clock(item, now)}</span>
          <span className="bg-state">{stateLabel(item.state)}</span>
        </div>
      ))}
    </div>
  );
}
