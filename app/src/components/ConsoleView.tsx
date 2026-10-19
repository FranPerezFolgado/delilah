import { useState } from "react";
import { statusLabel, type Task } from "../api";
import { Icon } from "./Icon";
import { statusIcon } from "./status";
import { TerminalPane } from "./TerminalPane";

type Layout = 1 | 2 | 4;
const GRIDS: Record<Layout, React.CSSProperties> = {
  1: { gridTemplateColumns: "minmax(0,1fr)", gridTemplateRows: "minmax(0,1fr)" },
  2: { gridTemplateColumns: "repeat(2,minmax(0,1fr))", gridTemplateRows: "minmax(0,1fr)" },
  4: { gridTemplateColumns: "repeat(2,minmax(0,1fr))", gridTemplateRows: "repeat(2,minmax(0,1fr))" },
};

interface Props {
  tasks: Task[];
  focus: string | null;
  onFocus: (key: string) => void;
  onNewTask: () => void;
}

/** Live terminals of the agents, side by side. */
export function ConsoleView({ tasks, focus, onFocus, onNewTask }: Props) {
  const [layout, setLayout] = useState<Layout>(4);
  const live = tasks.filter((t) => ["running", "waiting-for-user", "paused-by-limit"].includes(t.status.type));
  const focused = live.find((t) => t.key === focus) ?? live[0];
  const ordered = focused ? [focused, ...live.filter((t) => t !== focused)] : live;
  const shown = ordered.slice(0, layout);
  // Fewer sessions than slots: let the ones there are use the space.
  const grid: Layout = shown.length <= 1 ? 1 : shown.length === 2 ? 2 : 4;

  return (
    <>
      <div className="view-head">
        <h1>Consola</h1>
        <span className="muted" style={{ fontSize: 12 }}>
          {live.length} {live.length === 1 ? "sesión" : "sesiones"}
        </span>
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
      {live.length === 0 ? (
        <div className="empty">
          <Icon name="console" />
          <span>No hay agentes trabajando ahora mismo.</span>
          <button className="btn btn-p" onClick={onNewTask}>
            Lanzar una tarea
          </button>
        </div>
      ) : (
        <div className="grid" style={GRIDS[grid]}>
          {shown.map((t) => {
            const { icon, cls } = statusIcon(t.status);
            const on = t.key === focused?.key;
            return (
              <section key={t.key} className={`pane ${on ? "on" : ""}`} aria-label={`${t.key} · ${statusLabel(t.status)}`}>
                <div className="pane-head" onClick={() => onFocus(t.key)}>
                  <Icon name={icon} className={cls} />
                  <span style={{ fontWeight: 600 }}>Claude Code</span>
                  <span className="mono" style={{ color: "var(--tx2)" }}>{t.key}</span>
                  <span className="trunc muted" style={{ flex: 1 }}>{t.title}</span>
                  <span className={`pill ${cls}`} style={{ height: 18, fontSize: 11 }}>{statusLabel(t.status)}</span>
                  {layout !== 1 && (
                    <button className="btn btn-ghost" style={{ height: 22, padding: "0 4px" }} aria-label="Maximizar"
                      onClick={(e) => { e.stopPropagation(); onFocus(t.key); setLayout(1); }}>
                      <Icon name="maximize" />
                    </button>
                  )}
                </div>
                <TerminalPane taskKey={t.key} focused={on} />
              </section>
            );
          })}
        </div>
      )}
    </>
  );
}
