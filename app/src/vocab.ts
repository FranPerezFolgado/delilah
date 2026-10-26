// The one vocabulary for task status, inbox item kinds, PR state and session dots
// (docs/archive/ui-redesign.md §3). Labels are Spanish `t()` keys; the kit renders what these return.
import { BLOCKING, type InboxKind, type PullRequest, type Session, type TaskStatus } from "./api";
import { locale, t } from "./i18n";

export type StatusTone = "neutral" | "run" | "wait" | "pause" | "ok" | "err";
export interface StatusView { tone: StatusTone; label: string; detail?: string }

export function statusOf(status: TaskStatus | "backlog"): StatusView {
  if (status === "backlog") return { tone: "neutral", label: t("Backlog") };
  switch (status.type) {
    case "queued":
      return { tone: "neutral", label: t("Backlog") };
    case "preparing-workspace":
      return { tone: "run", label: t("Trabajando"), detail: t("Preparando el workspace") };
    case "running":
      return { tone: "run", label: t("Trabajando") };
    case "waiting-for-user":
      return { tone: "wait", label: t("Te espera") };
    case "paused-by-limit": {
      const at = new Date(status.resume_at);
      const time = isNaN(at.getTime())
        ? ""
        : at.toLocaleTimeString(locale(), { hour: "2-digit", minute: "2-digit", hour12: false });
      return { tone: "pause", label: t("En pausa"), detail: time ? t("se reanuda a las {time}", { time }) : undefined };
    }
    case "done":
      return { tone: "ok", label: t("Hecha") };
    case "failed":
    case "setup-failed":
      return { tone: "err", label: t("Error"), detail: status.reason };
  }
}

export interface KindView { kind: "Pregunta" | "Permiso" | "Aviso"; blocking: boolean; clearable: boolean }

/** `kind` is the Spanish name (a stable key); show it with `kindLabel`. */
export function kindOf(k: InboxKind): KindView {
  const blocking = BLOCKING.includes(k);
  const kind: KindView["kind"] = k === "permission" ? "Permiso" : k === "question" || k === "waiting" ? "Pregunta" : "Aviso";
  return { kind, blocking, clearable: !blocking };
}

export const kindLabel = (k: KindView["kind"]): string =>
  k === "Permiso" ? t("Permiso") : k === "Pregunta" ? t("Pregunta") : t("Aviso");

export type PrState = "draft" | "open" | "failing" | "merged";

export function prStateOf(pr: Pick<PullRequest, "snapshot">): { state: PrState; label: string } {
  const s = pr.snapshot;
  const state: PrState = s?.state === "merged" ? "merged" : s?.draft ? "draft" : s?.ci === "failure" ? "failing" : "open";
  const label = { draft: t("Borrador"), open: t("Abierta"), failing: t("Checks fallan"), merged: t("Fusionada") }[state];
  return { state, label };
}

export type SessionDot = "wait" | "run" | "neutral";

export function sessionDot(s: Pick<Session, "state" | "live">): SessionDot {
  if (!s.live) return "neutral";
  if (s.state === "waiting") return "wait";
  return s.state === "working" || s.state === "starting" ? "run" : "neutral";
}
