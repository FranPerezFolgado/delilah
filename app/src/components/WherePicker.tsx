import type { Project } from "../api";
import { t } from "../i18n";
import { OptionCard } from "./ui/OptionCard";
import "./WherePicker.css";

export type Where = "checkout" | "worktree";

/**
 * A task's saved choice; without one, the project's setting; without that, `fallback`.
 * The daemon runs a worktree then, so that is the default (what a backlog task will do);
 * the new-task form passes "checkout" to preselect it, as it always did.
 */
export const whereOf = (inCheckout?: boolean | null, project?: Project, fallback: Where = "worktree"): Where =>
  inCheckout === true ? "checkout" : inCheckout === false ? "worktree" : (project?.settings.workspace ?? fallback);

/** Where the agent works: a branch in the project's own checkout (the default), or a worktree. */
export function WherePicker({ value, onChange, project, withDefault }: {
  value: Where | null;
  onChange: (w: Where | null) => void;
  project?: Project;
  /** The project settings: an unset value (null) means the checkout, the default. */
  withDefault?: boolean;
}) {
  const current: Where = value ?? "checkout";
  const projectWhere = project?.settings.workspace ?? null;
  // Which card carries the "default" mark: the project's own setting, else the checkout.
  const defaultWhere: Where = withDefault ? "checkout" : (projectWhere ?? "checkout");
  const mark = (w: Where): string =>
    w !== defaultWhere ? "" : t(withDefault || !projectWhere ? " · por defecto" : " · por defecto del proyecto");
  return (
    <div className="field where-picker">
      <span>{t("Dónde trabaja el agente")}</span>
      <div className="where" role="radiogroup" aria-label={t("Dónde trabaja el agente")}>
        <OptionCard
          name="where"
          checked={current === "checkout"}
          onClick={() => onChange(withDefault ? null : "checkout")}
          title={t("En tu checkout") + mark("checkout")}
          description={
            t("Crea la rama en {path}", { path: project?.path ?? t("tu carpeta") }) +
            (project?.kind === "parent" ? t(" (en los submódulos elegidos)") : "") +
            t(" y trabaja ahí. Instantáneo; ves los cambios en tu IDE. Una tarea a la vez, con el checkout limpio.")
          }
        />
        <OptionCard
          name="where"
          checked={current === "worktree"}
          onClick={() => onChange("worktree")}
          title={t("Aislado (worktree)") + mark("worktree")}
          description={t("Copia aparte. Varias tareas en paralelo sin tocar tu carpeta; la primera vez clona los submódulos.")}
        />
      </div>
    </div>
  );
}
