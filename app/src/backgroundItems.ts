import type { BackgroundInfo } from "./api";

export type Outcome = "ok" | "failed" | "stopped" | "other";

/** How a finished background task ended, from its reported status and exit code. */
export function outcome(info: BackgroundInfo): Outcome {
  const s = info.status;
  if (s === "failed") return "failed";
  if (s === "stopped" || s === "killed") return "stopped";
  if (s === "completed") return info.exit_code != null && info.exit_code !== 0 ? "failed" : "ok";
  if (s == null && info.exit_code != null) return info.exit_code === 0 ? "ok" : "failed";
  return "other";
}
