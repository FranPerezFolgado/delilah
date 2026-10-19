import { useCallback, useEffect, useMemo, useReducer, useRef, useState } from "react";
import { EditorView } from "@codemirror/view";
import type { EditorState } from "@codemirror/state";
import { api, ConflictError, statusLabel, type ChangeKind, type FileContent, type Session, type Task } from "../api";
import { createState, languageFor, languageSlot } from "../editor/codemirror";
import { CHANGES, OVERVIEW, sessionFor, setTreeOpen, treeOpen, type Tab } from "../editor/session";
import { keyOf } from "../shortcuts";
import { ChangesView } from "./ChangesView";
import { FileTree } from "./FileTree";
import { Icon } from "./Icon";
import { statusIcon } from "./status";
import { TaskView } from "./TaskView";

interface Props {
  task: Task;
  sessions: Session[];
  onOpenConsole: (key: string, sessionId?: string) => void;
  onChanged: () => void;
}

const WATCH_MS = 3000;

/**
 * A task as Orca shows a worktree: tabs for the task itself, its changes and the files
 * opened from the project tree on the right. The agent keeps working in the same
 * files, so open files follow the disk, and a save never overwrites what it wrote in
 * the meantime.
 */
export function TaskScreen({ task, sessions, onOpenConsole, onChanged }: Props) {
  const session = useMemo(() => sessionFor(task.key), [task.key]);
  const host = useRef<HTMLDivElement>(null);
  const view = useRef<EditorView | null>(null);
  const [, redraw] = useReducer((n: number) => n + 1, 0);
  const [error, setError] = useState<string | null>(null);
  const [closing, setClosing] = useState<string | null>(null);
  const [changed, setChanged] = useState<Map<string, ChangeKind>>(new Map());
  const [treeRefresh, setTreeRefresh] = useState(0);
  const [showTree, setShowTree] = useState(treeOpen);
  const hasWorkspace = sessions.length > 0;

  const tab = (path: string) => session.tabs.find((t) => t.path === path);
  const active = tab(session.active);

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
        const f = await api.saveFile(task.key, path, state.doc.toString(), t.version);
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
    [session, task.key],
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
    redraw();
  };

  const openFile = async (path: string) => {
    if (!tab(path)) {
      let f: FileContent;
      try {
        f = await api.file(task.key, path);
      } catch (e) {
        setError((e as Error).message);
        return;
      }
      // Opened meanwhile by another call (a double click, React's dev double effect).
      if (tab(path)) return activate(path);
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
    // Reveal it in the tree.
    for (let i = path.indexOf("/"); i > 0; i = path.indexOf("/", i + 1)) session.expanded.add(path.slice(0, i));
    activate(path);
  };

  const openChanges = () => {
    session.changesOpen = true;
    activate(CHANGES);
  };

  const close = (id: string, force = false) => {
    if (id === OVERVIEW) return;
    // Tabs in strip order, to land on the neighbour.
    const order = [OVERVIEW, ...(session.changesOpen ? [CHANGES] : []), ...session.tabs.map((t) => t.path)];
    const at = order.indexOf(id);
    if (id === CHANGES) session.changesOpen = false;
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
      activate(order[at + 1] ?? order[at - 1] ?? OVERVIEW);
    } else redraw();
  };

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

  // ⌘W closes the active tab and ⌘B shows or hides the tree (the app sends them).
  const closeRef = useRef(close);
  closeRef.current = close;
  useEffect(() => {
    const onClose = () => closeRef.current(session.active);
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
      const f = await api.file(task.key, t.path);
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
      .changes(task.key)
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
  }, [task.key, treeRefresh, hasWorkspace]);

  const keepMine = async () => {
    if (!active) return;
    const f = await api.file(task.key, active.path);
    active.version = f.version;
    active.conflict = false;
    await save(active.path);
    view.current?.focus();
  };
  const takeDisk = async () => {
    if (!active) return;
    setText(active.path, await api.file(task.key, active.path));
    redraw();
  };

  const name = (path: string) => path.slice(path.lastIndexOf("/") + 1);
  const toggleTree = () => window.dispatchEvent(new Event("delilah:toggle-tree"));
  const { icon, cls } = statusIcon(task.status);

  return (
    <>
      <div className="view-head" data-tauri-drag-region>
        <span className="mono muted">{task.key}</span>
        <h1 data-tauri-drag-region className="trunc">{task.title}</h1>
        <span className={`pill ${cls}`} style={{ marginLeft: "auto" }}>
          <Icon name={icon} /> {statusLabel(task.status)}
        </span>
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
            <TabButton id={OVERVIEW} label="Tarea" icon="tasks" session={session} onActivate={activate} />
            {session.changesOpen && (
              <TabButton id={CHANGES} label="Cambios" icon="diff" session={session} onActivate={activate} onClose={close} />
            )}
            {session.tabs.map((t) => (
              <TabButton
                key={t.path}
                id={t.path}
                label={name(t.path)}
                title={t.path}
                dirty={t.dirty}
                session={session}
                onActivate={activate}
                onClose={close}
              />
            ))}
          </div>

          {session.active === OVERVIEW && (
            <TaskView task={task} sessions={sessions} onOpenConsole={onOpenConsole} onChanged={onChanged} onViewChanges={openChanges} />
          )}
          {session.active === CHANGES && <ChangesView task={task} onEdit={openFile} />}

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

        {showTree && hasWorkspace && (
          <aside className="tpanel" aria-label="Archivos del workspace">
            <div className="tpanel-head">
              <span className="sec">Archivos</span>
              <button className="btn btn-ghost icon-btn" onClick={() => setTreeRefresh((n) => n + 1)} title="Actualizar" aria-label="Actualizar el árbol">
                <Icon name="refresh" />
              </button>
              <button className="btn btn-ghost icon-btn" onClick={toggleTree} title={`Ocultar (${keyOf("toggle-tree")})`} aria-label="Ocultar el árbol">
                <Icon name="close" />
              </button>
            </div>
            <FileTree
              taskKey={task.key}
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
  dirty,
  session,
  onActivate,
  onClose,
}: {
  id: string;
  label: string;
  title?: string;
  icon?: "tasks" | "diff";
  dirty?: boolean;
  session: { active: string };
  onActivate: (id: string) => void;
  onClose?: (id: string) => void;
}) {
  const on = session.active === id;
  return (
    <div
      role="tab"
      aria-selected={on}
      className={`etab ${on ? "on" : ""}`}
      title={title ?? label}
      onClick={() => onActivate(id)}
      onAuxClick={(e) => e.button === 1 && onClose?.(id)}
    >
      {icon && <Icon name={icon} />}
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
