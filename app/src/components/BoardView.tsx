import { statusLabel, type SpecSummary, type Task } from "../api";
import { COLUMNS, columnOf } from "../phases";
import { Icon } from "./Icon";
import { statusIcon } from "./status";

interface Props {
  tasks: Task[];
  specs: Record<string, SpecSummary>;
  onOpen: (key: string) => void;
  onNewTask: () => void;
}

/** Every task on one board, in the column of its SpecKit phase. */
export function BoardView({ tasks, specs, onOpen, onNewTask }: Props) {
  return (
    <>
      <div className="view-head" data-tauri-drag-region>
        <h1 data-tauri-drag-region>Tareas</h1>
        <span className="muted" style={{ fontSize: 12 }}>
          por fase de la spec
        </span>
        <button className="btn" style={{ marginLeft: "auto" }} onClick={onNewTask}>
          <Icon name="plus" /> Nueva tarea
        </button>
      </div>
      {tasks.length === 0 ? (
        <div className="empty">
          <span>Todavía no hay tareas.</span>
          <button className="btn btn-p" onClick={onNewTask}>
            Nueva tarea
          </button>
        </div>
      ) : (
        <div className="board">
          {COLUMNS.map((col) => {
            const cards = tasks.filter((t) => columnOf(t, specs[t.key]) === col.id);
            return (
              <section className="col" key={col.id} aria-label={col.label}>
                <div className="col-head">
                  <b>{col.label}</b>
                  <span className="mono muted">{cards.length}</span>
                </div>
                <div className="col-body">
                  {cards.map((t) => (
                    <Card key={t.key} task={t} spec={specs[t.key]} onOpen={onOpen} />
                  ))}
                </div>
              </section>
            );
          })}
        </div>
      )}
    </>
  );
}

function Card({ task, spec, onOpen }: { task: Task; spec?: SpecSummary; onOpen: (key: string) => void }) {
  const { icon, cls } = statusIcon(task.status);
  const showProgress = spec && spec.total > 0;
  const showCurrent = spec?.current && task.status.type !== "done" && spec.phase === "implement";
  return (
    <button className="kcard" onClick={() => onOpen(task.key)}>
      <span className="kcard-top">
        <span className="mono muted">{task.key}</span>
        <span className={`kcard-status ${cls}`} title={statusLabel(task.status)}>
          <Icon name={icon} />
          {task.status.type !== "done" && task.status.type !== "running" && <span>{statusLabel(task.status)}</span>}
        </span>
      </span>
      <span className="kcard-title">{task.title}</span>
      {task.project && <span className="muted kcard-meta">{task.project}</span>}
      {showProgress && (
        <span className="kcard-progress">
          <span className="bar" style={{ flex: 1 }}>
            <span style={{ width: `${(spec.done / spec.total) * 100}%`, background: spec.done === spec.total ? "var(--done)" : "var(--run)" }} />
          </span>
          <span className="mono muted">
            {spec.done}/{spec.total}
          </span>
        </span>
      )}
      {showCurrent && <span className="muted kcard-meta trunc">→ {spec.current}</span>}
    </button>
  );
}
