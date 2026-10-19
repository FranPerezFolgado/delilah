import { useEffect, useState } from "react";
import { api, hhmm, isActive, statusLabel, type EventRecord, type Task } from "../api";
import { Icon } from "./Icon";
import { statusIcon } from "./status";

const EVENT_LABELS: Record<string, string> = {
  start: "Tarea creada",
  "workspace-ready": "Workspace listo",
  "phase-reached": "Cambio de fase",
  "agent-asked": "El agente te espera",
  "user-answered": "Respondiste",
  "limit-reached": "Límite de uso alcanzado",
  "reset-due": "Reanudada tras el reset",
  "manual-resume": "Reanudada a mano",
  "agent-finished": "El agente terminó",
  "mark-done": "Marcada como hecha",
  fail: "Falló",
};

interface Props {
  task: Task;
  onOpenConsole: (key: string) => void;
  onChanged: () => void;
}

export function TaskView({ task, onOpenConsole, onChanged }: Props) {
  const [events, setEvents] = useState<EventRecord[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    let alive = true;
    const load = () =>
      api.events(task.key).then((e) => alive && setEvents(e)).catch(() => {});
    load();
    const timer = setInterval(load, 2000);
    return () => {
      alive = false;
      clearInterval(timer);
    };
  }, [task.key]);

  const act = async (action: () => Promise<unknown>) => {
    setBusy(true);
    setError(null);
    try {
      await action();
      onChanged();
    } catch (e) {
      setError((e as Error).message);
    } finally {
      setBusy(false);
    }
  };

  const { icon, cls } = statusIcon(task.status);
  const s = task.status;
  return (
    <>
      <div className="view-head">
        <span className="mono muted">{task.key}</span>
        <h1 className="trunc">{task.title}</h1>
        <span className={`pill ${cls}`} style={{ marginLeft: "auto" }}>
          <Icon name={icon} /> {statusLabel(s)}
        </span>
      </div>
      <div className="view-body">
        <div className="muted" style={{ fontSize: 12 }}>
          {task.project ?? "sin proyecto"} · workflow <span className="mono">{task.workflow}</span>
          {task.phase && (
            <>
              {" "}· fase <span className="mono">{task.phase}</span>
            </>
          )}
        </div>

        {s.type === "paused-by-limit" && (
          <div className="notice pause">
            <Icon name="pause" className="c-pause" />
            <span style={{ flex: 1 }}>
              Claude llegó al límite de uso. Se reanuda sola a las <b>{hhmm(s.resume_at)}</b>; el contexto y el workspace se conservan.
            </span>
            <button className="btn" disabled={busy} onClick={() => act(() => api.resume(task.key))}>
              Reanudar ahora
            </button>
          </div>
        )}
        {s.type === "waiting-for-user" && (
          <div className="notice wait">
            <Icon name="wait" className="c-wait" />
            <span style={{ flex: 1 }}>El agente terminó su turno y te espera. Contéstale desde su consola.</span>
            <button className="btn btn-p" onClick={() => onOpenConsole(task.key)}>
              Abrir consola
            </button>
          </div>
        )}
        {s.type === "failed" && (
          <div className="notice">
            <Icon name="failed" className="c-err" />
            <span className="selectable">{s.reason}</span>
          </div>
        )}

        <div style={{ display: "flex", gap: 8 }}>
          {isActive(s) && (
            <button className="btn" onClick={() => onOpenConsole(task.key)}>
              <Icon name="console" /> Consola
            </button>
          )}
          {isActive(s) && (
            <button className="btn" disabled={busy} onClick={() => act(() => api.done(task.key))}>
              <Icon name="done" /> Marcar como hecha
            </button>
          )}
          {(s.type === "done" || s.type === "failed") && (
            <button className="btn" disabled={busy} onClick={() => act(() => api.archive(task.key))}>
              Archivar workspace
            </button>
          )}
        </div>
        {error && <div className="error selectable">{error}</div>}

        <div>
          <div className="sec" style={{ marginBottom: 8 }}>Historial</div>
          <div className="card history">
            {events.length === 0 && <div className="row muted">Sin eventos todavía.</div>}
            {[...events].reverse().map((e, i) => (
              <div className="row" key={i}>
                <span className="mono muted">{new Date(e.at).toLocaleTimeString("es-ES")}</span>
                <span>
                  {EVENT_LABELS[e.event.type] ?? e.event.type}
                  {e.event.type === "limit-reached" && typeof e.event.resume_at === "string" && (
                    <span className="muted"> · hasta las {hhmm(e.event.resume_at)}</span>
                  )}
                  {e.event.type === "phase-reached" && <span className="muted mono"> · {String(e.event.phase)}</span>}
                </span>
              </div>
            ))}
          </div>
        </div>
      </div>
    </>
  );
}
