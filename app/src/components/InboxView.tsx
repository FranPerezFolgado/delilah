import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { ago, api, ApiError, hhmm, type Handoff, type InboxItem, type InboxKind } from "../api";
import { HandoffActions } from "./HandoffActions";

import { Icon, type IconName } from "./Icon";
import { AskCard } from "./AskCard";
import { PermissionCard } from "./PermissionCard";
import { Markdown } from "./Markdown";
import { locale, t } from "../i18n";
import { isSendKey } from "../keys";
import { openExternal } from "../platform";

// Where the webview cannot size a textarea to its text, it is done by hand.
const SIZES_ITSELF = typeof CSS !== "undefined" && CSS.supports("field-sizing", "content");

const KINDS: Record<InboxKind, { label: string; icon: IconName; cls: string }> = {
  permission: { label: "Necesita un permiso", icon: "wait", cls: "c-wait" },
  question: { label: "Te pregunta", icon: "wait", cls: "c-wait" },
  error: { label: "Error en el turno", icon: "failed", cls: "c-err" },
  waiting: { label: "Te espera", icon: "wait", cls: "c-wait" },
  review: { label: "Lista para revisar", icon: "review", cls: "c-done" },
  handoff: { label: "Siguiente paso", icon: "run", cls: "c-done" },
  "suggest-review": { label: "¿Pasar a revisión?", icon: "review", cls: "c-done" },
  paused: { label: "Pausada por límite", icon: "pause", cls: "c-pause" },
  failed: { label: "Fallida", icon: "failed", cls: "c-err" },
  "pr-failed": { label: "CI falló en la PR", icon: "failed", cls: "c-err" },
  "pr-review": { label: "Comentarios en la PR", icon: "review", cls: "c-wait" },
  "pr-blocked": { label: "No se pudo archivar", icon: "failed", cls: "c-err" },
  autopilot: { label: "Autopiloto en pausa", icon: "pause", cls: "c-pause" },
};

interface Props {
  items: InboxItem[];
  /** Opens the task, on one session's tab if given. */
  onOpenTask: (task: string, sessionId?: string) => void;
  onOpenChanges: (task: string) => void;
  onChanged: () => void;
}

/** Everything that waits on you, from every task: what blocks an agent first. */
export function InboxView({ items, onOpenTask, onOpenChanges, onChanged }: Props) {
  return (
    <>
      <div className="view-head" data-tauri-drag-region>
        <h1 data-tauri-drag-region>{t("Bandeja")}</h1>
        <span className="muted" style={{ fontSize: 12 }}>
          {t("lo que te espera, de todas las tareas")}
        </span>
      </div>
      {items.length === 0 ? (
        <div className="empty">
          <Icon name="done" className="c-done" />
          <span>{t("Nada pendiente. Los agentes siguen a lo suyo.")}</span>
        </div>
      ) : (
        <div className="view-body inbox">
          {items.map((item) => (
            <Item
              key={`${item.kind}:${item.task}:${item.session?.id ?? ""}`}
              item={item}
              onOpenTask={onOpenTask}
              onOpenChanges={onOpenChanges}
              onChanged={onChanged}
            />
          ))}
        </div>
      )}
    </>
  );
}

