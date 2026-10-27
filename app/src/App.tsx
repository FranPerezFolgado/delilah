import { lazy, Suspense, useCallback, useEffect, useRef, useState } from "react";
import { api, ApiError, baseStaleNotices, BLOCKING, parseAskPosition, projectOfKey, type AskPosition, type InboxItem, type Project, type PrSummary, type Session, type SessionKind, type SpecSummary, type Task, type UsageReport } from "./api";
import { InboxView } from "./components/InboxView";
import { LogsView } from "./components/LogsView";
import { connect, on, type Topic } from "./live";
import { useDesktopAlerts } from "./alerts";
import { onOpenTask } from "./platform";
import { parseSetting, setLanguage, t, type LangSetting } from "./i18n";
import { CHANGES, OFFICE, persist, sessionFor, sessionOf, sessionTab } from "./editor/session";
import { ideaToOpen } from "./components/ideas";
import { BoardView } from "./components/BoardView";
import { JiraView } from "./components/JiraView";
import { NewTaskDialog, type TaskDraft } from "./components/NewTaskDialog";
import { DeleteTaskDialog } from "./components/DeleteTaskDialog";
import { BacklogTaskView } from "./components/BacklogTaskView";
import { ProjectSettings } from "./components/ProjectSettings";
import { ProjectsView } from "./components/ProjectsView";
import { SettingsView } from "./components/SettingsView";
import { Sidebar, type View } from "./components/Sidebar";
import { GlobalNotices } from "./components/GlobalNotices";
import { UpdateNotice } from "./components/UpdateNotice";
import { useUpdates } from "./useUpdates";
import { canUpdate } from "./platform";
import { StatusBar, type StatusFilter } from "./components/StatusBar";
import { pushToast, ToastHost } from "./components/ui";
import { useBreakpoints } from "./useWidth";
import "./App.css";
import { DisconnectedBanner } from "./components/DisconnectedBanner";
import { useConnection } from "./connection";
import { actionFor, type ActionId, type Bindings, DEFAULTS, setBindings } from "./shortcuts";

// The task screen carries CodeMirror, most of the bundle: load it when a task opens.
const TaskScreen = lazy(() => import("./components/TaskScreen").then((m) => ({ default: m.TaskScreen })));

const PlaygroundView = lazy(() => import("./playground/PlaygroundView").then((m) => ({ default: m.PlaygroundView })));

const RAIL_KEY = "delilah:rail";
const loadRail = (): boolean | null => {
  try {
    const v = localStorage.getItem(RAIL_KEY);
    return v === null ? null : v === "1";
  } catch {
    return null;
  }
};
const setToast = (text: string) => void pushToast({ text, kind: "error" });
const notify = (text: string) => void pushToast({ text });

const VIEW_LABEL: Record<View, () => string> = {
  board: () => t("Tareas"),
  inbox: () => t("Bandeja"),
  jira: () => "Jira",
  projects: () => t("Proyectos"),
  playground: () => t("Playground"),
  settings: () => t("Ajustes"),
  tasks: () => t("Tareas"),
};

