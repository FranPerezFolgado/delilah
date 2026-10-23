import { useEffect, useState } from "react";
import type { Project, PrSummary, Session, Task } from "../api";
import { sessionStatus } from "./sessionLabels";
import { keyOf } from "../shortcuts";
import { Icon } from "./Icon";
import { PrBadge } from "./PrBadge";
import { statusIcon } from "./status";
import { t, tn } from "../i18n";

export type View = "inbox" | "board" | "tasks" | "jira" | "projects" | "settings";


interface Props {
  view: View;
  onView: (v: View) => void;
  tasks: Task[];
  /** Each task's PR summary, for its badge. */
  prs?: PrSummary[];
  projects: Project[];
  sessions: Session[];
  /** A task's key or `@<project>`. */
  selected: string | null;
  onSelect: (key: string) => void;
  /** Shows an owner's screen on one of its session tabs. */
  onOpenSession: (owner: string, sessionId: string) => void;
  /** Starts a read-only Claude in the project's checkout. */
  onAsk: (project: string) => void;
  /** Starts an idea conversation in the project. */
  onIdea: (project: string) => void;
  /** Shows the project's screen on one of its idea conversations. */
  onOpenIdeas: (owner: string, ideas: Session[]) => void;
  onNewTask: () => void;
  /** Asks to confirm deleting a task (right-click on its row). */
  onDelete: (task: Task) => void;
  /** Items in the inbox that block an agent. */
  waiting: number;
}

/** Most urgent first; finished tasks stay on the board, not here, and backlog ones
 *  (`queued`) sit in their own folded section: they are not pending work. */
const ORDER = ["waiting-for-user", "paused-by-limit", "running", "preparing-workspace", "failed", "setup-failed"];

const COLLAPSED_KEY = "delilah.collapsed-projects";
/** Backlogs start folded: these are the ones the viewer opened. */
const OPEN_BACKLOGS_KEY = "delilah.open-backlogs";
function loadSet(key: string): Set<string> {
  try {
    return new Set(JSON.parse(localStorage.getItem(key) ?? "[]"));
  } catch {
    return new Set();
  }
}

function useRememberedSet(key: string) {
  const [set, setSet] = useState<Set<string>>(() => loadSet(key));
  const toggle = (id: string) =>
    setSet((c) => {
      const next = new Set(c);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      try {
        localStorage.setItem(key, JSON.stringify([...next]));
      } catch {
        // Not remembered: still folds for now.
      }
      return next;
    });
  return [set, toggle] as const;
}

