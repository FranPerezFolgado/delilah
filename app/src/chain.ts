import type { Handoff, Session, Step, Task } from "./api";
import { t } from "./i18n";

export const STEPS: Step[] = ["plan", "implement", "review"];

const STEP_LABELS: Record<Step, string> = {
  plan: "Spec y plan",
  implement: "Implementación",
  review: "Revisión",
};

export const stepLabel = (step: Step) => t(STEP_LABELS[step] ?? String(step));

/** The handoff stages in which its target step is still waiting for the new session. */
const WAITING: Handoff["stage"][] = ["offered", "saving", "starting", "failed"];

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
    if (h && h.to === step && WAITING.includes(h.stage)) {
      return { step, state: "waiting", session, error: h.stage === "failed" ? h.error : undefined };
    }
    return { step, state: session ? "done" : "pending", session };
  });
}
