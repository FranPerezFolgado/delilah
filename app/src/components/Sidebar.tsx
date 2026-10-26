import { useState } from "react";
import type { Project, PrSummary, Session, Task } from "../api";
import { keyOf, type ActionId } from "../shortcuts";
import { ConfirmDialog, Count, Icon, IconButton, type IconName, Menu, Row } from "./ui";
import { PrBadge } from "./PrBadge";
import { sessionDot, statusOf } from "../vocab";
import { t, tn } from "../i18n";
import "./Sidebar.css";

export type View = "inbox" | "board" | "tasks" | "jira" | "projects" | "playground" | "settings";

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
  /** Starts an idea conversation in the project («Nueva tarea en este proyecto»). */
  onIdea: (project: string) => void;
  /** Shows the project's screen on one of its idea conversations. */
  onOpenIdeas: (owner: string, ideas: Session[]) => void;
  /** No longer used: «+ Tarea» left the sidebar (the board and ⌘N create tasks). */
  onNewTask?: () => void;
  /** «Añadir proyecto»; without it the button opens the Proyectos view. */
  onAddProject?: () => void;
  /** Called after the user confirms deleting a task from its row's menu. */
  onDelete: (task: Task) => void;
  /** «Marcar como hecha» in a task row's menu; the item is disabled without it. */
  onDone?: (task: Task) => void;
  /** Items in the inbox that block an agent. */
  waiting: number;
  /** The 56 px icon rail. */
  rail?: boolean;
  /** «Plegar barra lateral» (⌘\\): the shell flips `rail`. */
  onToggleRail?: () => void;
}

/** Most urgent first; finished tasks stay on the board, not here, and backlog ones
 *  (`queued`) sit in their own folded section: they are not pending work. */
const ORDER = ["waiting-for-user", "paused-by-limit", "running", "preparing-workspace", "failed", "setup-failed"];

/** Active tasks shown per project before the «Mostrar N más» expander. */
const VISIBLE_TASKS = 6;
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

