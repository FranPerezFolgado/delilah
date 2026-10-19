import { useCallback, useEffect, useState } from "react";
import { api, type Project, type Session, type Task, type UsageReport } from "./api";
import { ConsoleView } from "./components/ConsoleView";
import { JiraView } from "./components/JiraView";
import { NewTaskDialog, type TaskDraft } from "./components/NewTaskDialog";
import { ProjectsView } from "./components/ProjectsView";
import { Sidebar, type View } from "./components/Sidebar";
import { StatusBar } from "./components/StatusBar";
import { TaskView } from "./components/TaskView";

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
    if (!selected && tasks.length > 0) setSelected(tasks[tasks.length - 1].key);
  }, [tasks, selected]);

  const openConsole = (key: string, sessionId?: string) => {
    setSelected(key);
    setFocusSession(sessionId ?? sessions.find((s) => s.task === key && s.primary)?.id ?? null);
    setView("console");
    refresh();
  };
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
            if (view === "projects" || view === "jira") setView("tasks");
          }}
          onNewTask={() => setNewTask(true)}
        />
        <main>
          {!online && (
            <div className="notice" style={{ margin: 12, borderColor: "var(--err)" }}>
              <span className="c-err">●</span>
              <span>
                No hay conexión con <span className="mono">delilahd</span>. Arráncalo con{" "}
                <span className="mono selectable">cargo run -p delilah-daemon</span>.
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
              tasks={tasks}
              sessions={sessions}
              task={selected}
              focus={focusSession}
              onFocus={(id, key) => {
                setFocusSession(id);
                setSelected(key);
              }}
              onChanged={refresh}
              onNewTask={() => setNewTask(true)}
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
        </main>
      </div>
      <StatusBar online={online} active={active} usage={usage} />
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
