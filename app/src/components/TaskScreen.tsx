import { useCallback, useEffect, useId, useMemo, useReducer, useRef, useState, type CSSProperties } from "react";
import { EditorView } from "@codemirror/view";
import type { EditorState } from "@codemirror/state";
import { api, ConflictError, statusLabel, type AskPosition, type ChangeKind, type FileContent, type InboxItem, type Project, type Session, type SessionKind, type SpecSummary, type Task, type TaskView as ViewMode } from "../api";
import { AskCard } from "./AskCard";
import { ConversationView } from "./ConversationView";
import { createState, languageFor, languageSlot } from "../editor/codemirror";
import { CHANGES, OVERVIEW, PRS, persist, sessionFor, sessionOf, sessionTab, setTreeOpen, treeOpen, treeWidth, type Tab } from "../editor/session";
import { t } from "../i18n";
import { on } from "../live";
import { isMarkdown } from "../markdown";
import { comboFromEvent, keyOf } from "../shortcuts";
import { ChangesView } from "./ChangesView";
import { TreeResizer } from "./TreeResizer";
import { FileTree } from "./FileTree";
import { Markdown } from "./Markdown";
import { QuickOpen } from "./QuickOpen";
import { Icon, type IconName } from "./Icon";
import { AutopilotMark } from "./AutopilotMark";
import { PreviewControl } from "./PreviewControl";
import { PhaseBadge } from "./PhaseBadge";
import { SessionChain } from "./SessionChain";
import { headerPhaseOf } from "../phases";
import { isSetupRunning, kindLabel, sessionStatus } from "./sessionLabels";
import { statusIcon } from "./status";
import { PrView } from "./PrView";
import { ProjectView } from "./ProjectView";
import { TaskView } from "./TaskView";
import { SetupBanner } from "./SetupBanner";
import { TerminalPane } from "./TerminalPane";

interface Props {
  /** The task's key, or `@<project>` for a project's own checkout. */
  owner: string;
  /** The task, when the owner is one. */
  task?: Task;
  /** The task's spec, for the phase shown in the header. */
  spec?: SpecSummary;
  /** The project, when the owner is one. */
  project?: Project;
  /** Opens the new task dialog (from a project). */
  onNewTask?: () => void;
  /** "Convertir en tarea" on one of the project's Claude sessions. */
  onConvert?: (session: Session) => void;
  /** Goes to a task, e.g. the one a conversation created. */
  onOpenTask?: (key: string) => void;
  /** The task's open sessions: each one is a tab. */
  sessions: Session[];
  /** Questions with options this task's sessions are waiting on. */
  asks: InboxItem[];
  /** The task's base-stale warnings, shown above the tabs until dismissed. */
  notices: InboxItem[];
  /** Where those questions sit, next to the session's terminal. */
  askPosition: AskPosition;
  onChanged: () => void;
}

const WATCH_MS = 3000;

/**
 * A task as Orca shows a worktree: tabs for the task itself, its sessions (Claude and
 * terminals), its changes and the files opened from the project tree on the right. The agent keeps working in the same
 * files, so open files follow the disk, and a save never overwrites what it wrote in
 * the meantime.
 */
