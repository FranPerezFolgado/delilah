import { useEffect, useRef } from "react";
import type { Task, UsageReport } from "../api";
import { t, tn } from "../i18n";
import { Button, dismissToastByKey, pushToast } from "./ui";
import "./GlobalNotices.css";

const HOT = 90;
/** The one grouped «te espera» toast, updated in place by key. */
const WAITING_TOAST_KEY = "waiting";
const MAX_PAUSED = 3;
const WINDOWS: Record<string, string> = { five_hour: "5 h", seven_day: "Semana" };

/** `resets_at` arrives in epoch seconds; tolerate milliseconds. */
const clock = (at: number) =>
  new Date(at < 1e12 ? at * 1000 : at).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });

interface Props {
  tasks: Task[];
  usage: UsageReport | null;
  /** The task whose screen is showing, if any: it needs no «te espera» toast. */
  current: string | null;
  onOpenTask: (key: string) => void;
  onOpenInbox: () => void;
}

/** Quota and autopilot-pause notices, and the «te espera» toast; only existing /usage and /tasks data. */
export function GlobalNotices({ tasks, usage, current, onOpenTask, onOpenInbox }: Props) {
  const seen = useRef<Set<string> | null>(null);
  const group = useRef<string[]>([]);
  const open = useRef({ current, onOpenTask, onOpenInbox });
  open.current = { current, onOpenTask, onOpenInbox };

  // A task that starts waiting while you are elsewhere; the first loaded list only seeds.
  useEffect(() => {
    // The tasks may arrive after the first render (empty list): seed on the first non-empty one.
    if (!seen.current && tasks.length === 0) return;
    const waiting = new Set(tasks.filter((k) => k.status.type === "waiting-for-user").map((k) => k.key));
    const before = seen.current;
    seen.current = waiting;
    if (!before) return;
    for (const key of waiting) if (!before.has(key) && !group.current.includes(key)) group.current.push(key);
  }, [tasks]);

  // The waiting group: one toast for every task waiting that you have not seen.
  useEffect(() => {
    const waiting = new Set(tasks.filter((k) => k.status.type === "waiting-for-user").map((k) => k.key));
    const g = group.current.filter((key) => waiting.has(key) && key !== current);
    group.current = g;
    if (g.length === 0) {
      dismissToastByKey(WAITING_TOAST_KEY);
      return;
    }
    const onDismiss = () => {
      group.current = [];
    };
    if (g.length === 1) {
      const key = g[0];
      pushToast({
        key: WAITING_TOAST_KEY,
        kind: "wait",
        text: t("{key} te espera", { key }),
        action: { label: t("Responder"), run: () => open.current.onOpenTask(key) },
        onDismiss,
      });
    } else {
      pushToast({
        key: WAITING_TOAST_KEY,
        kind: "wait",
        text: tn(g.length, "{n} tarea te espera", "{n} tareas te esperan"),
        action: { label: t("Ver"), run: () => open.current.onOpenInbox() },
        onDismiss,
      });
    }
  }, [tasks, current]);

  useEffect(() => () => dismissToastByKey(WAITING_TOAST_KEY), []);

  const hot = Object.entries(usage?.rate_limits ?? {}).filter(([, w]) => (w.used_percentage ?? 0) >= HOT);
  const paused = tasks.filter((k) => k.autopilot?.state === "paused");
  const limited = tasks.filter((k) => k.status.type === "paused-by-limit").length;

  return (
    <div className="gn">
      {hot.map(([name, w]) => {
        const used = Math.round(w.used_percentage ?? 0);
        const win = WINDOWS[name] ? t(WINDOWS[name]) : name;
        const full = used >= 100;
        return (
          <div key={name} className="gn-bar gn-wait" role="status" title={t("Quedan pocas peticiones en esta ventana de uso")}>
            <span className="grow">
              {full
                ? w.resets_at
                  ? t("Límite de {window} alcanzado · se reanuda a las {time}", { window: win, time: clock(w.resets_at) })
                  : t("Límite de {window} alcanzado", { window: win })
                : t("Claude {window} al {pct} %", { window: win, pct: used })}
            </span>
            {full && limited > 0 && <span>{tn(limited, "{n} tarea en pausa", "{n} tareas en pausa")}</span>}
          </div>
        );
      })}
      {paused.slice(0, MAX_PAUSED).map((k) => (
        <div key={k.key} className="gn-bar gn-pause" role="status">
          <span className="grow gn-reason" title={k.autopilot?.reason}>
            {k.autopilot?.reason
              ? t("{key} · autopiloto en pausa: {reason}", { key: k.key, reason: k.autopilot.reason })
              : t("{key} · autopiloto en pausa", { key: k.key })}
          </span>
          <Button variant="ghost" size={28} onClick={() => onOpenTask(k.key)}>{t("Abrir")}</Button>
        </div>
      ))}
      {paused.length > MAX_PAUSED && (
        <div className="gn-bar gn-pause" role="status">{t("y {n} más con el autopiloto en pausa", { n: paused.length - MAX_PAUSED })}</div>
      )}
    </div>
  );
}
