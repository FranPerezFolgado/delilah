import { useCallback, useEffect, useRef, useState } from "react";
import { api, type Project, type Session, type SessionKind, type Task, type UsageReport } from "./api";
import { ConsoleView, consoleOrder, type Layout } from "./components/ConsoleView";
import { JiraView } from "./components/JiraView";
import { NewTaskDialog, type TaskDraft } from "./components/NewTaskDialog";
import { ProjectsView } from "./components/ProjectsView";
import { SettingsView } from "./components/SettingsView";
import { Sidebar, type View } from "./components/Sidebar";
import { StatusBar } from "./components/StatusBar";
import { TaskView } from "./components/TaskView";
import { actionFor, type ActionId, type Bindings, DEFAULTS, setBindings } from "./shortcuts";

export function App() {
  const [view, setView] = useState<View>("tasks");
  const [tasks, setTasks] = useState<Task[]>([]);
  const [sessions, setSessions] = useState<Session[]>([]);
  const [focusSession, setFocusSession] = useState<string | null>(null);
  const [projects, setProjects] = useState<Project[]>([]);
  const [usage, setUsage] = useState<UsageReport | null>(null);
  const [online, setOnline] = useState(true);
  const [selected, setSelected] = useState<string | null>(null);
  const [newTask, setNewTask] = useState(false);
  const [draft, setDraft] = useState<TaskDraft | undefined>(undefined);
  const [layout, setLayout] = useState<Layout>(4);
  const [restoreLayout, setRestoreLayout] = useState<Layout>(4);
  const [bindings, setBindingsState] = useState<Bindings>(DEFAULTS);
  const [toast, setToast] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    try {
      const [t, p, u, s] = await Promise.all([api.tasks(), api.projects(), api.usage(), api.sessions()]);
      setTasks(t);
      setSessions(s);
      setProjects(p);
      setUsage(u);
      setOnline(true);
    } catch {
      setOnline(false);
    }
  }, []);

  // No event stream yet: polling every 2 s is plenty for a local daemon.
  useEffect(() => {
    refresh();
    const timer = setInterval(refresh, 2000);
    return () => clearInterval(timer);
  }, [refresh]);

  useEffect(() => {
    api
      .preference<Partial<Bindings>>("shortcuts")
      .then((saved) => applyBindings({ ...DEFAULTS, ...(saved ?? {}) }))
      .catch(() => {});
  }, []);

  useEffect(() => {
    if (!selected && tasks.length > 0) setSelected(tasks[tasks.length - 1].key);
  }, [tasks, selected]);

  useEffect(() => {
    if (!toast) return;
    const timer = setTimeout(() => setToast(null), 3500);
    return () => clearTimeout(timer);
  }, [toast]);

  const applyBindings = (b: Bindings) => {
    setBindings(b);
    setBindingsState(b);
  };

  const openConsole = (key: string, sessionId?: string) => {
    setSelected(key);
    setFocusSession(sessionId ?? sessions.find((s) => s.task === key && s.primary)?.id ?? null);
    setView("console");
    refresh();
  };

  const shown = consoleOrder(tasks, sessions, selected, focusSession);
  const focused = shown.find((s) => s.id === focusSession) ?? (view === "console" ? shown[0] : undefined);

  // The task a shortcut acts on: the focused pane's in the console, else the selected one.
  const currentTask = () => (view === "console" && focused ? focused.task : selected);

  const openSession = async (kind: SessionKind) => {
    const key = currentTask();
    if (!key || !sessions.some((s) => s.task === key)) {
      setToast("Esta tarea no tiene workspace donde abrir una sesión.");
      return;
    }
    try {
      const s = await api.openSession(key, { kind });
      openConsole(key, s.id);
    } catch (e) {
      setToast((e as Error).message);
    }
  };

  const closeFocused = async () => {
    if (view !== "console" || !focused) return;
    if (focused.primary) {
      setToast("La sesión principal no se cierra: termina con la tarea (Marcar como hecha).");
      return;
    }
    const next = shown.filter((s) => s.id !== focused.id);
    try {
      await api.closeSession(focused.id);
      setFocusSession(next[0]?.id ?? null);
      refresh();
    } catch (e) {
      setToast((e as Error).message);
    }
  };

  const movePane = (step: number) => {
    if (view !== "console" || shown.length === 0) return;
    const visible = shown.slice(0, layout);
    const at = Math.max(0, visible.findIndex((s) => s.id === focused?.id));
    const next = visible[(at + step + visible.length) % visible.length];
    setFocusSession(next.id);
    setSelected(next.task);
  };

  const run = (action: ActionId) => {
    switch (action) {
      case "new-terminal":
        return openSession("shell");
      case "new-claude":
        return openSession("claude");
      case "close-session":
        return closeFocused();
      case "next-pane":
        return movePane(1);
      case "prev-pane":
        return movePane(-1);
      case "toggle-maximize":
        if (view !== "console") return;
        if (layout === 1) setLayout(restoreLayout);
        else {
          setRestoreLayout(layout);
          setLayout(1);
        }
        return;
      case "new-task":
        return setNewTask(true);
      case "view-tasks":
        return setView("tasks");
      case "view-console":
        return setView("console");
      case "view-jira":
        return setView("jira");
      case "view-projects":
        return setView("projects");
    }
  };

  // One listener for the whole window, in the capture phase so it wins over the
  // terminal. The latest `run` is read through a ref to keep the listener stable.
  const runRef = useRef(run);
  runRef.current = run;
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const action = actionFor(e);
      if (!action) return;
      e.preventDefault();
      e.stopPropagation();
      runRef.current(action);
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, []);

  const task = tasks.find((t) => t.key === selected);
  const active = tasks.filter((t) => t.status.type === "running").length;

  return (
    <div className="window">
      <header className="titlebar" data-tauri-drag-region>
        <span data-tauri-drag-region>Delilah</span>
        {task && view === "tasks" && (
          <span data-tauri-drag-region className="mono" style={{ color: "var(--tx)" }}>/ {task.key}</span>
        )}
      </header>
      <div className="body">
        <Sidebar
          view={view}
          onView={setView}
          tasks={tasks}
          selected={selected}
          onSelect={(key) => {
            setSelected(key);
            setView("tasks");
          }}
          onNewTask={() => setNewTask(true)}
        />
        <main>
          {!online && (
            <div className="notice" style={{ margin: 12, borderColor: "var(--err)" }}>
              <span className="c-err">●</span>
              <span>
                No hay conexión con <span className="mono">delilahd</span>. Delilah lo arranca al abrirse; si sigue
                así, prueba <b>Delilah → Reiniciar el daemon</b> y mira{" "}
                <span className="mono selectable">~/Library/Logs/Delilah/delilahd.log</span>. En desarrollo:{" "}
                <span className="mono selectable">scripts/dev-daemon.sh</span>.
              </span>
            </div>
          )}
          {view === "tasks" &&
            (task ? (
              <TaskView task={task} sessions={sessions.filter((s) => s.task === task.key)} onOpenConsole={openConsole} onChanged={refresh} />
            ) : (
              <div className="empty">
                <span>Todavía no hay tareas.</span>
                <button className="btn btn-p" onClick={() => setNewTask(true)}>Nueva tarea</button>
              </div>
            ))}
          {view === "console" && (
            <ConsoleView
              layout={layout}
              onLayout={setLayout}
              tasks={tasks}
              sessions={sessions}
              task={selected}
              focus={focused?.id ?? null}
              onFocus={(id, key) => {
                setFocusSession(id);
                setSelected(key);
              }}
              onChanged={refresh}
              onNewTask={() => setNewTask(true)}
              onOpenTask={(key) => {
                setSelected(key);
                setView("tasks");
              }}
            />
          )}
          {view === "jira" && (
            <JiraView
              onAnalyze={(issue) => {
                setDraft({ key: issue.key, title: issue.summary, components: issue.components, description: issue.description });
                setNewTask(true);
              }}
              onOpenTask={(key) => {
                setSelected(key);
                setView("tasks");
              }}
            />
          )}
          {view === "projects" && <ProjectsView projects={projects} onChanged={refresh} />}
          {view === "settings" && <SettingsView bindings={bindings} onChange={applyBindings} />}
        </main>
      </div>
      <StatusBar online={online} active={active} usage={usage} />
      {toast && (
        <div className="toast" role="status">
          {toast}
        </div>
      )}
      {newTask && (
        <NewTaskDialog
          projects={projects}
          draft={draft}
          onClose={() => {
            setNewTask(false);
            setDraft(undefined);
          }}
          onStarted={(t) => {
            setNewTask(false);
            setDraft(undefined);
            refresh();
            openConsole(t.key);
          }}
        />
      )}
    </div>
  );
}