export function App() {
  const [view, setView] = useState<View>("board");
  const [tasks, setTasks] = useState<Task[]>([]);
  const [specs, setSpecs] = useState<Record<string, SpecSummary>>({});
  const [prs, setPrs] = useState<PrSummary[]>([]);
  const [inbox, setInbox] = useState<InboxItem[]>([]);
  const [sessions, setSessions] = useState<Session[]>([]);
  const [projects, setProjects] = useState<Project[]>([]);
  const [usage, setUsage] = useState<UsageReport | null>(null);
  const conn = useConnection();
  const [statusFilter, setStatusFilter] = useState<StatusFilter | null>(null);
  const bp = useBreakpoints();
  // ⌘\ overrides the automatic rail (narrow window), remembered per viewer.
  const [railPref, setRailPref] = useState<boolean | null>(loadRail);
  const rail = railPref ?? bp.rail;
  const toggleRail = () => {
    const next = !rail;
    setRailPref(next);
    try {
      localStorage.setItem(RAIL_KEY, next ? "1" : "0");
    } catch {
      /* private mode: the choice lasts until the window closes */
    }
  };
  const [selected, setSelected] = useState<string | null>(null);
  const [newTask, setNewTask] = useState(false);
  // «Nueva subtarea»: the parent task the dialog creates a subtask of.
  const [parentTask, setParentTask] = useState<Task | null>(null);
  const [draft, setDraft] = useState<TaskDraft | undefined>(undefined);
  const [bindings, setBindingsState] = useState<Bindings>(DEFAULTS);
  const [convertFrom, setConvertFrom] = useState<Session | null>(null);
  // A backlog task being started ("Empezar"), and one being deleted.
  const [starting, setStarting] = useState<Task | null>(null);
  const [deleting, setDeleting] = useState<Task | null>(null);
  // A project's settings, opened from a task's setup banner.
  const [settingsOf, setSettingsOf] = useState<string | null>(null);
  const [addingProject, setAddingProject] = useState(false);
  const [showLogs, setShowLogs] = useState(false);
  const [loaded, setLoaded] = useState(false);

  // "Eliminar" from a task's own view.
  useEffect(() => {
    const ask = (e: Event) => setDeleting((e as CustomEvent<Task>).detail);
    window.addEventListener("delilah:delete-task", ask);
    return () => window.removeEventListener("delilah:delete-task", ask);
  }, []);

  // Help › Ver logs, from the native menu.
  useEffect(() => {
    const open = () => setShowLogs(true);
    window.addEventListener("delilah:show-logs", open);
    return () => window.removeEventListener("delilah:show-logs", open);
  }, []);

  const refresh = useCallback(async () => {
    try {
      const [t, p, u, s, sp, ib, pr] = await Promise.all([
        api.tasks(),
        api.projects(),
        api.usage(),
        api.sessions(),
        // An older daemon has no specs: the board still works, without phases.
        api.specs().catch(() => ({})),
        api.inbox().catch(() => []),
        // An older daemon has no PRs: no badges.
        api.prSummaries().catch(() => []),
      ]);
      setTasks(t);
      setSpecs(sp);
      setInbox(ib);
      setPrs(pr);
      setSessions(s);
      setProjects(p);
      setUsage(u);
      setLoaded(true);
    } catch {
      /* the connection state is tracked by useConnection */
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
      prs: () => api.prSummaries().then(setPrs),
      // The open conversation view subscribes to this topic itself.
      conversations: () => Promise.resolve(),
      activity: async () => {}, // nothing here reads it; the playground views fetch it themselves
    };
    const offs = (Object.keys(fetchers) as Topic[]).map((t) => on(t, () => fetchers[t]().catch(() => {})));
    const stop = connect((connected) => {
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

  // The language: "auto" follows the Mac. Changing it re-renders everything.
  const [language, setLanguageState] = useState<LangSetting>("auto");
  const applyLanguage = (l: LangSetting) => {
    setLanguage(l);
    setLanguageState(l);
  };
  useEffect(() => {
    api
      .preference<unknown>("language")
      .then((l) => l != null && applyLanguage(parseSetting(l)))
      .catch(() => {});
  }, []);

  useEffect(() => {
    api
      .preference<Partial<Bindings>>("shortcuts")
      .then((saved) => applyBindings({ ...DEFAULTS, ...(saved ?? {}) }))
      .catch(() => {});
  }, []);

  // Where a session's questions with options sit: right of the terminal unless chosen.
  const [askPosition, setAskPosition] = useState<AskPosition>("right");
  useEffect(() => {
    api
      .preference<unknown>("ask-position")
      .then((v) => setAskPosition(parseAskPosition(v)))
      .catch(() => {});
  }, []);

  useEffect(() => {
    if (!selected && tasks.length > 0) setSelected(tasks[tasks.length - 1].key);
  }, [tasks, selected]);

  const applyBindings = (b: Bindings) => {
    setBindings(b);
    setBindingsState(b);
  };




  /** The sidebar already confirmed: delete now, and only ask again when work would be lost. */
  const deleteConfirmed = async (tk: Task) => {
    try {
      await api.deleteTask(tk.key);
      if (selected === tk.key) {
        setSelected(tk.project ? `@${tk.project}` : null);
        if (!tk.project) setView("board");
      }
      refresh();
    } catch (e) {
      if (e instanceof ApiError && e.unpushed) setDeleting(tk);
      else setToast((e as Error).message);
    }
  };

  /** Shows a task's screen, on one of its session tabs if given. */
  const openTask = (key: string, target?: string) => {
    if (target) {
      const ts = sessionFor(key);
      ts.active = target === OFFICE ? OFFICE : sessionTab(target);
      persist(key, ts);
    }
    setSelected(key);
    setView("tasks");
  };
  /** Shows a new idea conversation, a session of the project's own screen. */
  const openConversation = (name: string, s: Session) => {
    refresh();
    openTask(`@${name}`, s.id);
  };
  /** The project's screen on one of its ideas: see `ideaToOpen`. */
  const openIdeas = (owner: string, ideas: Session[]) => openTask(owner, ideaToOpen(ideas, sessionOf(sessionFor(owner).active)).id);
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
  useDesktopAlerts(inbox, tasks, prs, openItem);
  const updates = useUpdates();

  const run = (action: ActionId) => {
    switch (action) {
      case "new-terminal":
      case "new-claude": {
        const kind: SessionKind = action === "new-terminal" ? "shell" : "claude";
        // In a task the new session opens as one of its tabs.
        if (view === "tasks" && (selected?.startsWith("@") || sessions.some((s) => s.task === selected))) return void window.dispatchEvent(new CustomEvent("delilah:new-session", { detail: kind }));
        notify(t("Abre una tarea con workspace, o un proyecto, para abrir una sesión."));
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
      case "toggle-preview":
        if (view === "tasks") window.dispatchEvent(new Event("delilah:toggle-preview"));
        return;
      case "toggle-sidebar":
        return toggleRail();
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
      case "view-playground":
        return setView("playground");
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

  // A click in the playground window opens that task's terminal here.
  const openTaskRef = useRef(openTask);
  openTaskRef.current = openTask;
  useEffect(() => {
    const off = onOpenTask(({ task, session }) => openTaskRef.current(task, session));
    off.catch(() => {});
    return () => void off.then((f) => f()).catch(() => {});
  }, []);

  const task = tasks.find((t) => t.key === selected);
  // `@<name>` selects a project's own screen (sessions on its checkout).
  const project = projects.find((p) => p.name === projectOfKey(selected));
  const asks: Record<string, string> = {};
  for (const i of inbox) if (i.kind === "waiting" && !(i.task in asks)) asks[i.task] = i.text ?? i.title;
  // When each done task finished, for the board's «Hecha» cards.
  const finished: Record<string, string> = {};
  for (const x of tasks) if (x.finished_at) finished[x.key] = x.finished_at;
  const owner = task?.key ?? (project ? `@${project.name}` : null);

  const crumbs: { text: string; mono?: boolean }[] = [{ text: "Delilah" }];
  if (view === "tasks" && owner) {
    const proj = task?.project ?? project?.name;
    if (proj) crumbs.push({ text: proj });
    if (task) crumbs.push({ text: task.key, mono: true });
  } else {
    crumbs.push({ text: VIEW_LABEL[view]() });
  }
  const offline = conn.status === "down";

  return (
    <div className="window">
      <header className="titlebar" data-tauri-drag-region>
        <nav className="crumbs" aria-label={t("Ruta de navegación")} data-tauri-drag-region>
          {crumbs.map((c, i) => (
            <span key={i} data-tauri-drag-region className={`crumb trunc${i === crumbs.length - 1 ? " crumb-last" : ""}${c.mono ? " mono" : ""}`} title={c.text}>
              {i > 0 && <span className="crumb-sep" data-tauri-drag-region>/ </span>}
              {c.text}
            </span>
          ))}
        </nav>
      </header>
      <div className="body">
        <Sidebar
          view={view}
          onView={setView}
          tasks={tasks}
          prs={prs}
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
          onIdea={(name) =>
            api
              .openProjectSession(name, { kind: "claude", conversation: true })
              .then((s) => openConversation(name, s))
              .catch((e) => setToast((e as Error).message))
          }
          onOpenIdeas={openIdeas}
          onAddProject={() => {
            setView("projects");
            setAddingProject(true);
          }}
          onDone={(tk) => api.done(tk.key).then(refresh).catch((e) => setToast((e as Error).message))}
          rail={rail}
          onToggleRail={toggleRail}
          onDelete={deleteConfirmed}
          waiting={inbox.filter((i) => BLOCKING.includes(i.kind)).length}
        />
        <main className="shell-main">
          {offline && (
            <div className="shell-banner">
              <DisconnectedBanner lastOk={conn.lastOk} onRetry={conn.retry} />
            </div>
          )}
          <div className={`shell-content${offline ? " is-offline" : ""}`} aria-disabled={offline || undefined} {...(offline ? { inert: true } : {})}>
          <GlobalNotices tasks={tasks} usage={usage} current={view === "tasks" ? selected : null} onOpenTask={openTask} onOpenInbox={() => setView("inbox")} />
          <UpdateNotice state={updates.state} dispatch={updates.dispatch} installed={updates.meta.installed} />
          {view === "inbox" && <InboxView items={inbox} projects={projects} loading={!loaded} onOpenTask={openTask} onOpenChanges={openChanges} onChanged={refresh} />}
          {view === "playground" && (
            <Suspense fallback={<div className="empty">{t("Cargando…")}</div>}>
              <PlaygroundView onOpenFloor={(owner) => openTask(owner, OFFICE)} online={!offline} />
            </Suspense>
          )}
          {view === "board" && (
            <BoardView
              tasks={tasks}
              prs={prs}
              specs={specs}
              projects={projects}
              statusFilter={statusFilter}
              onClearStatusFilter={() => setStatusFilter(null)}
              loading={!loaded}
              asks={asks}
              finished={finished}
              onAddProject={() => setView("projects")}
              onConnectJira={() => setView("jira")}
              onOpen={(key) => {
                setSelected(key);
                setView("tasks");
              }}
              onNewTask={() => setNewTask(true)}
            />
          )}
          {view === "tasks" && task?.status.type === "queued" && (
            <BacklogTaskView
              key={task.key}
              task={task}
              projects={projects}
              onOpen={(key) => {
                setSelected(key);
                refresh();
              }}
              onChanged={refresh}
            />
          )}
          {view === "tasks" && task?.status.type !== "queued" &&
            (owner ? (
              <Suspense fallback={<div className="empty">{t("Cargando…")}</div>}>
                <TaskScreen
                  key={owner}
                  online={!offline}
                  owner={owner}
                  task={task}
                  spec={specs[owner]}
                  project={project}
                  projects={projects}
                  tasks={tasks}
                  sessions={sessions.filter((s) => s.task === owner)}
                  asks={inbox.filter((i) => i.task === owner && (i.ask || i.permission))}
                  notices={baseStaleNotices(inbox, owner)}
                  askPosition={askPosition}
                  onChanged={refresh}
                  onNewTask={() => setNewTask(true)}
                  onNewSubtask={(parent) => {
                    setParentTask(parent);
                    setNewTask(true);
                  }}
                  onOpenTask={(key) => openTask(key)}
                  onOpenSettings={setSettingsOf}
                  onConvert={(s) => {
                    setConvertFrom(s);
                    setNewTask(true);
                  }}
                />
              </Suspense>
            ) : (
              <div className="empty">
                <span>{t("Elige una tarea del tablero.")}</span>
                <button className="btn" onClick={() => setView("board")}>{t("Ir al tablero")}</button>
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
          {view === "projects" && <ProjectsView
              projects={projects}
              tasks={tasks}
              onChanged={refresh}
              onOpenProject={(name) => openTask(`@${name}`)}
              adding={addingProject}
              onAddingChange={setAddingProject}
            />}
          {view === "settings" && <SettingsView
              bindings={bindings}
              askPosition={askPosition}
              onAskPosition={(p) => {
                const prev = askPosition;
                setAskPosition(p);
                api.setPreference("ask-position", p).catch((e) => {
                  setAskPosition(prev);
                  setToast((e as Error).message);
                });
              }}
              onChange={applyBindings}
              updates={{ meta: updates.meta, canUpdate, check: updates.check }}
              language={language}
              onLanguage={(l) => {
                applyLanguage(l);
                api.setPreference("language", l).catch((e) => setToast((e as Error).message));
              }}
            />}
          </div>
        </main>
      </div>
      <StatusBar
        status={conn.status}
        tasks={tasks}
        usage={usage}
        onFilter={(f) => {
          setStatusFilter((cur) => (cur === f ? null : f));
          setView("board");
        }}
        onLogs={() => setShowLogs(true)}
      />
      {showLogs && <LogsView onClose={() => setShowLogs(false)} />}
      <ToastHost />
      {newTask && (
        <NewTaskDialog
          projects={projects}
          project={project?.name ?? task?.project ?? undefined}
          fromSession={convertFrom ? { id: convertFrom.id, label: convertFrom.label } : undefined}
          existing={starting ?? undefined}
          parentTask={parentTask ?? undefined}
          draft={draft}
          onClose={() => {
            setParentTask(null);
            setNewTask(false);
            setDraft(undefined);
            setConvertFrom(null);
            setStarting(null);
          }}
          onStarted={(t) => {
            setParentTask(null);
            setNewTask(false);
            setDraft(undefined);
            setConvertFrom(null);
            setStarting(null);
            refresh();
            openTask(t.key);
          }}
          onConversation={(name, s) => {
            setParentTask(null);
            setNewTask(false);
            setDraft(undefined);
            openConversation(name, s);
          }}
          onSaved={(saved) => {
            setParentTask(null);
            setNewTask(false);
            setDraft(undefined);
            refresh();
            notify(t("{key} guardada en el backlog de {project}", { key: saved.key, project: saved.project ?? "-" }));
          }}
        />
      )}
      {settingsOf && projects.some((p) => p.name === settingsOf) && (
        <ProjectSettings
          project={projects.find((p) => p.name === settingsOf)!}
          onClose={() => setSettingsOf(null)}
          onSaved={() => {
            setSettingsOf(null);
            refresh();
          }}
        />
      )}
      {deleting && (
        <DeleteTaskDialog
          task={deleting}
          startForced
          onClose={() => setDeleting(null)}
          onDeleted={(key) => {
            setDeleting(null);
            // Off a screen that no longer exists: to its project's, or the board.
            if (selected === key) {
              setSelected(deleting.project ? `@${deleting.project}` : null);
              if (!deleting.project) setView("board");
            }
            refresh();
          }}
        />
      )}
    </div>
  );
}
