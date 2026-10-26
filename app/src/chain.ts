import type { Handoff, Session, Step, Task } from "./api";
import { t } from "./i18n";

export const STEPS: Step[] = ["plan", "implement", "review"];

const STEP_LABELS: Record<Step, string> = {
  plan: "Spec y plan",
  implement: "Implementación",
  review: "Revisión",
};

/** The chain's steps: the three agent steps plus, with autopilot, the preview and the PR. */
export type ChainStepName = Step | "preview" | "pr";

const EXTRA_LABELS = { preview: "Preview", pr: "PR" };

export const stepLabel = (step: ChainStepName) =>
  t((STEP_LABELS as Record<string, string>)[step] ?? (EXTRA_LABELS as Record<string, string>)[step] ?? String(step));

/** The name a session shows in the UI: its step's, else "Claude principal" for the primary claude, else its label. */
export const sessionName = (s: Session): string =>
  s.step ? stepLabel(s.step) : s.primary && s.kind === "claude" ? t("Claude principal") : s.label;

/** The handoff stages in which its target step is still waiting for the new session. */
const WAITING: Handoff["stage"][] = ["offered", "saving", "starting", "failed"];

export type ChainState = "pending" | "active" | "done" | "waiting";

export interface ChainStep {
  step: ChainStepName;
  state: ChainState;
  /** The session that runs (or ran) this step, if any. */
  session?: Session;
  /** Why the handoff to this step failed. */
  error?: string;
}

/** What the chain cannot read from the task itself. */
export interface ChainOpts {
  /** The task's project has a preview command. */
  hasPreview?: boolean;
  /** A PR is linked to the task. */
  prLinked?: boolean;
}

/** The steps of a saving-mode task and where each stands; null without saving mode.
 *  With autopilot on it adds the preview (if the project has a command) and the PR. */
export function chainOf(task: Task, sessions: Session[], opts: ChainOpts = {}): ChainStep[] | null {
  if (!task.saving_mode) return null;
  const h = task.handoff;
  const ap = task.autopilot;
  const steps: ChainStep[] = STEPS.map((step) => {
    const mine = sessions.filter((s) => s.step === step);
    const session = mine.find((s) => s.primary) ?? mine[0];
    // Once autopilot is past the agent steps, they are all behind it.
    if (session?.primary) return { step, state: ap?.stage ? "done" : "active", session };
    if (h && h.to === step && WAITING.includes(h.stage)) {
      return { step, state: "waiting", session, error: h.stage === "failed" ? h.error : undefined };
    }
    return { step, state: session ? "done" : "pending", session };
  });
  if (!ap) return steps;

  const finished = ap.state === "done" || !!opts.prLinked;
  if (opts.hasPreview) {
    const state: ChainState = ap.stage === "preview" ? "active" : ap.stage === "opening_pr" || finished ? "done" : "pending";
    steps.push({ step: "preview", state });
  }
  steps.push({ step: "pr", state: finished ? "done" : ap.stage === "opening_pr" ? "active" : "pending" });

  if (ap.state === "paused") {
    const current =
      (ap.stage && steps.find((c) => c.step === (ap.stage === "preview" ? "preview" : "pr"))) ||
      steps.find((c) => c.state === "active") ||
      steps.find((c) => c.state === "waiting") ||
      // Nothing active, waiting or staged: the last unfinished step carries the banner.
      [...steps].reverse().find((c) => c.state !== "done");
    if (current && current.state !== "done") {
      current.state = "waiting";
      current.error = ap.reason ?? current.error;
    }
  }
  return steps;
}
