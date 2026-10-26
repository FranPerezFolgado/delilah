import type { ProjectSettings, ProjectWorkflow } from "./api";
import { lang, t } from "./i18n";

export interface WorkflowPreset {
  id: "builtin" | "nothing" | "speckit-feature" | "speckit-official";
  /** Shown name, in the app's language. */
  name: string;
  workflow: ProjectWorkflow;
}

/**
 * The review message of today's `/feature` flow; it must equal the daemon's
 * `feature_preset` (crates/delilah-daemon/src/lang.rs) in both languages; a test there checks it.
 */
const REVIEW_ES =
  "/review de la rama de esta tarea. Después aplica los hallazgos confirmados, actualiza la documentación que hayan dejado obsoleta y haz commit. No hagas push ni abras una PR.";
const REVIEW_EN =
  "/review the branch of this task. Then apply the confirmed findings, update the docs they made stale and commit. Do not push or open a PR.";

/** The fixed presets of the settings screen (research R6); applying one fills the fields. */
export function workflowPresets(): WorkflowPreset[] {
  const review = lang() === "en" ? REVIEW_EN : REVIEW_ES;
  return [
    { id: "builtin", name: t("Prompt integrado"), workflow: { kind: "single", start: { type: "builtin" } } },
    { id: "nothing", name: t("Nada"), workflow: { kind: "single", start: { type: "nothing" } } },
    {
      id: "speckit-feature",
      name: t("Spec Kit (`/feature`)"),
      workflow: { kind: "phased", plan: "/feature {task}", implement: "/orchestrate {spec_dir}", review },
    },
    {
      id: "speckit-official",
      name: t("Spec Kit (oficial)"),
      workflow: { kind: "phased", plan: "/speckit-specify {title}", implement: "/speckit-implement", review },
    },
  ];
}

/**
 * Whether a task runs in phases: its workflow is the project's (`project`, or the legacy
 * `feature`) and the project's workflow is phased. No project workflow means the
 * built-in prompt. Mirrors core `workflow::is_phased`.
 */
export const isPhased = (taskWorkflow: string, settings?: Pick<ProjectSettings, "workflow"> | null): boolean =>
  (taskWorkflow === "project" || taskWorkflow === "feature") && settings?.workflow?.kind === "phased";
