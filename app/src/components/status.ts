import type { TaskStatus } from "../api";
import type { IconName } from "./ui";

export const statusIcon = (s: TaskStatus): { icon: IconName; cls: string } => {
  switch (s.type) {
    case "waiting-for-user":
      return { icon: "wait", cls: "c-wait" };
    case "paused-by-limit":
      return { icon: "pause", cls: "c-pause" };
    case "done":
      return { icon: "done", cls: "c-done" };
    case "setup-failed":
    case "failed":
      return { icon: "failed", cls: "c-err" };
    case "queued":
      return { icon: "backlog", cls: "c-mut" };
    default:
      return { icon: "run", cls: "c-run" };
  }
};
