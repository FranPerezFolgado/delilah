import { statusLabel, type Session, type Task } from "../api";
import type { IconName } from "./Icon";
import { statusIcon } from "./status";

/** What a session's header shows: the primary one carries the task's status. */
export function sessionStatus(s: Session, task: Task | undefined): { label: string; icon: IconName; cls: string } {
  if (!s.live) return { label: "Parada", icon: "pause", cls: "c-mut" };
  if (s.kind === "shell") return { label: "Terminal", icon: "console", cls: "c-mut" };
  if (s.primary && task) return { label: statusLabel(task.status), ...statusIcon(task.status) };
  switch (s.state) {
    case "waiting":
      return { label: "Te espera", icon: "wait", cls: "c-wait" };
    case "paused":
      return { label: "Pausada", icon: "pause", cls: "c-pause" };
    case "starting":
      return { label: "Arrancando", icon: "run", cls: "c-run" };
    default:
      return { label: "Trabajando", icon: "run", cls: "c-run" };
  }
}
