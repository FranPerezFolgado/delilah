import { useCallback, useEffect, useMemo, useReducer, useRef, useState } from "react";
import { EditorView } from "@codemirror/view";
import type { EditorState } from "@codemirror/state";
import { api, ConflictError, type ChangeKind, type FileContent, type Task } from "../api";
import { createState, languageFor, languageSlot } from "../editor/codemirror";
import { sessionFor, type Tab } from "../editor/session";
import { FileTree } from "./FileTree";
import { Icon } from "./Icon";

interface Props {
  task: Task;
  /** A file to open, e.g. from the changes view; `n` makes the same file reopenable. */
  request: { path: string; n: number } | null;
  onBack: () => void;
}

const WATCH_MS = 3000;

/** The workspace's files: a tree, tabs and an editor. The agent keeps working in the
 *  same files, so open files follow the disk, and a save never overwrites what it
 *  wrote in the meantime. */
export function EditorScreen({ task, request, onBack }: Props) {
  const session = useMemo(() => sessionFor(task.key), [task.key]);
  const host = useRef<HTMLDivElement>(null);
  const view = useRef<EditorView | null>(null);
  const [, redraw] = useReducer((n: number) => n + 1, 0);
  const [error, setError] = useState<string | null>(null);
  const [closing, setClosing] = useState<string | null>(null);
  const [changed, setChanged] = useState<Map<string, ChangeKind>>(new Map());
  const [treeRefresh, setTreeRefresh] = useState(0);

  const tab = (path: string | null) => session.tabs.find((t) => t.path === path);
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

  const open = useCallback(
    async (path: string) => {
      if (!session.tabs.some((t) => t.path === path)) {
        let f: FileContent;
        try {
          f = await api.file(task.key, path);
        } catch (e) {
          setError((e as Error).message);
          return;
        }
        // Opened meanwhile by another call (a double click, React's dev double effect).
        if (session.tabs.some((t) => t.path === path)) return activate(path);
        const tab: Tab = { path, version: f.version, size: f.size, saved: null, dirty: false, conflict: false };
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
          tab.saved = state.doc;
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
        session.tabs.push(tab);
      }
      // Reveal it in the tree.
      for (let i = path.indexOf("/"); i > 0; i = path.indexOf("/", i + 1)) session.expanded.add(path.slice(0, i));
      activate(path);
    },
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [session, task.key],
  );

  const activate = (path: string | null) => {
    if (session.active && view.current) session.states.set(session.active, view.current.state);
    session.active = path;
    const state = path ? session.states.get(path) : undefined;
    if (state && view.current) {
      view.current.setState(state);
      view.current.focus();
    }
    setError(null);
    redraw();
  };

  const close = (path: string, force = false) => {
    const t = tab(path);
    if (!t) return;
    if (t.dirty && !force) return setClosing(path);
    setClosing(null);
    const at = session.tabs.indexOf(t);
    if (session.active === path && view.current) session.states.set(path, view.current.state);
    session.tabs.splice(at, 1);
    session.states.delete(path);
    if (session.active === path) {
      session.active = null;
      const next = session.tabs[at] ?? session.tabs[at - 1];
      activate(next?.path ?? null);
    } else redraw();
  };

  // One CodeMirror view, its state swapped as tabs change.
  useEffect(() => {
    if (!host.current) return;
    const v = new EditorView({ parent: host.current });
    view.current = v;
    const state = session.active ? session.states.get(session.active) : undefined;
    if (state) v.setState(state);
    return () => {
      if (session.active) session.states.set(session.active, v.state);
      v.destroy();
      view.current = null;
    };
  }, [session]);

  useEffect(() => {
    if (request) open(request.path);
  }, [request, open]);

  // ⌘W closes the active tab here (the app sends it; see App.tsx).
  useEffect(() => {
    const onClose = () => session.active && close(session.active);
    window.addEventListener("delilah:close-tab", onClose);
    return () => window.removeEventListener("delilah:close-tab", onClose);
  });

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
  }, [task.key, treeRefresh]);

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

  return (
    <>
      <div className="view-head" data-tauri-drag-region>
        <button className="btn btn-ghost" onClick={onBack} aria-label="Volver a la tarea" style={{ padding: "0 6px" }}>
          <Icon name="back" />
        </button>
        <span className="mono muted">{task.key}</span>
        <h1 data-tauri-drag-region>Editor</h1>
        <button className="btn" style={{ marginLeft: "auto" }} onClick={() => setTreeRefresh((n) => n + 1)}>
          <Icon name="refresh" /> Actualizar
        </button>
      </div>
      <div className="editor">
        <FileTree
          taskKey={task.key}
          changed={changed}
          active={session.active}
          expanded={session.expanded}
          refresh={treeRefresh}
          onOpen={open}
        />
        <div className="editor-main">
          {session.tabs.length > 0 && (
            <div className="etabs" role="tablist">
              {session.tabs.map((t) => (
                <div
                  key={t.path}
                  role="tab"
                  aria-selected={t.path === session.active}
                  className={`etab ${t.path === session.active ? "on" : ""}`}
                  title={t.path}
                  onClick={() => activate(t.path)}
                  onAuxClick={(e) => e.button === 1 && close(t.path)}
                >
                  <span className="trunc">{name(t.path)}</span>
                  <button
                    className={`etab-x ${t.dirty ? "dirty" : ""}`}
                    aria-label={`Cerrar ${name(t.path)}`}
                    onClick={(e) => {
                      e.stopPropagation();
                      close(t.path);
                    }}
                  >
                    <span className="dot-dirty" />
                    <Icon name="close" />
                  </button>
                </div>
              ))}
            </div>
          )}
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
          <div className="ehost" ref={host} style={{ display: active?.saved ? "block" : "none" }} />
          {active && !active.saved && (
            <div className="empty">
              <span>
                {active.size > 1 << 20 ? "Este archivo es demasiado grande para editarlo aquí." : "Este archivo es binario y no se puede editar aquí."}
              </span>
            </div>
          )}
          {!active && (
            <div className="empty">
              <span>Abre un archivo del árbol.</span>
              <span className="muted" style={{ fontSize: 12 }}>⌘S guarda · ⌘W cierra la pestaña · ⌘F busca</span>
            </div>
          )}
        </div>
      </div>
    </>
  );
}