function Item({ item, onOpenTask, onOpenChanges, onChanged }: { item: InboxItem } & Omit<Props, "items">) {
  const [reply, setReply] = useState("");
  const replyRef = useRef<HTMLTextAreaElement>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [expanded, setExpanded] = useState(false);
  const [paused, setPaused] = useState(false);
  const [confirming, setConfirming] = useState(false);
  useLayoutEffect(() => {
    const el = replyRef.current;
    if (!el || SIZES_ITSELF) return;
    el.style.height = "auto";
    el.style.height = `${el.scrollHeight + el.offsetHeight - el.clientHeight}px`;
  }, [reply]);
  // The item carries no task: the stage of its handoff decides the buttons.
  const [handoff, setHandoff] = useState<Handoff | null>(null);
  useEffect(() => {
    if (item.kind !== "handoff") return;
    let live = true;
    api
      .tasks()
      .then((ts) => live && setHandoff(ts.find((x) => x.key === item.task)?.handoff ?? null))
      .catch(() => {});
    return () => {
      live = false;
    };
  }, [item.kind, item.task, item.text]);
  const kind = KINDS[item.kind];
  const s = item.session;
  const long = (item.text?.length ?? 0) > 500 || (item.text?.split("\n").length ?? 0) > 8;
  const canReply = (item.kind === "question" || item.kind === "waiting") && s?.live && !item.ask;

  const act = async (action: () => Promise<unknown>) => {
    setBusy(true);
    setError(null);
    try {
      await action();
      onChanged();
    } catch (e) {
      if (e instanceof ApiError && e.status === 409 && e.resumeAt && item.pr) setPaused(true);
      setError((e as Error).message);
    } finally {
      setBusy(false);
    }
  };
  const send = () => {
    if (!s || !reply.trim()) return;
    act(async () => {
      await api.reply(s.id, reply);
      setReply("");
    });
  };

  return (
    <article className="card inbox-item">
      <header className="inbox-head">
        <Icon name={kind.icon} className={kind.cls} />
        <b className={kind.cls}>{t(kind.label)}</b>
        <button className="btn btn-ghost inbox-task" onClick={() => onOpenTask(item.task, s?.id)}>
          <span className="mono muted">{item.task}</span>
          <span className="trunc">{item.title}</span>
        </button>
        {s && !s.primary && <span className="tag">{s.label}</span>}
        {item.since && item.kind !== "paused" && (
          <span className="muted" style={{ marginLeft: "auto", fontSize: 11.5, flex: "none" }} title={new Date(item.since).toLocaleString(locale())}>
            {ago(item.since)}
          </span>
        )}
      </header>

      {item.ask && <AskCard ask={item.ask} onDone={onChanged} onOpenTask={(key) => onOpenTask(key)} />}
      {item.permission && s && <PermissionCard sessionId={s.id} ask={item.permission} onDone={onChanged} />}
      {item.text && !item.ask && !(item.permission && s) && (
        <div className={`inbox-text ${long && !expanded ? "clamped" : ""}`}>
          <Markdown text={item.text} remoteImages="click" />
        </div>
      )}
      {long && !item.ask && !(item.permission && s) && (
        <button className="btn btn-ghost" style={{ alignSelf: "flex-start" }} onClick={() => setExpanded((e) => !e)}>
          {expanded ? t("Ver menos") : t("Ver todo")}
        </button>
      )}
      {item.kind === "paused" && item.since && (
        <div className="muted" style={{ fontSize: 12.5 }}>
          {t("Se reanuda sola a las {time}.", { time: hhmm(item.since) })}
        </div>
      )}

      {canReply && (
        <form
          className="inbox-reply"
          onSubmit={(e) => {
            e.preventDefault();
            send();
          }}
        >
          <textarea
            ref={replyRef}
            className="input"
            rows={1}
            value={reply}
            onChange={(e) => setReply(e.target.value)}
            onKeyDown={(e) => {
              if (isSendKey(e.nativeEvent)) {
                e.preventDefault();
                send();
              }
            }}
            placeholder={t("Responder… (Enter envía, ⇧Enter nueva línea)")}
            aria-label={t("Responder a {task}", { task: item.task })}
            disabled={busy}
          />
          <button className="btn btn-p" disabled={busy || !reply.trim()}>{t("Enviar")}</button>
        </form>
      )}

      <div className="inbox-actions">
        {s && s.live && (
          <button className={`btn ${item.kind === "permission" ? "btn-p" : ""}`} onClick={() => onOpenTask(item.task, s.id)}>
            <Icon name="console" /> {item.kind === "permission" ? t("Responder en su terminal") : t("Abrir la sesión")}
          </button>
        )}
        {s && !s.live && (
          <>
            <span className="muted" style={{ fontSize: 12 }}>{t("La sesión está parada.")}</span>
            <button
              className="btn btn-p"
              disabled={busy}
              onClick={() => act(async () => (await api.reopenSession(s.id), onOpenTask(item.task, s.id)))}
            >
              {t("Reabrir y responder")}
            </button>
          </>
        )}
        {item.kind === "waiting" && !s && (
          <button className="btn" onClick={() => onOpenTask(item.task)}>
            <Icon name="console" /> {t("Abrir la tarea")}
          </button>
        )}
        {item.kind === "review" && (
          <>
            <button className="btn btn-p" onClick={() => onOpenChanges(item.task)}>
              <Icon name="diff" /> {t("Ver cambios")}
            </button>
            <button className="btn" onClick={() => onOpenTask(item.task)}>{t("Abrir la tarea")}</button>
          </>
        )}
        {item.kind === "suggest-review" && (
          <>
            <button className="btn btn-p" disabled={busy} onClick={() => act(() => api.setPhase(item.task, "review"))}>
              <Icon name="review" /> {t("Pasar a revisión")}
            </button>
            <button className="btn" onClick={() => onOpenChanges(item.task)}>
              <Icon name="diff" /> {t("Ver cambios")}
            </button>
            <button className="btn btn-ghost" disabled={busy} onClick={() => act(() => api.dismissReviewHint(item.task))}>
              {t("Todavía no")}
            </button>
          </>
        )}
        {item.kind === "paused" && (
          <button className="btn" disabled={busy} onClick={() => act(() => api.resume(item.task))}>
            {t("Reanudar ahora")}
          </button>
        )}
        {item.kind === "autopilot" && (
          <>
            <button className="btn btn-p" disabled={busy} onClick={() => act(() => api.setAutopilot(item.task, "resume"))}>
              {t("Reanudar autopiloto")}
            </button>
            <button className="btn btn-ghost" disabled={busy} onClick={() => act(() => api.setAutopilot(item.task, "off"))}>
              {t("Seguir a mano")}
            </button>
          </>
        )}
        {item.kind === "handoff" && (
          <HandoffActions
            task={item.task}
            handoff={handoff}
            onDone={onChanged}
            hideError={!!handoff?.error && !!item.text?.includes(handoff.error)}
          />
        )}
        {(item.kind === "pr-failed" || item.kind === "pr-review") && item.pr && (
          <>
            <button className="btn btn-p" disabled={busy || paused} onClick={() => act(() => api.forwardNotice(item.pr!.notice))}>
              {t("Pedir al agente que lo arregle")}
            </button>
            <button className="btn btn-ghost" disabled={busy} onClick={() => act(() => api.dismissNotice(item.pr!.notice))}>
              {t("Descartar")}
            </button>
          </>
        )}
        {item.kind === "pr-blocked" && item.pr && !confirming && (
          <>
            <button className="btn btn-p" disabled={busy} onClick={() => setConfirming(true)}>
              {t("Archivar igualmente")}
            </button>
            <button className="btn btn-ghost" disabled={busy} onClick={() => act(() => api.dismissNotice(item.pr!.notice))}>
              {t("Descartar")}
            </button>
          </>
        )}
        {item.kind === "pr-blocked" && item.pr && confirming && (
          <>
            <span className="muted" style={{ fontSize: 12.5 }}>{t("Se quitará el workspace y las ramas locales que queden. Lo no subido se perderá si lo hay. ¿Archivar igualmente?")}</span>
            <button
              className="btn btn-p"
              disabled={busy}
              onClick={() =>
                act(async () => {
                  await api.archive(item.task, true);
                  await api.dismissNotice(item.pr!.notice);
                })
              }
            >
              {t("Sí, archivar")}
            </button>
            <button className="btn btn-ghost" disabled={busy} onClick={() => setConfirming(false)}>
              {t("Cancelar")}
            </button>
          </>
        )}
        {item.pr && (
          <button className="btn" onClick={() => openExternal(item.pr!.url).catch(() => {})}>
            {t("Ver PR")}
          </button>
        )}
        {item.kind === "failed" && (
          <button className="btn" onClick={() => onOpenTask(item.task)}>{t("Abrir la tarea")}</button>
        )}
      </div>
      {error && <div className="error selectable">{error}</div>}
    </article>
  );
}
