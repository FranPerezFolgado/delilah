import { useCallback, useEffect, useMemo, useReducer, useRef, useState } from "react";
import { EditorView } from "@codemirror/view";
import type { EditorState } from "@codemirror/state";
import { api, ConflictError, statusLabel, type ChangeKind, type FileContent, type InboxItem, type Project, type Session, type SessionKind, type Task } from "../api";
import { AskCard } from "./AskCard";
import { createState, languageFor, languageSlot } from "../editor/codemirror";
import { CHANGES, OVERVIEW, persist, sessionFor, sessionOf, sessionTab, setTreeOpen, treeOpen, type Tab } from "../editor/session";
import { keyOf } from "../shortcuts";
import { ChangesView } from "./ChangesView";
import { FileTree } from "./FileTree";
import { QuickOpen } from "./QuickOpen";
import { Icon, type IconName } from "./Icon";
import { sessionStatus } from "./sessionLabels";
import { statusIcon } from "./status";
import { ProjectView } from "./ProjectView";
import { TaskView } from "./TaskView";
import { TerminalPane } from "./TerminalPane";

interface Props {
  /** The task's key, or `@<project>` for a project's own checkout. */
  owner: string;
  /** The task, when the owner is one. */
  task?: Task;
  /** The project, when the owner is one. */
  project?: Project;
  /** Opens the new task dialog (from a project). */
  onNewTask?: () => void;
  /** "Convertir en tarea" on one of the project's Claude sessions. */
  onConvert?: (session: Session) => void;
  /** The task's open sessions: each one is a tab. */
  sessions: Session[];
  /** Questions with options this task's sessions are waiting on. */
  asks: InboxItem[];
  onChanged: () => void;
}

const WATCH_MS = 3000;

/**
 * A task as Orca shows a worktree: tabs for the task itself, its sessions (Claude and
 * terminals), its changes and the files opened from the project tree on the right. The agent keeps working in the same
 * files, so open files follow the disk, and a save never overwrites what it wrote in
 * the meantime.
 */
