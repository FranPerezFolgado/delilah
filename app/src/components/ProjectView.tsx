import type { Project, Session, SessionKind } from "../api";
import { keyOf } from "../shortcuts";
import { Icon } from "./Icon";
import { sessionStatus } from "./sessionLabels";
import { t } from "../i18n";

interface Props {
  project: Project;
  sessions: Session[];
  onOpenSession: (id?: string) => void;
  onNewSession: (kind: SessionKind) => Promise<void>;
  onNewTask?: () => void;
}

/** A project's first tab: its checkout, and sessions to ask about the code without a task. */
export function ProjectView({ project, sessions, onOpenSession, onNewSession, onNewTask }: Props) {
  return (
    <div className="view-body">
      <div className="muted" style={{ fontSize: 12 }}>
        {project.kind === "parent" ? t("Repo parent con submódulos") : "Repo"} · {t("tu checkout en")}{" "}
        <span className="mono selectable">{project.path}</span>
      </div>
      <div className="notice">
        <Icon name="console" />
        <span style={{ flex: 1 }}>
          {t("Pregúntale a Claude sobre el proyecto sin crear una tarea. Arranca en solo lectura sobre tu checkout: lee, busca y explica, pero no edita hasta que se lo permitas.")}
        </span>
      </div>
      <div style={{ display: "flex", gap: 8, flexWrap: "wrap" }}>
        <button className="btn btn-p" onClick={() => onNewSession("claude")}>
          <Icon name="plus" /> {t("Preguntar a Claude")} <span className="kbd">{keyOf("new-claude")}</span>
        </button>
        <button className="btn" onClick={() => onNewSession("shell")}>
          <Icon name="plus" /> {t("Terminal")} <span className="kbd">{keyOf("new-terminal")}</span>
        </button>
        {onNewTask && (
          <button className="btn" onClick={onNewTask}>
            <Icon name="tasks" /> {t("Nueva tarea")} <span className="kbd">{keyOf("new-task")}</span>
          </button>
        )}
      </div>
      {sessions.length > 0 && (
        <div>
          <div className="sec" style={{ marginBottom: 8 }}>{t("Sesiones")}</div>
          <div className="card history">
            {sessions.map((s) => {
              const st = sessionStatus(s, undefined);
              return (
                <div className="row" key={s.id} style={{ gridTemplateColumns: "1fr auto auto", alignItems: "center" }}>
                  <span style={{ display: "flex", alignItems: "center", gap: 8 }}>
                    <Icon name={s.kind === "shell" ? "console" : st.icon} className={st.cls} />
                    <b style={{ fontWeight: 600 }}>{s.label}</b>
                    <span className="muted">{s.kind === "shell" ? t("terminal") : s.read_only ? t("Claude · solo lectura") : "Claude"}</span>
                  </span>
                  <span className={`pill ${st.cls}`} style={{ height: 18, fontSize: 11 }}>{st.label}</span>
                  <button className="btn btn-ghost" onClick={() => onOpenSession(s.id)}>{t("Abrir")}</button>
                </div>
              );
            })}
          </div>
        </div>
      )}
    </div>
  );
}