export function Sidebar({ view, onView, tasks, prs = [], projects, sessions, selected, onSelect, onOpenSession, onAsk, onIdea, onOpenIdeas, onNewTask, onDelete, waiting }: Props) {
  // Folded projects, remembered per viewer: a project with many sessions and tasks
  // would otherwise take the whole sidebar.
  const [collapsed, toggle] = useRememberedSet(COLLAPSED_KEY);
  const [openBacklogs, toggleBacklog] = useRememberedSet(OPEN_BACKLOGS_KEY);
  // The right-click menu of a task row.
  const [menu, setMenu] = useState<{ task: Task; x: number; y: number } | null>(null);
  useEffect(() => {
    if (!menu) return;
    const close = () => setMenu(null);
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && close();
    window.addEventListener("mousedown", close);
    window.addEventListener("keydown", onKey);
    return () => {
      window.removeEventListener("mousedown", close);
      window.removeEventListener("keydown", onKey);
    };
  }, [menu]);
  const prOf = new Map(prs.map((p) => [p.task, p]));
  const taskRow = (task: Task) => {
    const { icon, cls } = statusIcon(task.status);
    return (
      <button
        key={task.key}
        className={`srow ${selected === task.key ? "on" : ""}`}
        onClick={() => onSelect(task.key)}
        onContextMenu={(e) => {
          e.preventDefault();
          setMenu({ task, x: e.clientX, y: e.clientY });
        }}
        title={task.title}
      >
        <Icon name={icon} className={cls} />
        <span className="key mono">{task.key}</span>
        <span className="trunc">{task.title}</span>
        {prOf.get(task.key) && <PrBadge pr={prOf.get(task.key)!} />}
      </button>
    );
  };
  // As in Orca: everything grouped by project, its own sessions first, then its tasks.
  const group = (project: Project | null) => {
    const owner = project ? `@${project.name}` : null;
    const mine = tasks.filter((t) => (t.project ?? null) === (project?.name ?? null));
    return {
      owner,
      project,
      sessions: owner ? sessions.filter((s) => s.task === owner) : [],
      tasks: mine
        .filter((t) => t.status.type !== "done" && t.status.type !== "queued")
        .sort((a, b) => ORDER.indexOf(a.status.type) - ORDER.indexOf(b.status.type)),
      // Oldest first: the daemon lists tasks in the order they were created.
      backlog: mine.filter((t) => t.status.type === "queued"),
      done: mine.filter((t) => t.status.type === "done").length,
    };
  };
  const groups = [
    ...projects.map(group),
    ...(tasks.some((t) => !t.project) ? [group(null)] : []),
  ];
  return (
    <nav className="sidebar" aria-label={t("Navegación")}>
      <button className={`nav ${view === "inbox" ? "on" : ""}`} onClick={() => onView("inbox")}>
        <Icon name="inbox" /> {t("Bandeja")}
        {waiting > 0 ? <span className="count badge mono">{waiting}</span> : <span className="count mono" />}
        <span className="kbd">{keyOf("view-inbox")}</span>
      </button>
      <button className={`nav ${view === "board" ? "on" : ""}`} onClick={() => onView("board")}>
        <Icon name="tasks" /> {t("Tareas")} <span className="count mono">{tasks.length}</span>
        <span className="kbd">{keyOf("view-tasks")}</span>
      </button>
      <button className={`nav ${view === "jira" ? "on" : ""}`} onClick={() => onView("jira")}>
        <Icon name="ticket" /> Jira <span className="kbd" style={{ marginLeft: "auto" }}>{keyOf("view-jira")}</span>
      </button>
      <button className={`nav ${view === "projects" ? "on" : ""}`} onClick={() => onView("projects")}>
        <Icon name="folder" /> {t("Proyectos")} <span className="kbd" style={{ marginLeft: "auto" }}>{keyOf("view-projects")}</span>
      </button>

      <div style={{ display: "flex", alignItems: "center", justifyContent: "space-between", padding: "16px 8px 4px" }}>
        <span className="sec">{t("Proyectos")}</span>
        <button className="btn btn-ghost" style={{ height: 22, padding: "0 6px" }} onClick={onNewTask}>
          <Icon name="plus" /> {t("Tarea")} <span className="kbd">{keyOf("new-task")}</span>
        </button>
      </div>
      {groups.map((g) => {
        const id = g.owner ?? "none";
        const folded = collapsed.has(id);
        const count = g.sessions.length + g.tasks.length;
        const ideas = g.sessions.filter((s) => s.conversation);
        const others = g.sessions.filter((s) => !s.conversation);
        const ideasLabel = g.project ? t("Ideas de {name} ({n})", { name: g.project.name, n: ideas.length }) : "";
        // Folded, it still says when something in it waits on you.
        const waitingHere =
          g.tasks.some((t) => t.status.type === "waiting-for-user") || g.sessions.some((s) => s.state === "waiting");
        const chevron = (
          <button
            className="chev-btn"
            onClick={() => toggle(id)}
            aria-expanded={!folded}
            aria-label={folded ? t("Desplegar") : t("Plegar")}
          >
            <span className={`chev ${folded ? "" : "open"}`}>›</span>
          </button>
        );
        return (
        <div key={id} className="pgroup">
          {g.project ? (
            <div className={`phead ${selected === g.owner ? "on" : ""}`}>
              {chevron}
              <button className="phead-main" onClick={() => onSelect(g.owner!)} title={g.project.path}>
                <Icon name="folder" />
                <span className="trunc">{g.project.name}</span>
                {folded && count > 0 && <span className={`pcount mono ${waitingHere ? "c-wait" : ""}`}>{count}</span>}
              </button>
              <button className="btn btn-ghost icon-btn" onClick={() => onAsk(g.project!.name)} title={t("Preguntar a Claude sobre el proyecto")} aria-label={t("Preguntar a Claude sobre {name}", { name: g.project.name })}>
                <Icon name="plus" />
              </button>
              <button className="btn btn-ghost icon-btn" onClick={() => onIdea(g.project!.name)} title={t("Nueva tarea con Claude")} aria-label={t("Nueva tarea con Claude en {name}", { name: g.project.name })}>
                <Icon name="idea" />
              </button>
            </div>
          ) : (
            <div className="phead">
              {chevron}
              <button className="phead-main" onClick={() => toggle(id)} style={{ fontWeight: 500, color: "var(--tx3)" }}>
                {t("Sin proyecto")}
                {folded && count > 0 && <span className="pcount mono">{count}</span>}
              </button>
            </div>
          )}
          {!folded && ideas.length > 0 && (
            <button className="srow ideas" onClick={() => onOpenIdeas(g.owner!, ideas)} title={ideasLabel} aria-label={ideasLabel}>
              <Icon name="idea" className="idea" />
              <span className="trunc" style={{ flex: 1 }}>{t("Ideas")}</span>
              <span className="pcount mono">{ideas.length}</span>
            </button>
          )}
          {!folded && others.map((s) => {
            const st = sessionStatus(s, undefined);
            return (
              <button key={s.id} className="srow" onClick={() => onOpenSession(s.task, s.id)} title={s.read_only ? t("Claude en solo lectura") : s.label}>
                <Icon name={s.kind !== "claude" ? "console" : st.icon} className={s.kind !== "claude" ? "c-mut" : st.cls} />
                <span className="trunc" style={{ flex: 1 }}>{s.label}</span>
                {s.read_only && <Icon name="lock" className="c-mut" />}
              </button>
            );
          })}
          {!folded && g.tasks.map(taskRow)}
          {!folded && g.backlog.length > 0 && (
            <>
              <button className="bhead" onClick={() => toggleBacklog(id)} aria-expanded={openBacklogs.has(id)}>
                <span className={`chev ${openBacklogs.has(id) ? "open" : ""}`}>›</span>
                {t("Backlog ({n})", { n: g.backlog.length })}
              </button>
              {openBacklogs.has(id) && g.backlog.map(taskRow)}
            </>
          )}
          {!folded && g.done > 0 && <div className="pdone muted">{tn(g.done, "{n} hecha · en Tareas", "{n} hechas · en Tareas")}</div>}
        </div>
        );
      })}
      {projects.length === 0 && tasks.length === 0 && (
        <p className="muted" style={{ padding: "4px 8px", fontSize: 12 }}>{t("Añade un proyecto en Proyectos.")}</p>
      )}
      {menu && (
        <div className="menu ctx" style={{ left: menu.x, top: menu.y }} onMouseDown={(e) => e.stopPropagation()}>
          <button
            className="c-err"
            onClick={() => {
              onDelete(menu.task);
              setMenu(null);
            }}
          >
            <span>
              <Icon name="trash" /> {t("Eliminar {key}…", { key: menu.task.key })}
            </span>
          </button>
        </div>
      )}
      <button className={`nav ${view === "settings" ? "on" : ""}`} style={{ marginTop: "auto" }} onClick={() => onView("settings")}>
        <Icon name="settings" /> {t("Ajustes")}
      </button>
    </nav>
  );
}
