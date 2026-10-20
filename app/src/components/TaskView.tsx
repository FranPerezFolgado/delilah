import { useEffect, useState } from "react";
import { api, ApiError, hhmm, isActive, reviewPrompt, type EventRecord, type Session, type SessionKind, type Task } from "../api";
import { sessionStatus } from "./sessionLabels";
import { on } from "../live";
import { Icon } from "./Icon";
import { SpecPanel } from "./SpecPanel";
import { phaseLabel, type Column } from "../phases";
import { locale, t } from "../i18n";

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
  /** Shows a session's tab (the primary one if none given). */
  onOpenSession: (sessionId?: string) => void;
  /** Starts a session in the workspace and shows its tab. */
  onNewSession: (kind: SessionKind, prompt?: string, label?: string) => Promise<void>;
  onChanged: () => void;
  /** Opens the changes tab. */
  onViewChanges: () => void;
}

/** The task tab of a task's screen: status, actions, spec, sessions and history. */
export function TaskView({ task, sessions, onOpenSession, onNewSession, onChanged, onViewChanges }: Props) {
  const [events, setEvents] = useState<EventRecord[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  /** Why archiving the worktree was refused (work that would be lost). */
  const [keeps, setKeeps] = useState<string | null>(null);

  // The history changes with the task: fetched again whenever tasks change.
  useEffect(() => {
    let alive = true;
    const load = () =>
      api.events(task.key).then((e) => alive && setEvents(e)).catch(() => {});
    load();
    const off = on("tasks", load);
    return () => {
      alive = false;
      off();
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

  const openSession = (kind: SessionKind, prompt?: string, label?: string) => act(() => onNewSession(kind, prompt, label));
  const archive = (force: boolean) =>
    act(async () => {
      try {
        await api.archive(task.key, force);
        setKeeps(null);
      } catch (e) {
        if (e instanceof ApiError && e.status === 409 && !force) setKeeps(e.message);
        else throw e;
      }
    });
  // Asking for a review puts the task in review.
  const review = () =>
    act(async () => {
      if (task.phase !== "review" && task.status.type !== "done") await api.setPhase(task.key, "review");
      await onNewSession("claude", reviewPrompt(), t("Revisión"));
    });
  const hasWorkspace = sessions.length > 0;

  const s = task.status;
  return (
    <>
      <div className="view-body">
        <div className="muted" style={{ fontSize: 12 }}>
          {task.project ?? t("sin proyecto")} · workflow <span className="mono">{task.workflow}</span>
        </div>

        {s.type === "paused-by-limit" && (
          <div className="notice pause">
            <Icon name="pause" className="c-pause" />
            <span style={{ flex: 1 }}>
              {t("Claude llegó al límite de uso. Se reanuda sola a las {time}; el contexto y el workspace se conservan.", { time: hhmm(s.resume_at) })}
            </span>
            <button className="btn" disabled={busy} onClick={() => act(() => api.resume(task.key))}>
              {t("Reanudar ahora")}
            </button>
          </div>
        )}
        {s.type === "waiting-for-user" && (
          <div className="notice wait">
            <Icon name="wait" className="c-wait" />
            <span style={{ flex: 1 }}>{t("El agente terminó su turno y te espera. Contéstale en su pestaña.")}</span>
            <button className="btn btn-p" onClick={() => onOpenSession()}>
              {t("Contestar")}
            </button>
          </div>
        )}
        {task.phase === "review" && isActive(s) && hasWorkspace && (
          <div className="notice">
            <Icon name="review" className="c-done" />
            <span style={{ flex: 1 }}>{t("La tarea está en revisión: toca revisar los cambios de la rama.")}</span>
            <button className="btn" onClick={onViewChanges}>
              <Icon name="diff" /> {t("Ver cambios")}
            </button>
            <button className="btn btn-p" disabled={busy} onClick={review}>
              {t("Revisar con Claude")}
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
            <button className="btn" onClick={() => onOpenSession()}>
              <Icon name="console" /> {t("Claude principal")}
            </button>
          )}
          {hasWorkspace && (
            <button className="btn" onClick={onViewChanges}>
              <Icon name="diff" /> {t("Ver cambios")}
            </button>
          )}
          {isActive(s) && (
            <button className="btn" disabled={busy} onClick={() => act(() => api.done(task.key))}>
              <Icon name="done" /> {t("Marcar como hecha")}
            </button>
          )}
          {(s.type === "done" || s.type === "failed") && (
            <button className="btn" disabled={busy} onClick={() => archive(false)}>
              {t("Archivar workspace")}
            </button>
          )}
        </div>
        {error && <div className="error selectable">{error}</div>}
        {keeps && (
          <div className="notice">
            <Icon name="failed" className="c-err" />
            <span style={{ flex: 1 }} className="selectable">
              {t("No se ha archivado: {why}. Commitea y sube lo que quieras conservar, o archívalo igualmente.", { why: keeps })}
            </span>
            <button className="btn" onClick={onViewChanges}>
              <Icon name="diff" /> {t("Ver cambios")}
            </button>
            <button className="btn btn-danger" disabled={busy} onClick={() => archive(true)}>
              {t("Archivar igualmente (se pierde lo no subido)")}
            </button>
            <button className="btn btn-ghost" onClick={() => setKeeps(null)} aria-label={t("Cerrar")}>
              <Icon name="close" />
            </button>
          </div>
        )}

        <SpecPanel task={task} />

        {hasWorkspace && (
          <div>
            <div style={{ display: "flex", alignItems: "center", gap: 8, marginBottom: 8 }}>
              <span className="sec">{t("Sesiones en el workspace")}</span>
              <span style={{ marginLeft: "auto", display: "flex", gap: 6 }}>
                <button className="btn" disabled={busy} onClick={review}>
                  <Icon name="review" /> {t("Revisar cambios")}
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
                      <span className="muted">{s.kind === "shell" ? t("terminal") : s.primary ? t("Claude · principal") : "Claude"}</span>
                    </span>
                    <span className={`pill ${st.cls}`} style={{ height: 18, fontSize: 11 }}>{st.label}</span>
                    {s.live ? (
                      <button className="btn btn-ghost" onClick={() => onOpenSession(s.id)}>{t("Abrir")}</button>
                    ) : (
                      <button
                        className="btn btn-ghost"
                        disabled={busy}
                        onClick={() => act(async () => (await api.reopenSession(s.id), onOpenSession(s.id)))}
                      >
                        {t("Reabrir")}
                      </button>
                    )}
                  </div>
                );
              })}
            </div>
          </div>
        )}

        <div>
          <div className="sec" style={{ marginBottom: 8 }}>{t("Historial")}</div>
          <div className="card history">
            {events.length === 0 && <div className="row muted">{t("Sin eventos todavía.")}</div>}
            {[...events].reverse().map((e, i) => (
              <div className="row" key={i}>
                <span className="mono muted">{new Date(e.at).toLocaleTimeString(locale())}</span>
                <span>
                  {EVENT_LABELS[e.event.type] ? t(EVENT_LABELS[e.event.type]) : e.event.type}
                  {e.event.type === "limit-reached" && typeof e.event.resume_at === "string" && (
                    <span className="muted"> · {t("hasta las {time}", { time: hhmm(e.event.resume_at) })}</span>
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
