import type { Project } from "../api";
import { t } from "../i18n";

export type Where = "checkout" | "worktree";

/**
 * A task's saved choice; without one, the project's setting; without that, `fallback`.
 * The daemon runs a worktree then, so that is the default (what a backlog task will do);
 * the new-task form passes "checkout" to preselect it, as it always did.
 */
export const whereOf = (inCheckout?: boolean | null, project?: Project, fallback: Where = "worktree"): Where =>
  inCheckout === true ? "checkout" : inCheckout === false ? "worktree" : (project?.settings.workspace ?? fallback);

/** Where the agent works: a branch in the project's own checkout, or a worktree. */
export function WherePicker({ value, onChange, project, withDefault }: {
  value: Where | null;
  onChange: (w: Where | null) => void;
  project?: Project;
  /** The project settings: a first option for "unset" (value null), which means a worktree. */
  withDefault?: boolean;
}) {
  return (
    <div className="field">
      <span>{t("Dónde trabaja el agente")}</span>
      <div className="where">
        {withDefault && (
          <label className={`where-opt ${value === null ? "on" : ""}`}>
            <input type="radio" name="where" value="default" checked={value === null} onChange={() => onChange(null)} />
            <span>
              <b>{t("Por defecto (aislada)")}</b>
              <span className="muted">{t("Sin elegir: cada tarea trabaja en un worktree, salvo que se elija otra cosa para ella.")}</span>
            </span>
          </label>
        )}
        <label className={`where-opt ${value === "checkout" ? "on" : ""}`}>
          <input type="radio" name="where" checked={value === "checkout"} onChange={() => onChange("checkout")} />
          <span>
            <b>{t("En tu checkout")}</b>
            <span className="muted">
              {t("Crea la rama en {path}", { path: project?.path ?? t("tu carpeta") })}
              {project?.kind === "parent" ? t(" (en los submódulos elegidos)") : ""}
              {t(" y trabaja ahí. Instantáneo; ves los cambios en tu IDE. Una tarea a la vez, con el checkout limpio.")}
            </span>
          </span>
        </label>
        <label className={`where-opt ${value === "worktree" ? "on" : ""}`}>
          <input type="radio" name="where" checked={value === "worktree"} onChange={() => onChange("worktree")} />
          <span>
            <b>{t("Aislada (worktree)")}</b>
            <span className="muted">{t("Copia aparte. Varias tareas en paralelo sin tocar tu carpeta; la primera vez clona los submódulos.")}</span>
          </span>
        </label>
      </div>
    </div>
  );
}
