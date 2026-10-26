import { useEffect, useState } from "react";
import type { TurnProgress } from "../api";
import { t } from "../i18n";

/** Seconds as "12 s" or "3 min 05 s". */
function duration(secs: number): string {
  const s = Math.max(0, Math.floor(secs));
  if (s < 60) return `${s} s`;
  return `${Math.floor(s / 60)} min ${String(s % 60).padStart(2, "0")} s`;
}

/** The turn in progress: a ticking clock, the current step and, after 30 s of silence, how long. */
export function WorkingIndicator({ turn }: { turn: TurnProgress | null }) {
  const [now, setNow] = useState(() => Date.now());
  const live = turn !== null;
  useEffect(() => {
    if (!live) return;
    setNow(Date.now());
    const timer = setInterval(() => setNow(Date.now()), 1000);
    return () => clearInterval(timer);
  }, [live]);
  if (!turn) return null;
  const elapsed = (now - Date.parse(turn.started_at)) / 1000;
  const silent = (now - Date.parse(turn.last_signal_at)) / 1000;
  return (
    <div className="conv-working muted" data-testid="working-indicator">
      <span className="conv-working-dot" aria-hidden="true" />
      <span>{t("Trabajando")}</span>
      <span className="conv-working-clock">{duration(elapsed)}</span>
      {turn.step && <span className="conv-working-step">{turn.step}</span>}
      {silent >= 30 && <span>{t("sin novedades desde hace {time}", { time: duration(silent) })}</span>}
    </div>
  );
}
