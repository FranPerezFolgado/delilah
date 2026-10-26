import { useEffect, useRef, useState } from "react";
import { api, ApiError, hhmm, isActive, reviewPrompt, type EventRecord, type InboxItem, type Session, type SessionKind, type Task } from "../api";
import { SetupBanner } from "./SetupBanner";
import { isSetupRunning, kindLabel } from "./sessionLabels";
import { on } from "../live";
import { Icon } from "./Icon";
import { SpecPanel } from "./SpecPanel";
import { AskCard } from "./AskCard";
import { Button, ConfirmDialog, Menu, Skeleton, StatusBadge, dismissToast, pushToast } from "./ui";
import { historyEntry } from "../history";
import { relativeTime } from "../time";
import { sessionDot } from "../vocab";
import { t } from "../i18n";
import "./TaskView.css";

interface Props {
  task: Task;
  sessions: Session[];
  /** What the task's sessions wait on (questions with options, permissions): answered here, inline. */
  asks?: InboxItem[];
  /** Shows a session's tab (the primary one if none given). */
  onOpenSession: (sessionId?: string) => void;
  /** Starts a session in the workspace and shows its tab. */
  onNewSession: (kind: SessionKind, prompt?: string, label?: string) => Promise<void>;
  onChanged: () => void;
  /** Opens the changes tab. */
  onViewChanges: () => void;
  /** Goes to a task a proposal created. */
  onOpenTask?: (key: string) => void;
}

/** Asks the app to confirm deleting a task (see DeleteTaskDialog in App). */
/** The workspace of these tasks can be archived (a setup-failed one keeps its worktree). */
export const canArchive = (s: Task["status"]) => s.type === "done" || s.type === "failed" || s.type === "setup-failed";

export const askToDelete = (task: Task) =>
  window.dispatchEvent(new CustomEvent<Task>("delilah:delete-task", { detail: task }));

