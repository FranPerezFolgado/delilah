import { useState } from "react";
import { api, isActive, type Session, type Task } from "../api";
import { Icon } from "./Icon";
import { sessionStatus } from "./sessionLabels";
import { keyOf } from "../shortcuts";
import { TerminalPane } from "./TerminalPane";

export type Layout = 1 | 2 | 4;
const GRIDS: Record<Layout, React.CSSProperties> = {
  1: { gridTemplateColumns: "minmax(0,1fr)", gridTemplateRows: "minmax(0,1fr)" },
  2: { gridTemplateColumns: "repeat(2,minmax(0,1fr))", gridTemplateRows: "minmax(0,1fr)" },
  4: { gridTemplateColumns: "repeat(2,minmax(0,1fr))", gridTemplateRows: "repeat(2,minmax(0,1fr))" },
};

/** Sessions the console shows, in pane order: the selected task's first, its primary
 *  one first, and the focused one on top. */
export function consoleOrder(tasks: Task[], sessions: Session[], task: string | null, focus: string | null): Session[] {
  const taskOf = (s: Session) => tasks.find((t) => t.key === s.task);
  const rank = (s: Session) => (s.task === task ? 0 : 10) + (s.primary ? 0 : 1) + (s.id === focus ? -5 : 0);
  return sessions.filter((s) => s.live || (taskOf(s) && isActive(taskOf(s)!.status))).sort((a, b) => rank(a) - rank(b));
}

interface Props {
  layout: Layout;
  onLayout: (l: Layout) => void;
  tasks: Task[];
  sessions: Session[];
  /** Selected task: its sessions come first. */
  task: string | null;
  focus: string | null;
  onFocus: (sessionId: string, task: string) => void;
  onChanged: () => void;
  onNewTask: () => void;
  onOpenTask: (key: string) => void;
}

/** Live terminals of every session, side by side; a task's sessions sit together. */
export function ConsoleView({ layout, onLayout: setLayout, tasks, sessions, task, focus, onFocus, onChanged, onNewTask, onOpenTask }: Props) {
  const [menu, setMenu] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const taskOf = (s: Session) => tasks.find((t) => t.key === s.task);
  const shown = consoleOrder(tasks, sessions, task, focus);
  const panes = shown.slice(0, layout);
  const focused = panes.find((s) => s.id === focus) ?? panes[0];
  const grid: Layout = panes.length <= 1 ? 1 : panes.length === 2 ? 2 : 4;

  const act = async (action: () => Promise<unknown>) => {
    setError(null);
    setMenu(null);
    try {
      await action();
      onChanged();
    } catch (e) {
      setError((e as Error).message);
    }
  };
  const open = (s: Session, kind: "claude" | "shell") =>
    act(async () => {
      const created = await api.openSession(s.task, { kind });
      onFocus(created.id, s.task);
    });

  return (
    <>
      <div className="view-head" data-tauri-drag-region>
        <h1 data-tauri-drag-region>Consola</h1>
        <span className="muted" style={{ fontSize: 12 }}>
          {shown.length} {shown.length === 1 ? "sesión" : "sesiones"}
        </span>
        {task && (
          <button className="btn" style={{ marginLeft: 8 }} onClick={() => onOpenTask(task)}>
            ← Ver tarea <span className="mono">{task}</span>
          </button>
        )}
        <div className="segs" style={{ marginLeft: "auto" }} role="group" aria-label="Disposición">
          {([1, 2, 4] as Layout[]).map((n) => (
            <button key={n} className={`seg ${layout === n ? "on" : ""}`} aria-pressed={layout === n} onClick={() => setLayout(n)}>
              {n}
            </button>
          ))}
        </div>
        <button className="btn btn-p" onClick={onNewTask}>
          <Icon name="plus" /> Nueva tarea
        </button>
      </div>
      {error && <div className="error selectable" style={{ padding: "6px 14px" }}>{error}</div>}
      {shown.length === 0 ? (
        <div className="empty">
          <Icon name="console" />
          <span>No hay sesiones abiertas ahora mismo.</span>
          <button className="btn btn-p" onClick={onNewTask}>Lanzar una tarea</button>
        </div>
      ) : (
        <div className="grid" style={GRIDS[grid]}>
          {panes.map((s) => {
            const t = taskOf(s);
            const status = sessionStatus(s, t);
            const on = s.id === focused?.id;
            return (
              <section key={s.id} className={`pane ${on ? "on" : ""}`} aria-label={`${s.task} · ${s.label}`}>
                <div className="pane-head" onClick={() => onFocus(s.id, s.task)}>
                  <Icon name={s.kind === "shell" ? "console" : status.icon} className={status.cls} />
                  <span style={{ fontWeight: 600, whiteSpace: "nowrap" }}>{s.label}</span>
                  <span className="mono" style={{ color: "var(--tx2)" }}>{s.task}</span>
                  <span className="trunc muted" style={{ flex: 1 }}>{t?.title}</span>
                  <span className={`pill ${status.cls}`} style={{ height: 18, fontSize: 11 }}>{status.label}</span>
                  <button className="btn btn-ghost" style={{ height: 22, padding: "0 4px" }} aria-label="Ver la tarea" title="Ver la tarea"
                    onClick={(e) => { e.stopPropagation(); onOpenTask(s.task); }}>
                    <Icon name="open" />
                  </button>
                  <div style={{ position: "relative" }}>
                    <button className="btn btn-ghost" style={{ height: 22, padding: "0 4px" }} aria-label="Nueva sesión en este workspace"
                      onClick={(e) => { e.stopPropagation(); setMenu(menu === s.id ? null : s.id); }}>
                      <Icon name="plus" />
                    </button>
                    {menu === s.id && (
                      <div className="menu" onClick={(e) => e.stopPropagation()}>
                        <button onClick={() => open(s, "claude")}>Nueva sesión de Claude <span className="kbd">{keyOf("new-claude")}</span></button>
                        <button onClick={() => open(s, "shell")}>Nueva terminal <span className="kbd">{keyOf("new-terminal")}</span></button>
                      </div>
                    )}
                  </div>
                  {layout !== 1 && (
                    <button className="btn btn-ghost" style={{ height: 22, padding: "0 4px" }} aria-label="Maximizar"
                      onClick={(e) => { e.stopPropagation(); onFocus(s.id, s.task); setLayout(1); }}>
                      <Icon name="maximize" />
                    </button>
                  )}
                  {!s.primary && (
                    <button className="btn btn-ghost" style={{ height: 22, padding: "0 4px" }} aria-label="Cerrar sesión"
                      onClick={(e) => { e.stopPropagation(); act(() => api.closeSession(s.id)); }}>
                      <Icon name="close" />
                    </button>
                  )}
                </div>
                {s.live ? (
                  <TerminalPane sessionId={s.id} focused={on} />
                ) : (
                  <div className="empty" style={{ fontSize: 12.5 }}>
                    <span>La sesión está parada (el daemon se reinició o el proceso terminó).</span>
                    <button className="btn btn-p" onClick={() => act(() => api.reopenSession(s.id))}>
                      Reabrir{s.kind === "claude" ? " la conversación" : ""}
                    </button>
                  </div>
                )}
              </section>
            );
          })}
        </div>
      )}
    </>
  );
}
