import { lazy, Suspense, useCallback, useEffect, useRef, useState } from "react";
import { api, BLOCKING, type InboxItem, type Project, type Session, type SessionKind, type SpecSummary, type Task, type UsageReport } from "./api";
import { InboxView } from "./components/InboxView";
import { LogsView } from "./components/LogsView";
import { connect, on, type Topic } from "./live";
import { useDesktopAlerts } from "./alerts";
import { CHANGES, persist, sessionFor, sessionTab } from "./editor/session";
import { BoardView } from "./components/BoardView";
import { JiraView } from "./components/JiraView";
import { NewTaskDialog, type TaskDraft } from "./components/NewTaskDialog";
import { ProjectsView } from "./components/ProjectsView";
import { SettingsView } from "./components/SettingsView";
import { Sidebar, type View } from "./components/Sidebar";
import { StatusBar } from "./components/StatusBar";
import { actionFor, type ActionId, type Bindings, DEFAULTS, setBindings } from "./shortcuts";

// The task screen carries CodeMirror, most of the bundle: load it when a task opens.
const TaskScreen = lazy(() => import("./components/TaskScreen").then((m) => ({ default: m.TaskScreen })));

export function App() {
  const [view, setView] = useState<View>("board");
  const [tasks, setTasks] = useState<Task[]>([]);
  const [specs, setSpecs] = useState<Record<string, SpecSummary>>({});
  const [inbox, setInbox] = useState<InboxItem[]>([]);
  const [sessions, setSessions] = useState<Session[]>([]);
  const [projects, setProjects] = useState<Project[]>([]);
  const [usage, setUsage] = useState<UsageReport | null>(null);
  const [online, setOnline] = useState(true);
  const [selected, setSelected] = useState<string | null>(null);
  const [newTask, setNewTask] = useState(false);
  const [draft, setDraft] = useState<TaskDraft | undefined>(undefined);
  const [bindings, setBindingsState] = useState<Bindings>(DEFAULTS);
  const [toast, setToast] = useState<string | null>(null);
  const [convertFrom, setConvertFrom] = useState<Session | null>(null);
  const [showLogs, setShowLogs] = useState(false);

  // Help › Ver logs, from the native menu.
  useEffect(() => {
    const open = () => setShowLogs(true);
    window.addEventListener("delilah:show-logs", open);
    return () => window.removeEventListener("delilah:show-logs", open);
  }, []);

  const refresh = useCallback(async () => {
    try {
      const [t, p, u, s, sp, ib] = await Promise.all([
        api.tasks(),
        api.projects(),
        api.usage(),
        api.sessions(),
        // An older daemon has no specs: the board still works, without phases.
        api.specs().catch(() => ({})),
        api.inbox().catch(() => []),
      ]);
      setTasks(t);
      setSpecs(sp);
      setInbox(ib);
      setSessions(s);
      setProjects(p);
      setUsage(u);
      setOnline(true);
    } catch {
      setOnline(false);
    }
  }, []);

  // The daemon says what changed and only that is fetched again. A slow full refresh
  // stays as a safety net, should a message ever be missed.
  useEffect(() => {
    const fetchers: Record<Topic, () => Promise<unknown>> = {
      tasks: () => api.tasks().then(setTasks),
      sessions: () => api.sessions().then(setSessions),
      inbox: () => api.inbox().then(setInbox),
      specs: () => api.specs().then(setSpecs),
      usage: () => api.usage().then(setUsage),
      projects: () => api.projects().then(setProjects),
    };
    const offs = (Object.keys(fetchers) as Topic[]).map((t) => on(t, () => fetchers[t]().catch(() => {})));
    const stop = connect((connected) => {
      setOnline(connected);
      // Lost the feed: fall back to a full fetch now, and the slow refresh after.
      if (!connected) refresh();
    });
    const timer = setInterval(refresh, 30_000);
    return () => {
      offs.forEach((off) => off());
      stop();
      clearInterval(timer);
    };
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




  /** Shows a task's screen, on one of its session tabs if given. */
  const openTask = (key: string, sessionId?: string) => {
    if (sessionId) {
      const ts = sessionFor(key);
      ts.active = sessionTab(sessionId);
      persist(key, ts);
    }
    setSelected(key);
    setView("tasks");
  };
  const openChanges = (key: string) => {
    const ts = sessionFor(key);
    ts.changesOpen = true;
    ts.active = CHANGES;
    persist(key, ts);
    setSelected(key);
    setView("tasks");
  };

  /** Where an inbox item is dealt with: its session's tab, Cambios, or the task. */
  const openItem = (item: InboxItem) => {
    if (item.kind === "review") return openChanges(item.task);
    const primary = sessions.find((s) => s.task === item.task && s.primary);
    openTask(item.task, item.session?.id ?? (item.kind === "waiting" ? primary?.id : undefined));
  };
  useDesktopAlerts(inbox, tasks, openItem);

  const run = (action: ActionId) => {
    switch (action) {
      case "new-terminal":
      case "new-claude": {
        const kind: SessionKind = action === "new-terminal" ? "shell" : "claude";
        // In a task the new session opens as one of its tabs.
        if (view === "tasks" && (selected?.startsWith("@") || sessions.some((s) => s.task === selected))) return void window.dispatchEvent(new CustomEvent("delilah:new-session", { detail: kind }));
        setToast("Abre una tarea con workspace, o un proyecto, para abrir una sesión.");
        return;
      }
      case "close-session":
        // In a task, ⌘W closes its active tab (a file, the changes), as in any editor.
        if (view === "tasks") window.dispatchEvent(new Event("delilah:close-tab"));
        return;
      case "next-pane":
      case "prev-pane": {
        const step = action === "next-pane" ? 1 : -1;
        if (view === "tasks") window.dispatchEvent(new CustomEvent("delilah:cycle-tab", { detail: step }));
        return;
      }
      case "toggle-tree":
        if (view === "tasks") window.dispatchEvent(new Event("delilah:toggle-tree"));
        return;
      case "quick-open":
        if (view === "tasks") window.dispatchEvent(new Event("delilah:quick-open"));
        return;
      case "new-task":
        return setNewTask(true);
      case "view-inbox":
        return setView("inbox");
      case "view-tasks":
        return setView("board");
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
  // `@<name>` selects a project's own screen (sessions on its checkout).
  const project = selected?.startsWith("@") ? projects.find((p) => p.name === selected.slice(1)) : undefined;
  const owner = task?.key ?? (project ? `@${project.name}` : null);
  const active = tasks.filter((t) => t.status.type === "running").length;

  return (
    <div className="window">
      <header className="titlebar" data-tauri-drag-region>
        <span data-tauri-drag-region>Delilah</span>
        {owner && view === "tasks" && (
          <span data-tauri-drag-region className="mono" style={{ color: "var(--tx)" }}>/ {task?.key ?? project?.name}</span>
        )}
      </header>
      <div className="body">
        <Sidebar
          view={view}
          onView={setView}
          tasks={tasks}
          projects={projects}
          sessions={sessions}
          selected={selected}
          onSelect={(key) => {
            setSelected(key);
            setView("tasks");
          }}
          onOpenSession={openTask}
          onAsk={(name) =>
            api
              .openProjectSession(name, { kind: "claude" })
              .then((s) => {
                refresh();
                openTask(`@${name}`, s.id);
              })
              .catch((e) => setToast((e as Error).message))
          }
          onNewTask={() => setNewTask(true)}
          waiting={inbox.filter((i) => BLOCKING.includes(i.kind)).length}
        />
        <main>
          {!online && (
            <div className="notice" style={{ margin: 12, borderColor: "var(--err)" }}>
              <span className="c-err">●</span>
              <span>
                No hay conexión con <span className="mono">delilahd</span>.{" "}
                {import.meta.env.DEV ? (
                  <>
                    En desarrollo arráncalo con <span className="mono selectable">scripts/dev-daemon.sh</span>.
                  </>
                ) : (
                  <>
                    Delilah lo arranca al abrirse; si sigue así, prueba <b>Delilah → Reiniciar el daemon</b> y mira{" "}
                    <span className="mono selectable">~/Library/Logs/Delilah/delilahd.log</span>.
                  </>
                )}
              </span>
            </div>
          )}
          {view === "inbox" && <InboxView items={inbox} onOpenTask={openTask} onOpenChanges={openChanges} onChanged={refresh} />}
          {view === "board" && (
            <BoardView
              tasks={tasks}
              specs={specs}
              onOpen={(key) => {
                setSelected(key);
                setView("tasks");
              }}
              onNewTask={() => setNewTask(true)}
            />
          )}
          {view === "tasks" &&
            (owner ? (
              <Suspense fallback={<div className="empty">Cargando…</div>}>
                <TaskScreen
                  key={owner}
                  owner={owner}
                  task={task}
                  project={project}
                  sessions={sessions.filter((s) => s.task === owner)}
                  asks={inbox.filter((i) => i.task === owner && i.ask)}
                  onChanged={refresh}
                  onNewTask={() => setNewTask(true)}
                  onConvert={(s) => {
                    setConvertFrom(s);
                    setNewTask(true);
                  }}
                />
              </Suspense>
            ) : (
              <div className="empty">
                <span>Elige una tarea del tablero.</span>
                <button className="btn" onClick={() => setView("board")}>Ir al tablero</button>
              </div>
            ))}
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
      {showLogs && <LogsView onClose={() => setShowLogs(false)} />}
      {toast && (
        <div className="toast" role="status">
          {toast}
        </div>
      )}
      {newTask && (
        <NewTaskDialog
          projects={projects}
          project={project?.name ?? task?.project ?? undefined}
          fromSession={convertFrom ? { id: convertFrom.id, label: convertFrom.label } : undefined}
          draft={draft}
          onClose={() => {
            setNewTask(false);
            setDraft(undefined);
            setConvertFrom(null);
          }}
          onStarted={(t) => {
            setNewTask(false);
            setDraft(undefined);
            setConvertFrom(null);
            refresh();
            openTask(t.key);
          }}
        />
      )}
    </div>
  );
}