export function TaskScreen({ owner, task, project, sessions, asks, onChanged, onNewTask, onConvert }: Props) {
  const session = useMemo(() => sessionFor(owner), [owner]);
  const host = useRef<HTMLDivElement>(null);
  const view = useRef<EditorView | null>(null);
  const [, redraw] = useReducer((n: number) => n + 1, 0);
  const [error, setError] = useState<string | null>(null);
  const [closing, setClosing] = useState<string | null>(null);
  const [changed, setChanged] = useState<Map<string, ChangeKind>>(new Map());
  const [treeRefresh, setTreeRefresh] = useState(0);
  const [showTree, setShowTree] = useState(treeOpen);
  const [closingSession, setClosingSession] = useState<Session | null>(null);
  const [finding, setFinding] = useState(false);
  // A session just started: its tab shows before the next refresh lists it.
  const [starting, setStarting] = useState<string | null>(null);
  // Terminals stay mounted once shown, so switching tabs keeps their scrollback.
  const [mounted, setMounted] = useState<Set<string>>(() => new Set());
  // A project always has its checkout; a task only once its workspace exists.
  const hasWorkspace = !!project || sessions.length > 0;

  const tab = (path: string) => session.tabs.find((t) => t.path === path);

  // The tab on screen: the active one if it still exists (a session may have been
  // closed elsewhere, a file may not have loaded yet), else the task's.
  const activeSession = sessionOf(session.active);
  const exists =
    session.active === OVERVIEW ||
    (session.active === CHANGES && session.changesOpen) ||
    !!tab(session.active) ||
    (activeSession !== null && (sessions.some((s) => s.id === activeSession) || activeSession === starting));
  const shown = exists ? session.active : OVERVIEW;
  const shownRef = useRef(shown);
  shownRef.current = shown;
  const active = tab(shown);
  const shownSession = sessionOf(shown);
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
      const t: Tab = { path, version: f.version, size: f.size, saved: null, dirty: false, conflict: false };
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
  const newSession = async (kind: SessionKind, prompt?: string, label?: string) => {
    const created = project
      ? await api.openProjectSession(project.name, { kind, prompt, label })
      : await api.openSession(owner, { kind, prompt, label });
    setStarting(created.id);
    activate(sessionTab(created.id));
    onChanged();
  };
  const showSession = (id?: string) => {
    const s = id ? sessionById(id) : sessions.find((x) => x.primary);
    if (s) activate(sessionTab(s.id));
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
        setError("La sesión principal no se cierra: termina con la tarea (Marcar como hecha).");
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

  // The app sends the shortcuts that act on tabs: ⌘W closes the active one, ⌘T and
  // ⌘⇧T start a terminal or a Claude, ⌃Tab moves between tabs, ⌘B toggles the tree.
  const closeRef = useRef(close);
  closeRef.current = close;
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
    window.addEventListener("delilah:close-tab", onClose);
    window.addEventListener("delilah:toggle-tree", onTree);
    return () => {
      window.removeEventListener("delilah:close-tab", onClose);
      window.removeEventListener("delilah:toggle-tree", onTree);
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

  const name = (path: string) => path.slice(path.lastIndexOf("/") + 1);
  const toggleTree = () => window.dispatchEvent(new Event("delilah:toggle-tree"));
  const status = task ? statusIcon(task.status) : null;

  return (
    <>
      <div className="view-head" data-tauri-drag-region>
        {task && status ? (
          <>
            <span className="mono muted">{owner}</span>
            <h1 data-tauri-drag-region className="trunc">{task.title}</h1>
            <span className={`pill ${status.cls}`} style={{ marginLeft: "auto" }}>
              <Icon name={status.icon} /> {statusLabel(task.status)}
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
            title={`Árbol de archivos (${keyOf("toggle-tree")})`}
            aria-pressed={showTree}
          >
            <Icon name="sidebar" />
          </button>
        )}
      </div>
      <div className={`tscreen ${showTree && hasWorkspace ? "with-tree" : ""}`}>
        <div className="tmain">
          <div className="etabs" role="tablist">
            <TabButton id={OVERVIEW} label={project ? "Proyecto" : "Tarea"} icon={project ? "folder" : "tasks"} shown={shown} onActivate={activate} />
            {sessions.map((s) => {
              const st = sessionStatus(s, task);
              return (
                <TabButton
                  key={s.id}
                  id={sessionTab(s.id)}
                  label={s.label}
                  title={`${s.label} · ${s.kind === "shell" ? "terminal" : "Claude"} · ${st.label}`}
                  icon={s.kind === "shell" ? "console" : st.icon}
                  iconClass={s.kind === "shell" ? "" : st.cls}
                  shown={shown}
                  onActivate={activate}
                  onClose={s.primary ? undefined : close}
                />
              );
            })}
            {starting && !sessionById(starting) && (
              <TabButton id={sessionTab(starting)} label="Arrancando…" icon="run" shown={shown} onActivate={activate} />
            )}
            {session.changesOpen && (
              <TabButton id={CHANGES} label="Cambios" icon="diff" shown={shown} onActivate={activate} onClose={close} />
            )}
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
          </div>

          {shown === OVERVIEW && project && (
            <ProjectView
              project={project}
              sessions={sessions}
              onOpenSession={showSession}
              onNewSession={newSession}
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
          {shown === CHANGES && <ChangesView owner={owner} onEdit={openFile} />}

          {closingSession && (
            <div className="ebar warn">
              <span style={{ flex: 1 }}>
                Cerrar <b>{closingSession.label}</b> detiene ese Claude. Su conversación no se podrá reabrir desde aquí.
              </span>
              <button className="btn btn-p" onClick={() => close(sessionTab(closingSession.id), true)}>Cerrar</button>
              <button className="btn btn-ghost" onClick={() => setClosingSession(null)}>Cancelar</button>
            </div>
          )}
          {shownSession && !sessionById(shownSession) && <div className="empty">Arrancando la sesión…</div>}
          {sessions
            .filter((s) => mounted.has(s.id))
            .map((s) => {
              const on = shownSession === s.id;
              return (
                <div key={s.id} className="tterm" style={{ display: on ? "flex" : "none" }}>
                  {s.read_only && s.kind === "claude" && s.live && (
                    <div className="ebar warn">
                      <Icon name="lock" />
                      <span style={{ flex: 1 }}>
                        <b>Solo lectura.</b> Este Claude lee, busca y explica, pero no edita.
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
                        Permitir cambios en esta sesión
                      </button>
                      {onConvert && (
                        <button className="btn" onClick={() => onConvert(s)}>
                          Convertir en tarea
                        </button>
                      )}
                    </div>
                  )}
                  {!s.read_only && s.kind === "claude" && s.live && project && onConvert && (
                    <div className="ebar">
                      <span style={{ flex: 1 }} className="muted">
                        Claude sobre tu checkout, sin tarea.
                      </span>
                      <button className="btn" onClick={() => onConvert(s)}>
                        Convertir en tarea
                      </button>
                    </div>
                  )}
                  {asks
                    .filter((a) => a.session?.id === s.id && a.ask)
                    .map((a) => (
                      <div key={a.ask!.id} className="ask-bar">
                        <div className="sec" style={{ marginBottom: 6 }}>El agente te pregunta</div>
                        <AskCard ask={a.ask!} onDone={onChanged} />
                      </div>
                    ))}
                  {!s.live && (
                    <div className="ebar">
                      <span style={{ flex: 1 }}>Esta sesión no está en marcha (el daemon se reinició o el proceso terminó).</span>
                      <button
                        className="btn btn-p"
                        onClick={() =>
                          api
                            .reopenSession(s.id)
                            .then(onChanged)
                            .catch((e) => setError((e as Error).message))
                        }
                      >
                        Reabrir
                      </button>
                    </div>
                  )}
                  {/* A new connection once it comes back to life. */}
                  <TerminalPane key={`${s.id}:${s.live}:${s.read_only}`} sessionId={s.id} focused={on} />
                </div>
              );
            })}

          {active && (
            <div className="epath mono muted">
              <span className="selectable trunc">{active.path}</span>
              {active.dirty && <span className="c-wait">sin guardar · ⌘S</span>}
            </div>
          )}
          {closing && (
            <div className="ebar">
              <span style={{ flex: 1 }}>
                <b>{name(closing)}</b> tiene cambios sin guardar.
              </span>
              <button className="btn btn-p" onClick={async () => (await save(closing), close(closing, true))}>Guardar</button>
              <button className="btn" onClick={() => close(closing, true)}>Descartar</button>
              <button className="btn btn-ghost" onClick={() => setClosing(null)}>Cancelar</button>
            </div>
          )}
          {active?.conflict && (
            <div className="ebar warn">
              <span style={{ flex: 1 }}>
                El archivo ha cambiado en disco (probablemente el agente) y aquí tienes cambios sin guardar.
              </span>
              <button className="btn" onClick={takeDisk}>Cargar el del disco</button>
              <button className="btn" onClick={keepMine}>Guardar el mío encima</button>
            </div>
          )}
          {error && <div className="error selectable" style={{ padding: "8px 14px" }}>{error}</div>}
          {/* Always mounted: one CodeMirror view for every file tab. */}
          <div className="ehost" ref={host} style={{ display: active?.saved ? "block" : "none" }} />
          {active && !active.saved && (
            <div className="empty">
              <span>
                {active.size > 1 << 20 ? "Este archivo es demasiado grande para editarlo aquí." : "Este archivo es binario y no se puede editar aquí."}
              </span>
            </div>
          )}
        </div>

        {finding && hasWorkspace && <QuickOpen taskKey={owner} onOpen={openFile} onClose={() => setFinding(false)} />}
        {showTree && hasWorkspace && (
          <aside className="tpanel" aria-label="Archivos del workspace">
            <div className="tpanel-head">
              <span className="sec">Archivos</span>
              <button className="btn btn-ghost icon-btn mono" onClick={() => setFinding(true)} title="Abrir un archivo por su nombre">
                {keyOf("quick-open")}
              </button>
              <button className="btn btn-ghost icon-btn" onClick={() => setTreeRefresh((n) => n + 1)} title="Actualizar" aria-label="Actualizar el árbol">
                <Icon name="refresh" />
              </button>
              <button className="btn btn-ghost icon-btn" onClick={toggleTree} title={`Ocultar (${keyOf("toggle-tree")})`} aria-label="Ocultar el árbol">
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
          aria-label={`Cerrar ${label}`}
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
