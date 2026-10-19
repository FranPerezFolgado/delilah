import type { Task } from "../api";
import { keyOf } from "../shortcuts";
import { Icon } from "./Icon";
import { statusIcon } from "./status";

export type View = "tasks" | "console" | "jira" | "projects" | "settings";

const GROUPS: { name: string; match: (t: Task) => boolean }[] = [
  { name: "En ejecución", match: (t) => ["queued", "preparing-workspace", "running"].includes(t.status.type) },
  { name: "Te esperan", match: (t) => t.status.type === "waiting-for-user" },
  { name: "Pausadas por límite", match: (t) => t.status.type === "paused-by-limit" },
  { name: "Hechas", match: (t) => t.status.type === "done" },
  { name: "Fallidas", match: (t) => t.status.type === "failed" },
];

interface Props {
  view: View;
  onView: (v: View) => void;
  tasks: Task[];
  selected: string | null;
  onSelect: (key: string) => void;
  onNewTask: () => void;
}

export function Sidebar({ view, onView, tasks, selected, onSelect, onNewTask }: Props) {
  const live = tasks.filter((t) => ["running", "waiting-for-user", "paused-by-limit"].includes(t.status.type)).length;
  return (
    <nav className="sidebar" aria-label="Navegación">
      <button className={`nav ${view === "tasks" ? "on" : ""}`} onClick={() => onView("tasks")}>
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
        <span className="sec">Tareas</span>
        <button className="btn btn-ghost" style={{ height: 22, padding: "0 6px" }} onClick={onNewTask}>
          <Icon name="plus" /> Nueva <span className="kbd">{keyOf("new-task")}</span>
        </button>
      </div>
      {GROUPS.map((g) => {
        const rows = tasks.filter(g.match);
        if (rows.length === 0) return null;
        const { icon, cls } = statusIcon(rows[0].status);
        return (
          <div key={g.name}>
            <div className="group-head">
              <Icon name={icon} className={cls} />
              <b>{g.name}</b>
              <span className="mono">{rows.length}</span>
            </div>
            {rows.map((t) => (
              <button
                key={t.key}
                className={`srow ${selected === t.key ? "on" : ""}`}
                onClick={() => onSelect(t.key)}
              >
                <span className="key mono">{t.key}</span>
                <span className="trunc">{t.title}</span>
              </button>
            ))}
          </div>
        );
      })}
      {tasks.length === 0 && <p className="muted" style={{ padding: "4px 8px", fontSize: 12 }}>Aún no hay tareas.</p>}
      <button className={`nav ${view === "settings" ? "on" : ""}`} style={{ marginTop: "auto" }} onClick={() => onView("settings")}>
        <Icon name="settings" /> Ajustes
      </button>
    </nav>
  );
}
