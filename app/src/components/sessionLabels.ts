import { statusLabel, type Session, type Task } from "../api";
import type { IconName } from "./Icon";
import { statusIcon } from "./status";
import { t } from "../i18n";

/** What a session's header shows: the primary one carries the task's status. */
export function sessionStatus(s: Session, task: Task | undefined): { label: string; icon: IconName; cls: string } {
  if (!s.live) return { label: t("Parada"), icon: "pause", cls: "c-mut" };
  if (s.kind === "setup") return { label: t("Setup"), icon: "console", cls: "c-mut" };
  if (s.kind === "shell") return { label: t("Terminal"), icon: "console", cls: "c-mut" };
  if (s.primary && task) return { label: statusLabel(task.status), ...statusIcon(task.status) };
  switch (s.state) {
    case "waiting":
      return { label: t("Te espera"), icon: "wait", cls: "c-wait" };
    case "paused":
      return { label: t("Pausada"), icon: "pause", cls: "c-pause" };
    case "starting":
      return { label: t("Arrancando"), icon: "run", cls: "c-run" };
    default:
      return { label: t("Trabajando"), icon: "run", cls: "c-run" };
  }
}