export function Sidebar({ view, onView, tasks, prs = [], projects, sessions, selected, onSelect, onOpenSession, onAsk, onIdea, onOpenIdeas, onAddProject, onDelete, onDone, waiting, rail = false, onToggleRail }: Props) {
  // Folded projects, remembered per viewer: a project with many sessions and tasks
  // would otherwise take the whole sidebar.
  const [collapsed, toggle] = useRememberedSet(COLLAPSED_KEY);
  const [openBacklogs, toggleBacklog] = useRememberedSet(OPEN_BACKLOGS_KEY);
  const [filter, setFilter] = useState("");
  const [showAll, setShowAll] = useState<Set<string>>(new Set());
  const q = filter.trim().toLowerCase();
  const matches = (t: Task) => !q || t.title.toLowerCase().includes(q) || t.key.toLowerCase().includes(q);
  const toggleAll = (id: string) =>
    setShowAll((c) => {
      const next = new Set(c);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  const [deleting, setDeleting] = useState<Task | null>(null);
  const prOf = new Map(prs.map((p) => [p.task, p]));
  const tip = (label: string, action: ActionId) => `${label} (${keyOf(action)})`;

  const taskRow = (task: Task) => {
    const st = statusOf(task.status);
    const pr = prOf.get(task.key);
    const tooltip = `${task.key} · ${task.title} · ${st.label}`;
    return (
      <Menu
        key={task.key}
        items={[
          { label: t("Abrir la tarea"), onSelect: () => onSelect(task.key) },
          { label: t("Marcar como hecha"), onSelect: () => onDone?.(task), disabled: !onDone },
          { label: t("Eliminar tarea…"), onSelect: () => setDeleting(task), danger: true },
        ]}
        trigger={(trg) => (
          // Right-click (or the context-menu key) opens the kit Menu under the row.
          <div
            className="sb-task"
            ref={trg.ref}
            onContextMenu={(e) => {
              e.preventDefault();
              trg.onClick();
            }}
          >
            <Row
              className="sb-row sb-taskrow"
              current={selected === task.key}
              aria-label={`${task.key} · ${task.title}`}
              onClick={() => onSelect(task.key)}
              leading={<span className={`sb-dot ${st.tone}`} />}
              title={<span className="sb-title" title={tooltip}>{task.title}</span>}
              trailing={
                <>
                  {pr && <PrBadge pr={pr} />}
                  <span className="sb-key mono">{task.key}</span>
                </>
              }
            />
          </div>
        )}
      />
    );
  };
  // As in Orca: everything grouped by project, its own sessions first, then its tasks.
  const group = (project: Project | null) => {
    const owner = project ? `@${project.name}` : null;
    const mine = tasks.filter((t) => (t.project ?? null) === (project?.name ?? null));
    const active = mine
      .filter((t) => t.status.type !== "done" && t.status.type !== "queued")
      .sort((a, b) => ORDER.indexOf(a.status.type) - ORDER.indexOf(b.status.type));
    return {
      owner,
      project,
      sessions: owner ? sessions.filter((s) => s.task === owner) : [],
      tasks: active.filter(matches),
      allTasks: active,
      // Oldest first: the daemon lists tasks in the order they were created.
      backlog: mine.filter((t) => t.status.type === "queued").filter(matches),
      done: mine.filter((t) => t.status.type === "done").length,
    };
  };
  const groups = [
    ...projects.map(group),
    ...(tasks.some((t) => !t.project) ? [group(null)] : []),
    // While filtering, only the projects with a matching task stay.
  ].filter((g) => !q || g.tasks.length > 0 || g.backlog.length > 0);
  // Something in the group waits on you (also when it is folded).
  const waitsIn = (g: ReturnType<typeof group>) =>
    g.tasks.some((t) => t.status.type === "waiting-for-user") || g.sessions.some((s) => s.state === "waiting");

  const addProject = () => (onAddProject ? onAddProject() : onView("projects"));
  const navItems: { view: View; icon: IconName; label: string; action?: ActionId; count?: number; hot?: boolean }[] = [
    { view: "inbox", icon: "inbox", label: t("Bandeja"), action: "view-inbox", count: waiting, hot: true },
    { view: "board", icon: "tasks", label: t("Tareas"), action: "view-tasks", count: tasks.length },
    { view: "jira", icon: "ticket", label: "Jira", action: "view-jira" },
    { view: "projects", icon: "folder", label: t("Proyectos"), action: "view-projects" },
    { view: "playground", icon: "console", label: t("Playground"), action: "view-playground" },
  ];
  const isOn = (v: View) => view === v;
  const collapseLabel = rail ? t("Desplegar barra lateral") : t("Plegar barra lateral");
  const toggleBtn = (
    <IconButton
      label={collapseLabel}
      icon="rail"
      title={tip(collapseLabel, "toggle-sidebar")}
      onClick={() => onToggleRail?.()}
    />
  );
  const confirm = deleting && (
    <ConfirmDialog
      title={t("¿Eliminar {key}?", { key: deleting.key })}
      body={t("Se borra con su historial y sus sesiones, y se libera su workspace; la rama se queda. No se puede deshacer.")}
      action={t("Eliminar tarea")}
      danger
      onClose={() => setDeleting(null)}
      onConfirm={() => {
        onDelete(deleting);
        setDeleting(null);
      }}
    />
  );

  if (rail) {
    return (
      <nav className="sidebar rail" aria-label={t("Navegación")}>
        {navItems.map((n) => (
          <IconButton
            key={n.view}
            className="sb-railbtn"
            label={n.label}
            icon={n.icon}
            title={n.action ? tip(n.label, n.action) : n.label}
            aria-current={isOn(n.view) ? "true" : undefined}
            onClick={() => onView(n.view)}
          />
        ))}
        <span className="sb-rail-sep" />
        {groups.map((g) => {
          const name = g.project?.name ?? t("Sin proyecto");
          const hot = waitsIn(g);
          return (
            <button
              key={g.owner ?? "none"}
              type="button"
              className="sb-initial"
              data-n={g.allTasks.length || undefined}
              aria-label={name}
              title={name}
              aria-current={g.owner && selected === g.owner ? "true" : undefined}
              onClick={() => (g.owner ? onSelect(g.owner) : toggle("none"))}
            >
              {name.charAt(0).toUpperCase()}
                            {hot && <span className="sb-hot" />}
            </button>
          );
        })}
        <div className="sb-foot">
          <IconButton label={t("Ajustes")} icon="settings" title={t("Ajustes")} aria-current={view === "settings" ? "true" : undefined} onClick={() => onView("settings")} />
          {toggleBtn}
        </div>
      </nav>
    );
  }

  return (
    <nav className="sidebar" aria-label={t("Navegación")}>
      {navItems.map((n) => (
        <Row
          key={n.view}
          className="sb-nav"
          current={isOn(n.view)}
          title={<span className="sb-title" title={n.action ? tip(n.label, n.action) : n.label}>{n.label}</span>}
          onClick={() => onView(n.view)}
          leading={<Icon name={n.icon} />}
          trailing={n.count != null ? <Count n={n.count} hot={n.hot} /> : undefined}
        />
      ))}

      <div className="sb-sec">
        <span className="sec">{t("Proyectos")}</span>
        <IconButton label={t("Añadir proyecto")} icon="plus" title={t("Añadir proyecto")} onClick={addProject} />
      </div>
      <input
        type="search"
        className="sb-filter"
        aria-label={t("Filtrar tareas")}
        placeholder={t("Filtrar tareas")}
        value={filter}
        onChange={(e) => setFilter(e.target.value)}
      />
      {groups.map((g) => {
        const id = g.owner ?? "none";
        const folded = collapsed.has(id);
        const count = g.sessions.length + g.allTasks.length;
        const tones = [...new Set(g.allTasks.map((t) => statusOf(t.status).tone))].slice(0, 4);
        const open = showAll.has(id) || !!q;
        // Past the cap the current task still shows, so the row you are on never disappears.
        const shown = open ? g.tasks : g.tasks.filter((x, i) => i < VISIBLE_TASKS || x.key === selected);
        const hidden = g.tasks.length - VISIBLE_TASKS;
        const hiddenNow = g.tasks.length - shown.length;
        const backlogOpen = openBacklogs.has(id) || !!q;
        const owner = g.owner;
        const name = g.project?.name ?? t("Sin proyecto");
        const ideas = g.sessions.filter((s) => s.conversation);
        const others = g.sessions.filter((s) => !s.conversation);
        const ideasLabel = g.project ? t("Ideas de {name} ({n})", { name: g.project.name, n: ideas.length }) : "";
        const waitingHere = waitsIn(g);
        const chevron = (
          <IconButton
            className={`sb-chev ${folded ? "" : "open"}`}
            label={folded ? t("Desplegar {name}", { name }) : t("Plegar {name}", { name })}
            icon="chevron"
            aria-expanded={!folded}
            onClick={() => toggle(id)}
          />
        );
        const countBadge = folded && count > 0 && (
          <>
            <span className="sb-dots">
              {tones.map((tone) => (
                <span key={tone} className={`sb-dot ${tone}`} />
              ))}
            </span>
            <span className={`sb-count mono ${waitingHere ? "hot" : ""}`}>{count}</span>
          </>
        );
        return (
          <div key={id} className="sb-group">
            {g.project ? (
              <div className={`sb-head ${selected === owner ? "on" : ""}`}>
                {chevron}
                <Row
                  className="sb-main"
                  aria-label={name}
                  current={selected === owner}
                  onClick={() => onSelect(owner!)}
                  title={<span className="sb-title" title={name}>{name}</span>}
                  trailing={countBadge || undefined}
                />
                <IconButton
                  className="sb-act"
                  label={t("Preguntar a Claude sobre {name}", { name })}
                  title={t("Preguntar a Claude sobre el proyecto")}
                  icon="more"
                  onClick={() => onAsk(name)}
                />
                <IconButton
                  className="sb-act"
                  label={t("Nueva tarea en {name}", { name })}
                  title={t("Nueva tarea en este proyecto")}
                  icon="plus"
                  onClick={() => onIdea(name)}
                />
              </div>
            ) : (
              <div className="sb-head">
                {chevron}
                <Row
                  className="sb-main sb-muted"
                  title={t("Sin proyecto")}
                  onClick={() => toggle(id)}
                  trailing={countBadge || undefined}
                />
              </div>
            )}
            {!folded && owner && ideas.length > 0 && (
              <Row
                className="sb-row ideas"
                aria-label={ideasLabel}
                onClick={() => onOpenIdeas(owner, ideas)}
                leading={<Icon name="idea" className="idea" />}
                title={<span className="sb-title" title={ideasLabel}>{t("Ideas · {n}", { n: ideas.length })}</span>}
                trailing={ideas.some((s) => s.state === "waiting") ? <span className="sb-count hot">●</span> : undefined}
              />
            )}
            {!folded &&
              others.map((s) => (
                <Row
                  key={s.id}
                  className="sb-row"
                  onClick={() => onOpenSession(s.task, s.id)}
                  leading={<span className={`sb-dot ${sessionDot(s)}`} />}
                  title={<span className="sb-title" title={s.read_only ? t("Claude en solo lectura") : s.label}>{s.label}</span>}
                  trailing={s.read_only ? <Icon name="lock" className="sb-lock" /> : undefined}
                />
              ))}
            {!folded && shown.map(taskRow)}
            {!folded && !q && (open ? hidden > 0 : hiddenNow > 0) && (
              <Row
                className="sb-row sb-more"
                aria-expanded={open}
                onClick={() => toggleAll(id)}
                title={open ? t("Mostrar menos") : t("Mostrar {n} más", { n: hiddenNow })}
              />
            )}
            {!folded && g.backlog.length > 0 && (
              <>
                <Row
                  className={`sb-row sb-fold ${backlogOpen ? "open" : ""}`}
                  aria-expanded={backlogOpen}
                  onClick={() => toggleBacklog(id)}
                  leading={<Icon name="chevron" />}
                  title={t("Backlog · {n}", { n: g.backlog.length })}
                />
                {backlogOpen && g.backlog.map(taskRow)}
              </>
            )}
            {!folded && g.done > 0 && <div className="sb-done">{tn(g.done, "{n} hecha", "{n} hechas")}</div>}
          </div>
        );
      })}
      {q && groups.length === 0 && <p className="sb-empty">{t("Ninguna tarea coincide.")}</p>}
      {projects.length === 0 && tasks.length === 0 && (
        <p className="sb-empty">{t("Añade un proyecto en Proyectos.")}</p>
      )}
      <div className="sb-foot">
        <Row
          className="sb-nav"
          current={view === "settings"}
          title={t("Ajustes")}
          onClick={() => onView("settings")}
          leading={<Icon name="settings" />}
        />
        <Row
          className="sb-nav"
          title={<span className="sb-title" title={tip(t("Plegar barra lateral"), "toggle-sidebar")}>{t("Plegar barra lateral")}</span>}
          onClick={() => onToggleRail?.()}
          leading={<Icon name="rail" />}
        />
      </div>
      {confirm}
    </nav>
  );
}
