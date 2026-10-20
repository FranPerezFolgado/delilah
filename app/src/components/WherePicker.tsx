import type { Project } from "../api";
import { t } from "../i18n";

export type Where = "checkout" | "worktree";

/** A backlog task's saved choice; without one, a worktree, as the daemon starts it. */
export const whereOf = (inCheckout?: boolean | null): Where => (inCheckout === true ? "checkout" : "worktree");

/** Where the agent works: a branch in the project's own checkout, or a worktree. */
export function WherePicker({ value, onChange, project }: { value: Where; onChange: (w: Where) => void; project?: Project }) {
  return (
    <div className="field">
      <span>{t("Dónde trabaja el agente")}</span>
      <div className="where">
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
