import { useEffect, useState } from "react";
import { api, hhmm, isActive, REVIEW_PROMPT, type EventRecord, type Session, type SessionKind, type Task } from "../api";
import { sessionStatus } from "./sessionLabels";
import { Icon } from "./Icon";
import { SpecPanel } from "./SpecPanel";
import { phaseLabel, type Column } from "../phases";

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
  sessions: Session[];
  /** Opens the console on a session of this task (its primary one if none given). */
  onOpenConsole: (key: string, sessionId?: string) => void;
  onChanged: () => void;
  /** Opens the changes tab. */
  onViewChanges: () => void;
}

/** The task tab of a task's screen: status, actions, spec, sessions and history. */
export function TaskView({ task, sessions, onOpenConsole, onChanged, onViewChanges }: Props) {
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

  const openSession = (kind: SessionKind, prompt?: string, label?: string) =>
    act(async () => {
      const s = await api.openSession(task.key, { kind, prompt, label });
      onOpenConsole(task.key, s.id);
    });
  const hasWorkspace = sessions.length > 0;

  const s = task.status;
  return (
    <>
      <div className="view-body">
        <div className="muted" style={{ fontSize: 12 }}>
          {task.project ?? "sin proyecto"} · workflow <span className="mono">{task.workflow}</span>
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
        {task.phase === "review" && isActive(s) && hasWorkspace && (
          <div className="notice">
            <Icon name="review" className="c-done" />
            <span style={{ flex: 1 }}>Todas las subtareas están hechas: toca revisar los cambios de la rama.</span>
            <button className="btn" onClick={onViewChanges}>
              <Icon name="diff" /> Ver cambios
            </button>
            <button className="btn btn-p" disabled={busy} onClick={() => openSession("claude", REVIEW_PROMPT, "Revisión")}>
              Revisar con Claude
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
          {hasWorkspace && (
            <button className="btn" onClick={onViewChanges}>
              <Icon name="diff" /> Ver cambios
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

        <SpecPanel task={task} />

        {hasWorkspace && (
          <div>
            <div style={{ display: "flex", alignItems: "center", gap: 8, marginBottom: 8 }}>
              <span className="sec">Sesiones en el workspace</span>
              <span style={{ marginLeft: "auto", display: "flex", gap: 6 }}>
                <button className="btn" disabled={busy} onClick={() => openSession("claude", REVIEW_PROMPT, "Revisión")}>
                  <Icon name="review" /> Revisar cambios
                </button>
                <button className="btn" disabled={busy} onClick={() => openSession("claude")}>
                  <Icon name="plus" /> Claude
                </button>
                <button className="btn" disabled={busy} onClick={() => openSession("shell")}>
                  <Icon name="plus" /> Terminal
                </button>
              </span>
            </div>
            <div className="card history">
              {sessions.map((s) => {
                const st = sessionStatus(s, task);
                return (
                  <div className="row" key={s.id} style={{ gridTemplateColumns: "1fr auto auto", alignItems: "center" }}>
                    <span style={{ display: "flex", alignItems: "center", gap: 8 }}>
                      <Icon name={s.kind === "shell" ? "console" : st.icon} className={st.cls} />
                      <b style={{ fontWeight: 600 }}>{s.label}</b>
                      <span className="muted">{s.kind === "shell" ? "terminal" : s.primary ? "Claude · principal" : "Claude"}</span>
                    </span>
                    <span className={`pill ${st.cls}`} style={{ height: 18, fontSize: 11 }}>{st.label}</span>
                    {s.live ? (
                      <button className="btn btn-ghost" onClick={() => onOpenConsole(task.key, s.id)}>Abrir</button>
                    ) : (
                      <button className="btn btn-ghost" disabled={busy} onClick={() => act(() => api.reopenSession(s.id))}>Reabrir</button>
                    )}
                  </div>
                );
              })}
            </div>
          </div>
        )}

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
                  {e.event.type === "phase-reached" && <span className="muted"> · {phaseLabel(String(e.event.phase) as Column)}</span>}
                </span>
              </div>
            ))}
          </div>
        </div>
      </div>
    </>
  );
}
