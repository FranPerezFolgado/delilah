import type { Project, Session, Task } from "../api";
import { sessionStatus } from "./sessionLabels";
import { keyOf } from "../shortcuts";
import { Icon } from "./Icon";
import { statusIcon } from "./status";

export type View = "inbox" | "board" | "tasks" | "console" | "jira" | "projects" | "settings";


interface Props {
  view: View;
  onView: (v: View) => void;
  tasks: Task[];
  projects: Project[];
  sessions: Session[];
  /** A task's key or `@<project>`. */
  selected: string | null;
  onSelect: (key: string) => void;
  /** Shows an owner's screen on one of its session tabs. */
  onOpenSession: (owner: string, sessionId: string) => void;
  /** Starts a read-only Claude in the project's checkout. */
  onAsk: (project: string) => void;
  onNewTask: () => void;
  /** Items in the inbox that block an agent. */
  waiting: number;
}

/** Most urgent first; finished tasks stay on the board, not here. */
const ORDER = ["waiting-for-user", "paused-by-limit", "running", "preparing-workspace", "queued", "failed"];

export function Sidebar({ view, onView, tasks, projects, sessions, selected, onSelect, onOpenSession, onAsk, onNewTask, waiting }: Props) {
  // As in Orca: everything grouped by project, its own sessions first, then its tasks.
  const group = (project: Project | null) => {
    const owner = project ? `@${project.name}` : null;
    const mine = tasks.filter((t) => (t.project ?? null) === (project?.name ?? null));
    return {
      owner,
      project,
      sessions: owner ? sessions.filter((s) => s.task === owner) : [],
      tasks: mine
        .filter((t) => t.status.type !== "done")
        .sort((a, b) => ORDER.indexOf(a.status.type) - ORDER.indexOf(b.status.type)),
      done: mine.filter((t) => t.status.type === "done").length,
    };
  };
  const groups = [
    ...projects.map(group),
    ...(tasks.some((t) => !t.project) ? [group(null)] : []),
  ];
  const live = tasks.filter((t) => ["running", "waiting-for-user", "paused-by-limit"].includes(t.status.type)).length;
  return (
    <nav className="sidebar" aria-label="Navegación">
      <button className={`nav ${view === "inbox" ? "on" : ""}`} onClick={() => onView("inbox")}>
        <Icon name="inbox" /> Bandeja
        {waiting > 0 ? <span className="count badge mono">{waiting}</span> : <span className="count mono" />}
        <span className="kbd">{keyOf("view-inbox")}</span>
      </button>
      <button className={`nav ${view === "board" ? "on" : ""}`} onClick={() => onView("board")}>
        <Icon name="tasks" /> Tareas <span className="count mono">{tasks.length}</span>
        <span className="kbd">{keyOf("view-tasks")}</span>
      </button>
      <button className={`nav ${view === "console" ? "on" : ""}`} onClick={() => onView("console")}>
        <Icon name="console" /> Consola <span className="count mono">{live}</span>
        <span className="kbd">{keyOf("view-console")}</span>
      </button>
      <button className={`nav ${view === "jira" ? "on" : ""}`} onClick={() => onView("jira")}>
        <Icon name="ticket" /> Jira <span className="kbd" style={{ marginLeft: "auto" }}>{keyOf("view-jira")}</span>
      </button>
      <button className={`nav ${view === "projects" ? "on" : ""}`} onClick={() => onView("projects")}>
        <Icon name="folder" /> Proyectos <span className="kbd" style={{ marginLeft: "auto" }}>{keyOf("view-projects")}</span>
      </button>

      <div style={{ display: "flex", alignItems: "center", justifyContent: "space-between", padding: "16px 8px 4px" }}>
        <span className="sec">Proyectos</span>
        <button className="btn btn-ghost" style={{ height: 22, padding: "0 6px" }} onClick={onNewTask}>
          <Icon name="plus" /> Tarea <span className="kbd">{keyOf("new-task")}</span>
        </button>
      </div>
      {groups.map((g) => (
        <div key={g.owner ?? "none"} className="pgroup">
          {g.project ? (
            <div className={`phead ${selected === g.owner ? "on" : ""}`}>
              <button className="phead-main" onClick={() => onSelect(g.owner!)} title={g.project.path}>
                <Icon name="folder" />
                <span className="trunc">{g.project.name}</span>
              </button>
              <button className="btn btn-ghost icon-btn" onClick={() => onAsk(g.project!.name)} title="Preguntar a Claude sobre el proyecto" aria-label={`Preguntar a Claude sobre ${g.project.name}`}>
                <Icon name="plus" />
              </button>
            </div>
          ) : (
            <div className="group-head"><b>Sin proyecto</b></div>
          )}
          {g.sessions.map((s) => {
            const st = sessionStatus(s, undefined);
            return (
              <button key={s.id} className="srow" onClick={() => onOpenSession(s.task, s.id)} title={s.read_only ? "Claude en solo lectura" : s.label}>
                <Icon name={s.kind === "shell" ? "console" : st.icon} className={s.kind === "shell" ? "c-mut" : st.cls} />
                <span className="trunc" style={{ flex: 1 }}>{s.label}</span>
                {s.read_only && <Icon name="lock" className="c-mut" />}
              </button>
            );
          })}
          {g.tasks.map((t) => {
            const { icon, cls } = statusIcon(t.status);
            return (
              <button key={t.key} className={`srow ${selected === t.key ? "on" : ""}`} onClick={() => onSelect(t.key)} title={t.title}>
                <Icon name={icon} className={cls} />
                <span className="key mono">{t.key}</span>
                <span className="trunc">{t.title}</span>
              </button>
            );
          })}
          {g.done > 0 && <div className="pdone muted">{g.done} {g.done === 1 ? "hecha" : "hechas"} · en Tareas</div>}
        </div>
      ))}
      {projects.length === 0 && tasks.length === 0 && (
        <p className="muted" style={{ padding: "4px 8px", fontSize: 12 }}>Añade un proyecto en Proyectos.</p>
      )}
      <button className={`nav ${view === "settings" ? "on" : ""}`} style={{ marginTop: "auto" }} onClick={() => onView("settings")}>
        <Icon name="settings" /> Ajustes
      </button>
    </nav>
  );
}
