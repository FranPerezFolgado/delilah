import type { Session, Step, Task } from "./api";

export const STEPS: Step[] = ["plan", "implement", "review"];

export type ChainState = "pending" | "active" | "done" | "waiting";

export interface ChainStep {
  step: Step;
  state: ChainState;
  /** The session that runs (or ran) this step, if any. */
  session?: Session;
  /** Why the handoff to this step failed. */
  error?: string;
}

/** The steps of a saving-mode task and where each stands; null without saving mode. */
export function chainOf(task: Task, sessions: Session[]): ChainStep[] | null {
  if (!task.saving_mode) return null;
  const h = task.handoff;
  return STEPS.map((step) => {
    const mine = sessions.filter((s) => s.step === step);
    const session = mine.find((s) => s.primary) ?? mine[0];
    if (session?.primary) return { step, state: "active", session };
    if (h && h.to === step && ["offered", "saving", "starting", "failed"].includes(h.stage)) {
      return { step, state: "waiting", session, error: h.stage === "failed" ? h.error : undefined };
    }
    return { step, state: session ? "done" : "pending", session };
  });
}
