import type { Task, UsageReport } from "../api";
import type { ConnectionStatus } from "../connection";
import { t } from "../i18n";
import { statusOf, type StatusTone } from "../vocab";
import "./StatusBar.css";

/** The task groups the statusbar counts; clicking one filters the board by it. */
export type StatusFilter = Extract<StatusTone, "run" | "wait" | "pause" | "err">;

const NAMES: Record<string, string> = { five_hour: "5 h", seven_day: "Semana" };
const nameOf = (n: string) => (NAMES[n] ? t(NAMES[n]) : n);

const COUNTED: StatusFilter[] = ["run", "wait", "pause", "err"];

export interface StatusBarProps {
  status: ConnectionStatus;
  tasks: Task[];
  usage: UsageReport | null;
  onFilter: (status: StatusFilter) => void;
  onLogs: () => void;
}

export function StatusBar({ status, tasks, usage, onFilter, onLogs }: StatusBarProps) {
  const up = status === "up";
  const counts: Record<StatusFilter, number> = { run: 0, wait: 0, pause: 0, err: 0 };
  for (const task of tasks) {
    const tone = statusOf(task.status).tone;
    if ((COUNTED as string[]).includes(tone)) counts[tone as StatusFilter]++;
  }
  const label: Record<StatusFilter, string> = {
    run: t("{n} trabajando", { n: counts.run }),
    wait: t("{n} te espera", { n: counts.wait }),
    pause: t("{n} en pausa", { n: counts.pause }),
    err: t("{n} con error", { n: counts.err }),
  };

  return (
    <footer className="statusbar">
      <span className="sb-conn" role="status">
        <span className={up ? "dot sb-ok" : "dot sb-down"} />
        {up ? t("Conectado") : t("Sin conexión con delilahd · reintentando…")}
      </span>
      {up && (
        <span className="sb-counts">
          {COUNTED.filter((k) => counts[k] > 0).map((k) => (
            <button key={k} type="button" className={k === "err" ? "sb-btn sb-err" : "sb-btn"} onClick={() => onFilter(k)}>
              {label[k]}
            </button>
          ))}
        </span>
      )}
      <span className="sb-usage">
        {usage ? (
          Object.entries(usage.rate_limits).map(([name, w]) => {
            const used = Math.round(w.used_percentage ?? 0);
            return (
              <span key={name} className={used >= 80 ? "sb-hot" : undefined}>
                {t("Claude")} {nameOf(name)} <span className="mono">{used} %</span>
              </span>
            );
          })
        ) : (
          <span>{t("Uso de Claude: llega con el primer turno de un agente")}</span>
        )}
      </span>
      <button type="button" className="sb-btn" onClick={onLogs}>{t("Logs")}</button>
    </footer>
  );
}