export function TaskScreen({ owner, task, spec, project, sessions, asks, notices, askPosition, onChanged, onNewTask, onConvert, onOpenTask }: Props) {
  const session = useMemo(() => sessionFor(owner), [owner]);
  const host = useRef<HTMLDivElement>(null);
  const view = useRef<EditorView | null>(null);
  const [, redraw] = useReducer((n: number) => n + 1, 0);
  const [error, setError] = useState<string | null>(null);
  const [switching, setSwitching] = useState(false);
  const [closing, setClosing] = useState<string | null>(null);
  const [changed, setChanged] = useState<Map<string, ChangeKind>>(new Map());
  const [treeRefresh, setTreeRefresh] = useState(0);
  const [showTree, setShowTree] = useState(treeOpen);
  const [treeW, setTreeW] = useState(treeWidth);
  const treeId = useId();
  const [closingSession, setClosingSession] = useState<Session | null>(null);
  const [finding, setFinding] = useState(false);
  // A session just started: its tab shows before the next refresh lists it.
  const [starting, setStarting] = useState<string | null>(null);
  // Terminals stay mounted once shown, so switching tabs keeps their scrollback.
  const [mounted, setMounted] = useState<Set<string>>(() => new Set());
  // Per Claude session, the view the user picked with the Chat / Terminal switch; without
  // one, the task's setting decides (a project's sessions open in chat). Kept while the
  // screen is open.
  const [views, setViews] = useState<Map<string, ViewMode>>(() => new Map());
  const viewOf = (s: Session): ViewMode =>
    s.kind === "shell" ? "terminal" : s.runtime === "stream" ? "chat" : (views.get(s.id) ?? (task?.view === "terminal" ? "terminal" : "chat"));
  const showView = (id: string, v: ViewMode) => setViews((cur) => new Map(cur).set(id, v));
  // A project always has its checkout; a task only once its workspace exists.
  const hasWorkspace = !!project || sessions.length > 0;
  // The PR tab: for a task with a workspace (its PR is coming) or with PRs already linked.
  const [hasPrs, setHasPrs] = useState(false);
  const prTab = !project && (hasWorkspace || hasPrs);
  useEffect(() => {
    if (project) return;
    let alive = true;
    const load = () =>
      api
        .taskPrs(owner)
        .then((d) => alive && setHasPrs(d.prs.length > 0))
        .catch(() => {});
    api.refreshPrs(owner).catch(() => {});
    load();
    const off = on("prs", load);
    return () => {
      alive = false;
      off();
    };
  }, [owner, project]);

  const tab = (path: string) => session.tabs.find((t) => t.path === path);

  // The tab on screen: the active one if it still exists (a session may have been
  // closed elsewhere, a file may not have loaded yet), else the task's.
  const activeSession = sessionOf(session.active);
  const exists =
    session.active === OVERVIEW ||
    (session.active === CHANGES && session.changesOpen) ||
    (session.active === PRS && prTab) ||
    !!tab(session.active) ||
    (activeSession !== null && (sessions.some((s) => s.id === activeSession) || activeSession === starting));
  const shown = exists ? session.active : OVERVIEW;
  const shownRef = useRef(shown);
  shownRef.current = shown;
  const active = tab(shown);
  const shownSession = sessionOf(shown);
  const shownClaude = sessions.find((s) => s.id === shownSession && s.kind !== "shell");
  if (shownSession && !mounted.has(shownSession)) setMounted(new Set(mounted).add(shownSession));

  // The live state of a file: the view's for the active one, the stored one otherwise.
  const stateOf = (path: string): EditorState | undefined =>
    path === session.active && view.current ? view.current.state : session.states.get(path);

  const setText = (path: string, f: FileContent) => {
    const t = tab(path);
    if (!t) return;
    t.version = f.version;
    t.size = f.size;
    t.conflict = false;
    t.dirty = false;
    const state = stateOf(path);
    if (f.text === null || !state) {
      t.saved = null;
      return;
    }
    // Replace the text in place: undo history and scroll stay, the cursor is kept in range.
    const head = Math.min(state.selection.main.head, f.text.length);
    const tr = state.update({ changes: { from: 0, to: state.doc.length, insert: f.text }, selection: { anchor: head } });
    // Before dispatching: the change listener compares against it.
    t.saved = tr.state.doc;
    if (path === session.active && view.current) {
      view.current.dispatch(tr);
      view.current.focus();
    } else session.states.set(path, tr.state);
  };

  const save = useCallback(
    async (path: string) => {
      const t = session.tabs.find((x) => x.path === path);
      const state = path === session.active && view.current ? view.current.state : session.states.get(path);
      if (!t || !state || !t.saved) return;
      try {
        const f = await api.saveFile(owner, path, state.doc.toString(), t.version);
        t.version = f.version;
        t.saved = state.doc;
        t.dirty = false;
        t.conflict = false;
        setError(null);
        setTreeRefresh((n) => n + 1);
      } catch (e) {
        if (e instanceof ConflictError) t.conflict = true;
        else setError((e as Error).message);
      }
      redraw();
    },
    [session, owner],
  );
  const saveRef = useRef(save);
  saveRef.current = save;

  const activate = (id: string) => {
    if (tab(session.active) && view.current) session.states.set(session.active, view.current.state);
    session.active = id;
    const state = session.states.get(id);
    if (state && view.current) {
      view.current.setState(state);
      view.current.focus();
    }
    setError(null);
    persist(owner, session);
    redraw();
  };

  const openFile = async (path: string, show = true) => {
    if (!tab(path)) {
      let f: FileContent;
      try {
        f = await api.file(owner, path);
      } catch (e) {
        setError((e as Error).message);
        return;
      }
      // Opened meanwhile by another call (a double click, React's dev double effect).
      if (tab(path)) return show ? activate(path) : undefined;
      // A Markdown file that was shown rendered last time comes back rendered.
      const preview = isMarkdown(path) && session.pendingPreview.delete(path);
      const t: Tab = { path, version: f.version, size: f.size, saved: null, dirty: false, conflict: false, preview };
      if (f.text !== null) {
        const state = createState(f.text, {
          onSave: () => saveRef.current(path),
          onChange: (s) => {
            const t = session.tabs.find((x) => x.path === path);
            if (!t || !t.saved) return;
            const dirty = !s.doc.eq(t.saved);
            if (dirty !== t.dirty) {
              t.dirty = dirty;
              redraw();
            }
          },
        });
        t.saved = state.doc;
        session.states.set(path, state);
        languageFor(path).then((lang) => {
          if (!lang) return;
          const effects = languageSlot.reconfigure(lang);
          if (session.active === path && view.current) view.current.dispatch({ effects });
          else {
            const s = session.states.get(path);
            if (s) session.states.set(path, s.update({ effects }).state);
          }
        });
      }
      session.tabs.push(t);
    }
    if (!show) return;
    // Reveal it in the tree.
    for (let i = path.indexOf("/"); i > 0; i = path.indexOf("/", i + 1)) session.expanded.add(path.slice(0, i));
    activate(path);
  };

  const sessionById = (id: string | null) => sessions.find((s) => s.id === id);

  /** Starts a Claude or a terminal in the workspace and shows its tab. */
  const newSession = async (kind: SessionKind, prompt?: string, label?: string, conversation?: boolean) => {
    const created = project
      ? await api.openProjectSession(project.name, { kind, prompt, label, ...(conversation ? { conversation } : {}) })
      : await api.openSession(owner, { kind, prompt, label });
    setStarting(created.id);
    activate(sessionTab(created.id));
    onChanged();
  };
  const showSession = (id?: string) => {
    const s = id ? sessionById(id) : sessions.find((x) => x.primary);
    if (s) activate(sessionTab(s.id));
  };

  // After a handoff the new primary's terminal is the one to look at.
  const primaryId = task?.saving_mode ? (sessions.find((s) => s.primary)?.id ?? null) : null;
  const lastPrimary = useRef({ owner, primaryId });
  useEffect(() => {
    const before = lastPrimary.current;
    lastPrimary.current = { owner, primaryId };
    // Only a new primary of the same task: another task's is not a handoff.
    if (before.owner === owner && before.primaryId && primaryId && before.primaryId !== primaryId) {
      activate(sessionTab(primaryId));
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [owner, primaryId]);

  const setSaving = (on: boolean) =>
    api
      .setSavingMode(owner, on)
      .then(onChanged)
      .catch((e) => setError((e as Error).message));

  const [editing, setEditing] = useState<string | null>(null);
  const saveTitle = () => {
    const title = (editing ?? "").trim();
    setEditing(null);
    if (!task || !title || title === task.title) return;
    api.updateTask(owner, { title }).then(onChanged).catch((e) => setError((e as Error).message));
  };
  const restoreTitle = () =>
    task?.previous_title &&
    api
      .updateTask(owner, { title: task.previous_title })
      .then(onChanged)
      .catch((e) => setError((e as Error).message));
  const [autopilotBusy, setAutopilotBusy] = useState(false);
  const setAutopilotMode = (action: "on" | "off") => {
    setAutopilotBusy(true);
    api
      .setAutopilot(owner, action)
      .then(onChanged)
      .catch((e) => setError((e as Error).message))
      .finally(() => setAutopilotBusy(false));
  };

  const openChanges = () => {
    session.changesOpen = true;
    activate(CHANGES);
  };

  // Tabs in strip order: the task, its sessions, the changes, the files.
  const order = () => [
    OVERVIEW,
    ...sessions.map((s) => sessionTab(s.id)),
    ...(starting && !sessionById(starting) ? [sessionTab(starting)] : []),
    ...(session.changesOpen ? [CHANGES] : []),
    ...(prTab ? [PRS] : []),
    ...session.tabs.map((t) => t.path),
  ];

  const close = async (id: string, force = false) => {
    if (id === OVERVIEW) return;
    const strip = order();
    const at = strip.indexOf(id);
    const sid = sessionOf(id);
    if (sid) {
      const s = sessionById(sid);
      if (!s) return;
      if (s.primary) {
        setError(t("La sesión principal no se cierra: termina con la tarea (Marcar como hecha)."));
        return;
      }
      // Closing a working Claude stops it: ask first. A terminal just closes.
      if (s.kind === "claude" && s.live && !force) return setClosingSession(s);
      setClosingSession(null);
      try {
        await api.closeSession(s.id);
      } catch (e) {
        setError((e as Error).message);
        return;
      }
      onChanged();
    } else if (id === CHANGES) session.changesOpen = false;
    else {
      const t = tab(id);
      if (!t) return;
      if (t.dirty && !force) return setClosing(id);
      if (session.active === id && view.current) session.states.set(id, view.current.state);
      session.tabs.splice(session.tabs.indexOf(t), 1);
      session.states.delete(id);
    }
    setClosing(null);
    if (session.active === id) {
      session.active = OVERVIEW;
      activate(strip[at + 1] ?? strip[at - 1] ?? OVERVIEW);
    } else {
      persist(owner, session);
      redraw();
    }
  };

  // A Markdown file shows its source or the rendered document. The editor state stays as
  // it is, so going back keeps the cursor, the unsaved edits and the undo history.
  const togglePreview = () => {
    const t = tab(session.active);
    if (!t || !t.saved || !isMarkdown(t.path)) return;
    if (view.current) session.states.set(t.path, view.current.state);
    t.preview = !t.preview;
    persist(owner, session);
    redraw();
    // Once the editor is shown again.
    if (!t.preview) requestAnimationFrame(() => view.current?.focus());
  };

  // The files that were open last time, loaded in their tab order.
  useEffect(() => {
    const paths = session.pending.splice(0);
    if (paths.length === 0) return;
    (async () => {
      for (const p of paths) await openFile(p, false);
      if (tab(session.active)) activate(session.active);
      else redraw();
    })();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [session]);

  // One CodeMirror view, its state swapped as tabs change.
  useEffect(() => {
    if (!host.current) return;
    const v = new EditorView({ parent: host.current });
    view.current = v;
    const state = session.states.get(session.active);
    if (state) v.setState(state);
    return () => {
      if (session.states.has(session.active)) session.states.set(session.active, v.state);
      v.destroy();
      view.current = null;
    };
  }, [session]);

  // ⌘S lives in the editor's keymap, and the editor is hidden while a preview shows:
  // the preview saves too, so its "sin guardar · ⌘S" stays true.
  const previewing = !!tab(session.active)?.preview;
  useEffect(() => {
    if (!previewing) return;
    const onKey = (e: KeyboardEvent) => {
      if (comboFromEvent(e) !== "Mod+S") return;
      e.preventDefault();
      saveRef.current(session.active);
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [previewing, session]);

  // The app sends the shortcuts that act on tabs: ⌘W closes the active one, ⌘T and
  // ⌘⇧T start a terminal or a Claude, ⌃Tab moves between tabs, ⌘B toggles the tree,
  // ⌘⇧V toggles a Markdown file's preview.
  const closeRef = useRef(close);
  closeRef.current = close;
  const previewRef = useRef(togglePreview);
  previewRef.current = togglePreview;
  const newRef = useRef(newSession);
  newRef.current = newSession;
  const cycleRef = useRef<(step: number) => void>(() => {});
  cycleRef.current = (step: number) => {
    const strip = order();
    const at = Math.max(0, strip.indexOf(shown));
    activate(strip[(at + step + strip.length) % strip.length]);
  };
  useEffect(() => {
    const onNew = (e: Event) => {
      const kind = (e as CustomEvent<SessionKind>).detail;
      newRef.current(kind).catch((err) => setError((err as Error).message));
    };
    const onCycle = (e: Event) => cycleRef.current((e as CustomEvent<number>).detail);
    const onFind = () => setFinding(true);
    window.addEventListener("delilah:new-session", onNew);
    window.addEventListener("delilah:cycle-tab", onCycle);
    window.addEventListener("delilah:quick-open", onFind);
    return () => {
      window.removeEventListener("delilah:new-session", onNew);
      window.removeEventListener("delilah:cycle-tab", onCycle);
      window.removeEventListener("delilah:quick-open", onFind);
    };
  }, []);
  useEffect(() => {
    const onClose = () => closeRef.current(shownRef.current);
    const onTree = () =>
      setShowTree((open) => {
        setTreeOpen(!open);
        return !open;
      });
    const onPreview = () => previewRef.current();
    window.addEventListener("delilah:close-tab", onClose);
    window.addEventListener("delilah:toggle-tree", onTree);
    window.addEventListener("delilah:toggle-preview", onPreview);
    return () => {
      window.removeEventListener("delilah:close-tab", onClose);
      window.removeEventListener("delilah:toggle-tree", onTree);
      window.removeEventListener("delilah:toggle-preview", onPreview);
    };
  }, [session]);

  // Follow the disk: reload the active file if the agent changed it, or flag the
  // conflict if it has unsaved edits here.
  const check = async () => {
    const t = tab(session.active);
    if (!t || t.conflict) return;
    try {
      const f = await api.file(owner, t.path);
      if (f.version === t.version || tab(t.path) !== t) return;
      if (t.dirty) t.conflict = true;
      else setText(t.path, f);
      redraw();
    } catch {
      // Deleted, or the daemon is away: leave the tab as it is.
    }
  };
  // The app re-renders every couple of seconds: the timer must not restart with it.
  const checkRef = useRef(check);
  checkRef.current = check;
  useEffect(() => {
    const run = () => checkRef.current();
    const timer = setInterval(run, WATCH_MS);
    window.addEventListener("focus", run);
    return () => {
      clearInterval(timer);
      window.removeEventListener("focus", run);
    };
  }, []);

  // What the branch changed, to mark it in the tree.
  useEffect(() => {
    if (!hasWorkspace) return;
    let alive = true;
    api
      .changes(owner)
      .then((repos) => {
        if (!alive) return;
        const m = new Map<string, ChangeKind>();
        for (const r of repos) for (const f of r.files) m.set(r.repo === "." ? f.path : `${r.repo}/${f.path}`, f.kind);
        setChanged(m);
      })
      .catch(() => {});
    return () => {
      alive = false;
    };
  }, [owner, treeRefresh, hasWorkspace]);

  const keepMine = async () => {
    if (!active) return;
    const f = await api.file(owner, active.path);
    active.version = f.version;
    active.conflict = false;
    await save(active.path);
    view.current?.focus();
  };
  const takeDisk = async () => {
    if (!active) return;
    setText(active.path, await api.file(owner, active.path));
    redraw();
  };

  const switchRuntime = (mode: "chat" | "terminal") => {
    setSwitching(true);
    setError(null);
    api
      .setRuntime(owner, mode)
      .then(() => {
        // The sessions come back in the new runtime: to the terminal, show it (the old
        // choice was a chat the session no longer has); to chat, drop any override.
        setViews(mode === "terminal" ? new Map(sessions.filter((x) => x.kind === "claude").map((x) => [x.id, "terminal" as ViewMode])) : new Map());
        onChanged();
      }, (err) => setError((err as Error).message))
      .finally(() => setSwitching(false));
  };

  const name = (path: string) => path.slice(path.lastIndexOf("/") + 1);
  const toggleTree = () => window.dispatchEvent(new Event("delilah:toggle-tree"));
  const status = task ? statusIcon(task.status) : null;
  const phase = task ? headerPhaseOf(task, spec) : null;

  return (
    <>
      <div className="view-head" data-tauri-drag-region>
        {task && status ? (
          <>
            <span className="mono muted">{owner}</span>
            {editing === null ? (
              <h1
                data-tauri-drag-region
                className="trunc"
                title={t("Clic para renombrar")}
                onClick={() => setEditing(task.title)}
              >
                {task.title}
              </h1>
            ) : (
              <input
                autoFocus
                aria-label={t("Título de la tarea")}
                value={editing}
                onChange={(e) => setEditing(e.target.value)}
                onBlur={() => setEditing(null)}
                onKeyDown={(e) => {
                  if (e.key === "Enter") saveTitle();
                  else if (e.key === "Escape") setEditing(null);
                }}
              />
            )}
            {task.previous_title && (
              <span className="muted trunc">
                {t("Antes: {title}", { title: task.previous_title })}{" "}
                <button className="saving-off" onClick={restoreTitle}>{t("Restaurar")}</button>
              </span>
            )}
            <span className="head-pills">
              {phase && <PhaseBadge phase={phase} />}
              <AutopilotMark autopilot={task.autopilot} />
              {task.chat_mode && <span className="pill chat-mode-pill">{t("experimental: modo chat")}</span>}
              {task.status.type !== "done" && (
                <button
                  className="btn btn-ghost saving-on"
                  disabled={switching}
                  title={task.chat_mode ? undefined : t("Experimental: la tarea sigue en modo chat, sin terminal")}
                  onClick={() => switchRuntime(task.chat_mode ? "terminal" : "chat")}
                >
                  {task.chat_mode ? t("Pasar a terminal") : t("Pasar a modo chat")}
                </button>
              )}
              <PreviewControl taskKey={owner} project={task.project} status={task.status} />
              {task.workflow === "feature" && task.status.type !== "done" && task.autopilot?.state === "running" && (
                <button className="saving-off" disabled={autopilotBusy} title={t("Desactiva el autopiloto y sigue a mano")} onClick={() => setAutopilotMode("off")}>
                  {t("Desactivar")}
                </button>
              )}
              {task.saving_mode && (
                <span className="pill saving-pill">
                  {t("Modo ahorro")}
                  {task.status.type !== "done" && (
                    <button className="saving-off" onClick={() => setSaving(false)}>{t("Desactivar")}</button>
                  )}
                </span>
              )}
              <select
                className="input view-select"
                aria-label={t("Vista")}
                title={t("Vista")}
                value={task.view === "terminal" ? "terminal" : "chat"}
                onChange={(e) => {
                  const v = e.target.value as ViewMode;
                  setViews(new Map());
                  api.updateTask(owner, { view: v }).then(onChanged, (err) => setError((err as Error).message));
                }}
              >
                <option value="chat">Chat</option>
                <option value="terminal">Terminal</option>
              </select>
              <span className={`pill ${status.cls}`}>
                <Icon name={status.icon} /> {statusLabel(task.status)}
              </span>
              {task.workflow === "feature" && task.status.type !== "done" && !task.autopilot && (
                <button className="btn btn-ghost saving-on" disabled={autopilotBusy} title={t("Activa el autopiloto: la cadena sigue sola hasta abrir las PRs")} onClick={() => setAutopilotMode("on")}>
                  {t("Activar autopiloto")}
                </button>
              )}
              {!task.saving_mode && task.status.type !== "done" && (
                <button className="btn btn-ghost saving-on" onClick={() => setSaving(true)}>
                  {t("Activar modo ahorro")}
                </button>
              )}
            </span>
          </>
        ) : (
          <>
            <Icon name="folder" />
            <h1 data-tauri-drag-region className="trunc">{project?.name}</h1>
            <span className="mono muted trunc" style={{ marginRight: "auto" }}>{project?.path}</span>
          </>
        )}
        {hasWorkspace && (
          <button
            className={`btn btn-ghost ${showTree ? "is-on" : ""}`}
            onClick={toggleTree}
            title={t("Árbol de archivos ({key})", { key: keyOf("toggle-tree") })}
            aria-pressed={showTree}
          >
            <Icon name="sidebar" />
          </button>
        )}
      </div>
      <div
        className={`tscreen ${showTree && hasWorkspace ? "with-tree" : ""}`}
        style={{ "--tree-w": `${treeW}px` } as CSSProperties}
      >
        <div className="tmain">
          {task && <SessionChain task={task} sessions={sessions} onOpenSession={showSession} onChanged={onChanged} />}
          {notices.map((n) => (
            <div key={n.notice} className="ebar warn base-stale">
              <span style={{ flex: 1 }}>{n.text}</span>
              {n.notice != null && (
                <button
                  className="btn btn-ghost"
                  onClick={() => api.dismissNotice(n.notice!).then(onChanged, () => {})}
                >
                  {t("Descartar")}
                </button>
              )}
            </div>
          ))}
          <div className="etabs" role="tablist">
            <TabButton id={OVERVIEW} label={project ? t("Proyecto") : t("Tarea")} icon={project ? "folder" : "tasks"} shown={shown} onActivate={activate} />
            {sessions.map((s) => {
              const st = sessionStatus(s, task);
              return (
                <TabButton
                  key={s.id}
                  id={sessionTab(s.id)}
                  label={s.label}
                  title={`${s.label} · ${kindLabel(s)} · ${st.label}`}
                  icon={s.kind === "claude" ? st.icon : "console"}
                  iconClass={s.kind === "claude" ? st.cls : ""}
                  shown={shown}
                  onActivate={activate}
                  onClose={s.primary ? undefined : close}
                />
              );
            })}
            {starting && !sessionById(starting) && (
              <TabButton id={sessionTab(starting)} label={t("Arrancando…")} icon="run" shown={shown} onActivate={activate} />
            )}
            {session.changesOpen && (
              <TabButton id={CHANGES} label={t("Cambios")} icon="diff" shown={shown} onActivate={activate} onClose={close} />
            )}
            {prTab && <TabButton id={PRS} label="PR" icon="review" shown={shown} onActivate={activate} />}
            {session.tabs.map((t) => (
              <TabButton
                key={t.path}
                id={t.path}
                label={name(t.path)}
                title={t.path}
                dirty={t.dirty}
                shown={shown}
                onActivate={activate}
                onClose={close}
              />
            ))}
            {shownClaude && (
              <span className="segs view-switch" style={{ marginLeft: "auto", alignSelf: "center", marginRight: 8 }}>
                {(["chat", "terminal"] as const).map((v) => (
                  <button
                    key={v}
                    className={`seg ${viewOf(shownClaude) === v ? "on" : ""}`}
                    aria-pressed={viewOf(shownClaude) === v}
                    disabled={v === "terminal" && shownClaude.runtime === "stream"}
                    title={v === "terminal" && shownClaude.runtime === "stream" ? t("Esta sesión va en modo chat: no hay terminal") : undefined}
                    onClick={() => showView(shownClaude.id, v)}
                  >
                    {v === "chat" ? "Chat" : "Terminal"}
                  </button>
                ))}
              </span>
            )}
          </div>

          {shown === OVERVIEW && project && (
            <ProjectView
              project={project}
              sessions={sessions}
              onOpenSession={showSession}
              onNewSession={newSession}
              onNewIdea={() => newSession("claude", undefined, undefined, true)}
              onNewTask={onNewTask}
            />
          )}
          {shown === OVERVIEW && task && (
            <TaskView
              task={task}
              sessions={sessions}
              onOpenSession={showSession}
              onNewSession={newSession}
              onChanged={onChanged}
              onViewChanges={openChanges}
            />
          )}
          {shown === PRS && <PrView owner={owner} />}
          {shown === CHANGES && <ChangesView owner={owner} onEdit={openFile} />}

          {closingSession && (
            <div className="ebar warn">
              <span style={{ flex: 1 }}>
                {t("Cerrar «{label}» detiene ese Claude. Su conversación no se podrá reabrir desde aquí.", { label: closingSession.label })}
              </span>
              <button className="btn btn-p" onClick={() => close(sessionTab(closingSession.id), true)}>{t("Cerrar")}</button>
              <button className="btn btn-ghost" onClick={() => setClosingSession(null)}>{t("Cancelar")}</button>
            </div>
          )}
          {shownSession && !sessionById(shownSession) && <div className="empty">{t("Arrancando la sesión…")}</div>}
          {sessions
            .filter((s) => mounted.has(s.id))
            .map((s) => {
              const on = shownSession === s.id;
              const sessionAsks = asks.filter((a) => a.session?.id === s.id && a.ask);
              const asConversation = viewOf(s) === "chat" && s.kind !== "shell";
              const reopen = () =>
                api
                  .reopenSession(s.id)
                  .then(onChanged)
                  .catch((e) => setError((e as Error).message));
              return (
                <div key={s.id} className="tterm" style={{ display: on ? "flex" : "none" }}>
                  {s.kind === "setup" && s.live && task && (
                    <SetupBanner task={task} running={isSetupRunning(sessions)} onChanged={onChanged} />
                  )}
                  {!asConversation && s.read_only && s.kind === "claude" && s.live && (
                    <div className="ebar warn">
                      <Icon name="lock" />
                      <span style={{ flex: 1 }}>
                        <b>{t("Solo lectura.")}</b> {t("Este Claude lee, busca y explica, pero no edita.")}
                      </span>
                      <button
                        className="btn"
                        onClick={() =>
                          api
                            .allowEdits(s.id)
                            .then(onChanged)
                            .catch((e) => setError((e as Error).message))
                        }
                      >
                        {t("Permitir cambios en esta sesión")}
                      </button>
                      {onConvert && (
                        <button className="btn" onClick={() => onConvert(s)}>
                          {t("Convertir en tarea")}
                        </button>
                      )}
                    </div>
                  )}
                  {!asConversation && !s.read_only && s.kind === "claude" && s.live && project && onConvert && (
                    <div className="ebar">
                      <span style={{ flex: 1 }} className="muted">
                        {t("Claude sobre tu checkout, sin tarea.")}
                      </span>
                      <button className="btn" onClick={() => onConvert(s)}>
                        {t("Convertir en tarea")}
                      </button>
                    </div>
                  )}
                  {!s.live && !asConversation && (
                    <div className="ebar">
                      <span style={{ flex: 1 }}>{t("Esta sesión no está en marcha (el daemon se reinició o el proceso terminó).")}</span>
                      <button
                        className="btn btn-p"
                        onClick={reopen}
                      >
                        {t("Reabrir")}
                      </button>
                    </div>
                  )}
                  {/* Fixed order, placed by CSS: moving the questions never remounts the
                      terminal or loses an answer being written. */}
                  <div className={`tterm-body ask-${askPosition}`}>
                    {/* A new connection once it comes back to life. */}
                    {asConversation ? (
                      <ConversationView
                        session={s}
                        onOpenTerminal={() => showView(s.id, "terminal")}
                        onOpenFile={(path) => void openFile(path)}
                        onOpenTask={(key) => onOpenTask?.(key)}
                        onReopen={reopen}
                      />
                    ) : (
                      <TerminalPane key={`${s.id}:${s.live}:${s.read_only}`} sessionId={s.id} kind={s.kind} focused={on} />
                    )}
                    {!asConversation && sessionAsks.length > 0 && (
                      <div className="ask-bar">
                        <div className="sec">{t("El agente te pregunta")}</div>
                        {sessionAsks.map((a) => (
                          <AskCard key={a.ask!.id} ask={a.ask!} onDone={onChanged} onOpenTask={onOpenTask} />
                        ))}
                      </div>
                    )}
                  </div>
                </div>
              );
            })}

          {active && (
            <div className="epath mono muted">
              <span className="selectable trunc">{active.path}</span>
              {active.dirty && <span className="c-wait">{t("sin guardar · ⌘S")}</span>}
              {active.saved && isMarkdown(active.path) && (
                <button
                  className={`btn btn-ghost ${active.preview ? "is-on" : ""}`}
                  style={{ marginLeft: "auto", height: 20 }}
                  onClick={togglePreview}
                  title={t("Alternar vista previa ({key})", { key: keyOf("toggle-preview") })}
                  aria-pressed={active.preview}
                >
                  {active.preview ? t("Código") : t("Vista previa")}
                </button>
              )}
            </div>
          )}
          {closing && (
            <div className="ebar">
              <span style={{ flex: 1 }}>
                {t("«{name}» tiene cambios sin guardar.", { name: name(closing) })}
              </span>
              <button className="btn btn-p" onClick={async () => (await save(closing), close(closing, true))}>{t("Guardar")}</button>
              <button className="btn" onClick={() => close(closing, true)}>{t("Descartar")}</button>
              <button className="btn btn-ghost" onClick={() => setClosing(null)}>{t("Cancelar")}</button>
            </div>
          )}
          {active?.conflict && (
            <div className="ebar warn">
              <span style={{ flex: 1 }}>
                {t("El archivo ha cambiado en disco (probablemente el agente) y aquí tienes cambios sin guardar.")}
              </span>
              <button className="btn" onClick={takeDisk}>{t("Cargar el del disco")}</button>
              <button className="btn" onClick={keepMine}>{t("Guardar el mío encima")}</button>
            </div>
          )}
          {error && <div className="error selectable" style={{ padding: "8px 14px" }}>{error}</div>}
          {/* Always mounted: one CodeMirror view for every file tab. */}
          <div className="ehost" ref={host} style={{ display: active?.saved && !active.preview ? "block" : "none" }} />
          {/* The live text, unsaved edits included; a reload from disk redraws it. */}
          {active?.saved && active.preview && (
            <div className="md-preview">
              <Markdown
                text={(stateOf(active.path)?.doc ?? active.saved).toString()}
                doc={{
                  path: active.path,
                  version: active.version,
                  openFile,
                  loadImage: (path) => api.image(owner, path),
                }}
              />
            </div>
          )}
          {active && !active.saved && (
            <div className="empty">
              <span>
                {active.size > 1 << 20 ? t("Este archivo es demasiado grande para editarlo aquí.") : t("Este archivo es binario y no se puede editar aquí.")}
              </span>
            </div>
          )}
        </div>

        {finding && hasWorkspace && <QuickOpen taskKey={owner} onOpen={openFile} onClose={() => setFinding(false)} />}
        {showTree && hasWorkspace && (
          <aside id={treeId} className="tpanel" aria-label={t("Archivos del workspace")}>
            <TreeResizer width={treeW} controls={treeId} onChange={setTreeW} />
            <div className="tpanel-head">
              <span className="sec">{t("Archivos")}</span>
              <button className="btn btn-ghost icon-btn mono" onClick={() => setFinding(true)} title={t("Abrir un archivo por su nombre")}>
                {keyOf("quick-open")}
              </button>
              <button className="btn btn-ghost icon-btn" onClick={() => setTreeRefresh((n) => n + 1)} title={t("Refrescar")} aria-label={t("Actualizar el árbol")}>
                <Icon name="refresh" />
              </button>
              <button className="btn btn-ghost icon-btn" onClick={toggleTree} title={t("Ocultar ({key})", { key: keyOf("toggle-tree") })} aria-label={t("Ocultar el árbol")}>
                <Icon name="close" />
              </button>
            </div>
            <FileTree
              taskKey={owner}
              changed={changed}
              active={tab(session.active) ? session.active : null}
              expanded={session.expanded}
              refresh={treeRefresh}
              onOpen={openFile}
            />
          </aside>
        )}
      </div>
    </>
  );
}

function TabButton({
  id,
  label,
  title,
  icon,
  iconClass,
  dirty,
  shown,
  onActivate,
  onClose,
}: {
  id: string;
  label: string;
  title?: string;
  icon?: IconName;
  iconClass?: string;
  dirty?: boolean;
  /** The tab on screen. */
  shown: string;
  onActivate: (id: string) => void;
  onClose?: (id: string) => void;
}) {
  const on = shown === id;
  return (
    <div
      role="tab"
      aria-selected={on}
      className={`etab ${on ? "on" : ""}`}
      title={title ?? label}
      onClick={() => onActivate(id)}
      onAuxClick={(e) => e.button === 1 && onClose?.(id)}
    >
      {icon && <Icon name={icon} className={iconClass} />}
      <span className="trunc">{label}</span>
      {onClose && (
        <button
          className={`etab-x ${dirty ? "dirty" : ""}`}
          aria-label={t("Cerrar {label}", { label })}
          onClick={(e) => {
            e.stopPropagation();
            onClose(id);
          }}
        >
          <span className="dot-dirty" />
          <Icon name="close" />
        </button>
      )}
    </div>
  );
}
