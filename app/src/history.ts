// The Historial of a task: every event the daemon records, as a human sentence (R22).
import { hhmm, type EventRecord } from "./api";
import { stepLabel } from "./chain";
import { t } from "./i18n";
import { phaseLabel } from "./phases";
import { relativeTime } from "./time";

type Ev = EventRecord["event"];

const str = (v: unknown) => (typeof v === "string" ? v : "");
const short = (s: string, n = 70) => (s.length > n ? `${s.slice(0, n - 1)}…` : s);

const AUTOFIX_STOP: Record<string, string> = {
  limit: "alcanzó el límite de intentos",
  "no-push": "el agente no subió ningún cambio",
  "no-session": "no hay sesión del agente",
};

// Attempts per commit; events carry no limit (the daemon's autofix LIMIT).
const AUTOFIX_LIMIT = 3;

/** Adds the repo to an autofix line when the PR isn't in the root repo (multi-repo parents). */
const inRepo = (text: string, ev: Ev) => {
  const repo = str(ev.repo);
  return repo && repo !== "." ? `${text} · ${repo}` : text;
};

function expired(outcome: string): string {
  if (outcome === "terminal") return t("se respondió en la terminal");
  if (outcome === "withdrawn") return t("se retiró");
  return t("sin respuesta");
}

/** One sentence for an event. An event this app does not know gets a generic sentence,
 *  never its slug. */
export function historyLine(ev: Ev): string {
  switch (ev.type) {
    case "start": return t("Tarea creada");
    case "workspace-ready": return t("Workspace listo");
    case "phase-reached": return t("Pasó a la fase {phase}", { phase: phaseLabel(str(ev.phase) as never) });
    case "agent-asked": return t("Claude te espera");
    case "user-answered": return t("Respondiste");
    case "limit-reached":
      return str(ev.resume_at)
        ? t("Límite de uso alcanzado · se reanuda a las {time}", { time: hhmm(str(ev.resume_at)) })
        : t("Límite de uso alcanzado");
    case "reset-due": return t("Reanudada tras el reset");
    case "manual-resume": return t("Reanudada a mano");
    case "agent-finished": return t("Claude terminó");
    case "mark-done": return t("Marcada como hecha");
    case "reopened": return t("Reabriste la tarea");
    case "fail": return str(ev.reason) ? t("Falló: {reason}", { reason: short(str(ev.reason)) }) : t("Falló");
    case "start-aborted": return t("No se pudo empezar; vuelve al backlog");
    case "setup-failed": return t("Falló la preparación del workspace");
    case "retry-setup": return t("Repetiste la preparación del workspace");
    case "skip-setup": return t("Saltaste la preparación del workspace");
    case "created-from-conversation": return t("Creada desde la conversación «{label}»", { label: str(ev.label) });
    case "step-started": return t("Paso a {step} (sesión nueva)", { step: stepLabel(str(ev.step) as never) });
    case "autopilot-paused": return t("Autopiloto en pausa");
    case "autopilot-resumed": return t("Autopiloto reanudado");
    case "autopilot-answered": return t("Autopiloto respondió");
    case "autopilot-nudged": return t("Autopiloto empujó el paso");
    case "autopilot-finished": return t("Autopiloto terminó");
    case "runtime-switched":
      return str(ev.mode) === "chat" || str(ev.mode) === "stream" ? t("La sesión pasó a chat") : t("La sesión pasó a la terminal");
    case "permission-asked": return t("Claude pidió permiso para {tool}: {summary}", { tool: str(ev.tool), summary: short(str(ev.summary)) });
    case "permission-expired": return t("Caducó el permiso de {tool} · {outcome}", { tool: str(ev.tool), outcome: expired(str(ev.outcome)) });
    case "autofix-sent": {
      const attempt = t("Autofix de CI: intento {n} de {limit}", { n: Number(ev.attempt) || 1, limit: AUTOFIX_LIMIT });
      const checks = Array.isArray(ev.checks) ? ev.checks.map(str).filter(Boolean) : [];
      return inRepo(checks.length ? `${attempt} — ${short(checks.join(", "))}` : attempt, ev);
    }
    case "autofix-fixed": return inRepo(t("Autofix de CI: CI arreglado"), ev);
    case "autofix-failed-again": return inRepo(t("Autofix de CI: el CI sigue fallando"), ev);
    case "autofix-stopped": {
      const why = AUTOFIX_STOP[str(ev.reason)];
      return inRepo(why ? t("Autofix de CI parado: {reason}", { reason: t(why) }) : t("Autofix de CI parado"), ev);
    }
    case "autofix-cancelled": return inRepo(t("Autofix de CI cancelado"), ev);
    case "agent-resumed":
      return str(ev.reason) === "background-done" ? t("Claude continúa: tarea en segundo plano terminada") : t("Claude continúa");
    default: return t("Pasó algo en la tarea");
  }
}

export interface HistoryEntry { when: string; text: string }

/** A row of the Historial: relative time plus the sentence. */
export function historyEntry(e: EventRecord, now: Date | number = Date.now()): HistoryEntry {
  return { when: relativeTime(e.at, now), text: historyLine(e.event) };
}