/** The task tab of a task's screen: meta line, actions, pending asks, sessions, spec and history. */
export function TaskView({ task, sessions, asks = [], onOpenSession, onNewSession, onChanged, onViewChanges, onOpenTask }: Props) {
  const [events, setEvents] = useState<EventRecord[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  /** Why archiving the worktree was refused (work that would be lost). */
  const [keeps, setKeeps] = useState<string | null>(null);
  const [deleting, setDeleting] = useState(false);
  const [askAt, setAskAt] = useState(0);
  /** The «Deshacer» toast of the last «Marcar como hecha», while undoing still works. */
  const undoToast = useRef<number | null>(null);

  // The history changes with the task: fetched again whenever tasks change.
  useEffect(() => {
    let alive = true;
    const load = () =>
      api.events(task.key).then((e) => alive && setEvents(e)).catch(() => alive && setEvents((cur) => cur ?? []));
    load();
    const off = on("tasks", load);
    return () => {
      alive = false;
      off();
    };
  }, [task.key]);

  const hasWorkspace = sessions.length > 0;
  const s = task.status;

  // Once the workspace is gone a reopen cannot work: «Deshacer» goes away.
  useEffect(() => {
    if (undoToast.current != null && s.type === "done" && !hasWorkspace) {
      dismissToast(undoToast.current);
      undoToast.current = null;
    }
  }, [s.type, hasWorkspace]);

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

  const archive = (force: boolean) =>
    act(async () => {
      try {
        await api.archive(task.key, force);
        if (undoToast.current != null) dismissToast(undoToast.current);
        undoToast.current = null;
        setKeeps(null);
      } catch (e) {
        if (e instanceof ApiError && e.status === 409 && !force) setKeeps(e.message);
        else throw e;
      }
    });
  const markDone = () =>
    act(async () => {
      await api.done(task.key);
      undoToast.current = pushToast({
        text: t("{key} marcada como hecha", { key: task.key }),
        action: {
          label: t("Deshacer"),
          run: () => {
            undoToast.current = null;
            api
              .reopen(task.key)
              .then(onChanged)
              .catch(() => pushToast({ text: t("No se pudo deshacer: el workspace ya no está."), kind: "error" }));
          },
        },
      });
    });
  const remove = async () => {
    setDeleting(false);
    try {
      await api.deleteTask(task.key);
      onChanged();
    } catch (e) {
      // Work that would be lost: the app's own dialog asks about forcing it.
      if (e instanceof ApiError && e.unpushed) askToDelete(task);
      else setError((e as Error).message);
    }
  };
  // Asking for a review puts the task in review.
  const review = () =>
    act(async () => {
      if (task.phase !== "review" && task.status.type !== "done") await api.setPhase(task.key, "review");
      await onNewSession("claude", reviewPrompt(), t("Revisión"));
    });

  const doneAt = s.type === "done" ? [...(events ?? [])].reverse().find((e) => e.event.type === "mark-done")?.at : undefined;
  const pending = asks.filter((a) => a.ask || a.permission);
  const at = Math.min(askAt, Math.max(0, pending.length - 1));
  const current = pending[at];
  const menu = [
    { label: t("Abrir cambios"), onSelect: onViewChanges, disabled: !hasWorkspace },
    { label: t("Archivar workspace"), onSelect: () => archive(false), disabled: !canArchive(s) || busy },
    ...(s.type !== "preparing-workspace" ? [{ label: t("Eliminar tarea…"), onSelect: () => setDeleting(true), danger: true }] : []),
  ];

  return (
    <div className="view-body tv">
      <div className="tv-head">
        <div className="tv-meta">
          <span>{task.project ?? t("sin proyecto")}</span>
          <span aria-hidden="true">·</span>
          <span>{t("flujo")} <span className="mono">{task.workflow}</span></span>
          {task.branch && (
            <>
              <span aria-hidden="true">·</span>
              <span className="mono selectable tv-branch" data-testid="task-branch">{task.branch}</span>
            </>
          )}
          {s.type === "done" && (
            <>
              <StatusBadge status={s} />
              {doneAt && <span>{relativeTime(doneAt)}</span>}
            </>
          )}
        </div>
        <div className="tv-actions">
          {isActive(s) && (
            <Button variant="secondary" icon="done" disabled={busy} onClick={markDone}>
              {t("Marcar como hecha")}
            </Button>
          )}
          <Menu
            items={menu}
            trigger={(p) => (
              <Button variant="secondary" {...p}>
                {t("Más")}
              </Button>
            )}
          />
        </div>
      </div>

      {error && <div className="tv-error selectable">{error}</div>}

      {s.type === "paused-by-limit" && (
        <div className="tv-notice tv-pause">
          <Icon name="pause" className="c-pause" />
          <span className="tv-grow">
            {t("Claude llegó al límite de uso. Se reanuda sola a las {time}; el contexto y el workspace se conservan.", { time: hhmm(s.resume_at) })}
          </span>
          <Button variant="secondary" size={28} disabled={busy} onClick={() => act(() => api.resume(task.key))}>
            {t("Reanudar ahora")}
          </Button>
        </div>
      )}
      {(s.type === "setup-failed" || isSetupRunning(sessions)) && (
        <SetupBanner task={task} running={isSetupRunning(sessions)} onChanged={onChanged} />
      )}
      {s.type === "failed" && (
        <div className="tv-notice tv-err">
          <Icon name="failed" className="c-err" />
          <span className="tv-grow selectable">{s.reason}</span>
          <Button variant="primary" size={28} disabled={busy} onClick={() => act(() => api.resume(task.key))}>
            {t("Reanudar")}
          </Button>
        </div>
      )}
      {current && (
        <AskCard
          key={current.ask?.id ?? current.id}
          variant="inline"
          ask={current.ask}
          permission={!current.ask && current.permission && current.session ? { sessionId: current.session.id, ask: current.permission } : undefined}
          nav={{ index: at, total: pending.length, onChange: setAskAt }}
          onOpenTask={onOpenTask}
          onDone={onChanged}
        />
      )}
      {s.type === "waiting-for-user" && !current && (
        <div className="tv-notice tv-wait">
          <Icon name="wait" className="c-wait" />
          <span className="tv-grow">{t("El agente terminó su turno y te espera. Contéstale en su pestaña.")}</span>
          <Button variant="primary" size={28} onClick={() => onOpenSession()}>
            {t("Contestar")}
          </Button>
        </div>
      )}
      {task.phase === "review" && isActive(s) && hasWorkspace && (
        <div className="tv-notice">
          <Icon name="review" className="c-done" />
          <span className="tv-grow">{t("La tarea está en revisión: toca revisar los cambios de la rama.")}</span>
          <Button variant="secondary" size={28} icon="diff" onClick={onViewChanges}>
            {t("Ver cambios")}
          </Button>
          <Button variant="primary" size={28} disabled={busy} onClick={review}>
            {t("Revisar con Claude")}
          </Button>
        </div>
      )}
      {keeps && (
        <div className="tv-notice">
          <Icon name="failed" className="c-err" />
          <span className="tv-grow selectable">
            {t("No se ha archivado: {why}. Commitea y sube lo que quieras conservar, o archívalo igualmente.", { why: keeps })}
          </span>
          <Button variant="secondary" size={28} icon="diff" onClick={onViewChanges}>
            {t("Ver cambios")}
          </Button>
          <Button variant="danger" size={28} disabled={busy} onClick={() => archive(true)}>
            {t("Archivar igualmente (se pierde lo no subido)")}
          </Button>
          <button type="button" className="ib" onClick={() => setKeeps(null)} aria-label={t("Cerrar")}>
            <Icon name="close" />
          </button>
        </div>
      )}

      {hasWorkspace && (
        <section className="tv-section">
          <div className="tv-title">
            <h3>{t("Sesiones")}</h3>
            <Button variant="ghost" size={28} icon="review" disabled={busy} onClick={review}>
              {t("Revisar cambios")}
            </Button>
          </div>
          <div className="tv-card tv-sessions">
            {sessions.map((x) => (
              <div className="tv-srow" key={x.id}>
                <span className={`tv-dot tv-dot-${sessionDot(x)}`} aria-hidden="true" />
                <span className="tv-sname">{x.primary && x.kind === "claude" ? t("Claude principal") : x.label}</span>
                <span className="tv-skind">{kindLabel(x)}</span>
                {x.live ? (
                  <Button variant="ghost" size={28} onClick={() => onOpenSession(x.id)}>{t("Abrir")}</Button>
                ) : (
                  <Button
                    variant="ghost"
                    size={28}
                    disabled={busy}
                    onClick={() => act(async () => (await api.reopenSession(x.id), onOpenSession(x.id)))}
                  >
                    {t("Reabrir")}
                  </Button>
                )}
              </div>
            ))}
          </div>
        </section>
      )}

      <SpecPanel task={task} />

      <section className="tv-section">
        <div className="tv-title"><h3>{t("Historial")}</h3></div>
        <div className="tv-card tv-history">
          {events === null && <Skeleton lines={3} />}
          {events?.length === 0 && <div className="tv-empty">{t("Sin eventos todavía.")}</div>}
          {[...(events ?? [])].reverse().map((e, i) => {
            const h = historyEntry(e);
            return (
              <div className="tv-ev" key={i}>
                <span className="tv-when">{h.when}</span>
                <span className="selectable">{h.text}</span>
              </div>
            );
          })}
        </div>
      </section>

      {deleting && (
        <ConfirmDialog
          title={t("Eliminar tarea")}
          body={t("«{title}» se elimina para siempre: se detiene su agente y se libera su workspace (la rama se queda).", { title: task.title })}
          action={t("Eliminar tarea")}
          danger
          onConfirm={remove}
          onClose={() => setDeleting(false)}
        />
      )}
    </div>
  );
}
