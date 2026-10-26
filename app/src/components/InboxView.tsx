import { useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import { api, ApiError, hhmm, type Handoff, type InboxItem } from "../api";
import { HandoffActions } from "./HandoffActions";
import { Icon } from "./Icon";
import { AskCard } from "./AskCard";
import { locale, t } from "../i18n";
import { isSendKey } from "../keys";
import { on } from "../live";
import { openExternal } from "../platform";
import { relativeTime } from "../time";
import { useBreakpoints } from "../useWidth";
import { kindOf } from "../vocab";
import { Button, Count, InboxKind, Row, Skeleton, pushToast } from "./ui";
import "./InboxView.css";

// Where the webview cannot size a textarea to its text, it is done by hand.
const SIZES_ITSELF = typeof CSS !== "undefined" && CSS.supports("field-sizing", "content");

/** Notices that stay Spanish keys because the daemon sends the kind. */
const NOTICE_TITLE: Partial<Record<InboxItem["kind"], string>> = {
  error: "Error en el turno",
  review: "Lista para revisar",
  handoff: "Siguiente paso",
  "suggest-review": "¿Pasar a revisión?",
  paused: "Pausada por límite",
  failed: "Fallida",
  "pr-failed": "CI falló en la PR",
  "pr-review": "Comentarios en la PR",
  "pr-blocked": "No se pudo archivar",
  "base-stale": "Rama posiblemente atrasada",
  branch: "Rama sin crear",
  autopilot: "Autopiloto en pausa",
  "turn-done": "Terminó su turno",
  "pr-opened": "PR abierta",
  waiting: "Te espera",
  question: "Te pregunta",
  permission: "Necesita un permiso",
};

const idOf = (i: InboxItem) => i.id ?? `${i.kind}:${i.task}:${i.session?.id ?? ""}`;

interface Props {
  items: InboxItem[];
  /** True until the first fetch of the inbox came back. */
  loading?: boolean;
  /** Opens the task, on one session's tab if given. */
  onOpenTask: (task: string, sessionId?: string) => void;
  onOpenChanges: (task: string) => void;
  onChanged: () => void;
}

interface Group {
  task: string;
  title: string;
  items: InboxItem[];
}

const isTextTarget = (el: EventTarget | null) => {
  const e = el as HTMLElement | null;
  return !!e && (/^(INPUT|TEXTAREA|SELECT)$/.test(e.tagName) || e.isContentEditable);
};

/** Everything that waits on you, from every task: a list grouped by task and the detail of one. */
export function InboxView({ items, loading, onOpenTask, onOpenChanges, onChanged }: Props) {
  const [selected, setSelected] = useState<string | null>(null);
  // Below 1200 px the detail is an overlay that only opens when a row is chosen.
  const { overlays } = useBreakpoints();
  const [detailOpen, setDetailOpen] = useState(false);
  const groups = useMemo(() => {
    const by = new Map<string, Group>();
    for (const i of items) {
      const g = by.get(i.task) ?? { task: i.task, title: i.title, items: [] };
      g.items.push(i);
      by.set(i.task, g);
    }
    return [...by.values()];
  }, [items]);
  const flat = useMemo(() => groups.flatMap((g) => g.items), [groups]);
  const current = flat.find((i) => idOf(i) === selected) ?? flat[0];
  const currentId = current ? idOf(current) : null;
  const group = groups.find((g) => g.task === current?.task);

  const showOverlay = overlays && detailOpen && !!current;
  useEffect(() => {
    if (detailOpen && selected && !flat.some((i) => idOf(i) === selected)) setDetailOpen(false);
  }, [flat, selected, detailOpen]);
  useEffect(() => {
    if (!showOverlay) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") setDetailOpen(false);
    };
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, [showOverlay]);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (isTextTarget(e.target) || e.metaKey || e.ctrlKey || e.altKey) return;
      if (e.key !== "ArrowDown" && e.key !== "ArrowUp") return;
      const at = flat.findIndex((i) => idOf(i) === currentId);
      const next = flat[Math.min(flat.length - 1, Math.max(0, at + (e.key === "ArrowDown" ? 1 : -1)))];
      if (next) {
        e.preventDefault();
        setSelected(idOf(next));
      }
    };
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, [flat, currentId]);

  const clearable = items.filter((i) => kindOf(i.kind).clearable && i.id);
  const clear = async () => {
    try {
      const { ids } = await api.clearInbox();
      onChanged();
      if (ids.length === 0) return;
      pushToast({
        text: ids.length === 1 ? t("1 aviso quitado") : t("{n} avisos quitados", { n: ids.length }),
        action: {
          label: t("Deshacer"),
          run: () =>
            api
              .restoreInbox(ids)
              .then(onChanged)
              .catch((e) => pushToast({ kind: "error", text: (e as Error).message })),
        },
      });
    } catch (e) {
      pushToast({ kind: "error", text: (e as Error).message });
    }
  };

  return (
    <>
      <div className="view-head" data-tauri-drag-region>
        <h1 data-tauri-drag-region>{t("Bandeja")}</h1>
        <span className="muted ib-sub">{t("lo que te espera, de todas las tareas")}</span>
        <Button
          variant="secondary"
          size={28}
          className="ib-clear"
          disabled={clearable.length === 0}
          title={clearable.length === 0 ? t("Solo quedan elementos que esperan una respuesta tuya.") : undefined}
          onClick={clear}
        >
          {t("Limpiar avisos")}
        </Button>
      </div>
      {loading && items.length === 0 ? (
        <div className="ib-loading">
          <Skeleton lines={4} />
        </div>
      ) : items.length === 0 ? (
        <Empty />
      ) : (
        <div className={`ib ${overlays ? "ib-narrow" : ""}`}>
          <div className="ib-list" role="listbox" aria-label={t("Bandeja")}>
            {groups.map((g) => (
              <div key={g.task} className="ib-group" role="group" aria-label={g.task}>
                <div className="ib-ghead">
                  <span className="mono">{g.task}</span>
                  <span className="trunc">{g.title}</span>
                  <Count n={g.items.length} showZero />
                </div>
                {g.items.map((i) => (
                  <Row
                    key={idOf(i)}
                    role="option"
                    selected={idOf(i) === currentId}
                    title={<InboxKind kind={i.kind} />}
                    sub={<span className="trunc">{i.text ?? t(NOTICE_TITLE[i.kind] ?? "Aviso")}</span>}
                    trailing={i.since && i.kind !== "paused" ? <span className="ib-when">{relativeTime(i.since)}</span> : undefined}
                    onClick={() => {
                      setSelected(idOf(i));
                      setDetailOpen(true);
                    }}
                  />
                ))}
              </div>
            ))}
          </div>
          {(!overlays || showOverlay) && (
            <>
              {overlays && <div className="ib-scrim" onClick={() => setDetailOpen(false)} />}
              <div
                className={`ib-detail ${overlays ? "ib-overlay" : ""}`}
                role={overlays ? "dialog" : undefined}
                aria-label={overlays ? t("Detalle del aviso") : undefined}
              >
                {overlays && (
                  <div className="ib-close">
                    <Button variant="secondary" size={28} onClick={() => setDetailOpen(false)}>
                      {t("Cerrar")}
                    </Button>
                  </div>
                )}
                {current && (
                  <Item
                    key={idOf(current)}
                    item={current}
                    nav={
                      group && group.items.length > 1
                        ? { index: group.items.indexOf(current), total: group.items.length, onChange: (n) => setSelected(idOf(group.items[n])) }
                        : undefined
                    }
                    onOpenTask={onOpenTask}
                    onOpenChanges={onOpenChanges}
                    onChanged={onChanged}
                  />
                )}
              </div>
            </>
          )}
        </div>
      )}
    </>
  );
}

/** «Nada te espera» and when the agents last did something, worked out here from sessions and events. */
function Empty() {
  const [last, setLast] = useState<{ task: string; text: string | null; at: string } | null>(null);
  useEffect(() => {
    let live = true;
    const load = async () => {
      try {
        const sessions = await api.sessions();
        const newest = sessions
          .filter((s) => s.last_activity_at)
          .sort((a, b) => Date.parse(b.last_activity_at!) - Date.parse(a.last_activity_at!))[0];
        if (newest) return live && setLast({ task: newest.task, text: newest.last_message ?? null, at: newest.last_activity_at! });
        const tasks = await api.tasks();
        let best: { task: string; text: string | null; at: string } | null = null;
        for (const task of tasks.slice(0, 5)) {
          const ev = (await api.events(task.key)).at(-1);
          if (ev && (!best || Date.parse(ev.at) > Date.parse(best.at))) best = { task: task.key, text: null, at: ev.at };
        }
        if (live) setLast(best);
      } catch {
        /* no line without data */
      }
    };
    load();
    const off = [on("sessions", load), on("tasks", load)];
    return () => {
      live = false;
      off.forEach((f) => f());
    };
  }, []);
  return (
    <div className="empty ib-empty">
      <Icon name="done" className="c-done" />
      <b>{t("Nada te espera")}</b>
      {last && (
        <span className="muted ib-last">
          {t("Última actividad: {task}{text} · {time}", {
            task: last.task,
            text: last.text ? ` · ${last.text.slice(0, 120)}` : "",
            time: relativeTime(last.at),
          })}
        </span>
      )}
    </div>
  );
}

function Item({ item, nav, onOpenTask, onOpenChanges, onChanged }: { item: InboxItem; nav?: NonNullable<Parameters<typeof AskCard>[0]["nav"]> } & Omit<Props, "items" | "loading">) {
  const [reply, setReply] = useState("");
  const replyRef = useRef<HTMLTextAreaElement>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
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
  const view = kindOf(item.kind);
  const s = item.session;
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
      // The branch item is stale (task gone, or already on its branch): let the list refresh.
      if (item.kind === "branch" && e instanceof ApiError && (e.status === 404 || e.status === 409)) onChanged();
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
    <article className="ib-item">
      <header className="ib-head">
        <InboxKind kind={item.kind} />
        <b className="ib-what">{t(NOTICE_TITLE[item.kind] ?? "Aviso")}</b>
        <button type="button" className="ib-task" onClick={() => onOpenTask(item.task, s?.id)}>
          <span className="mono muted">{item.task}</span>
          <span className="trunc">{item.title}</span>
        </button>
        {s && !s.primary && <span className="tag">{s.label}</span>}
        {item.since && item.kind !== "paused" && (
          <span className="muted ib-when" title={new Date(item.since).toLocaleString(locale())}>
            {relativeTime(item.since)}
          </span>
        )}
      </header>

      {item.ask ? (
        <AskCard variant="inbox" keys nav={nav} ask={item.ask} onDone={onChanged} onOpenTask={(key) => onOpenTask(key)} />
      ) : item.permission && s ? (
        <AskCard variant="inbox" keys nav={nav} permission={{ sessionId: s.id, ask: item.permission }} onDone={onChanged} />
      ) : (
        item.text && (
          <AskCard
            variant="inbox"
            keys
            nav={nav}
            notice={{
              text: item.kind === "branch" ? t("No se pudo crear la rama: {text}", { text: item.text }) : item.text,
              onDismiss: view.clearable && item.id ? () => act(() => api.dismissInbox(item.id!)) : undefined,
            }}
          />
        )
      )}
      {item.kind === "paused" && item.since && (
        <div className="muted">
          {t("Se reanuda sola a las {time}.", { time: hhmm(item.since) })}
        </div>
      )}

      {canReply && (
        <form
          className="ib-reply"
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

      <div className="ib-actions">
        {/* A chat-mode ask has no terminal: its card is the way to answer. */}
        {s && s.live && !item.permission && (
          <button className={`btn ${item.kind === "permission" ? "btn-p" : ""}`} onClick={() => onOpenTask(item.task, s.id)}>
            <Icon name="console" /> {item.kind === "permission" ? t("Responder en su terminal") : t("Abrir la sesión")}
          </button>
        )}
        {s && !s.live && (
          <>
            <span className="muted">{t("La sesión está parada.")}</span>
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
              <Icon name="diff" /> {t("Abrir cambios")}
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
              <Icon name="diff" /> {t("Abrir cambios")}
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
            <span className="muted">{t("Se quitará el workspace y las ramas locales que queden. Lo no subido se perderá si lo hay. ¿Archivar igualmente?")}</span>
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
        {item.kind === "base-stale" && item.notice != null && (
          <button className="btn btn-ghost" disabled={busy} onClick={() => act(() => api.dismissNotice(item.notice!))}>
            {t("Descartar")}
          </button>
        )}
        {item.pr && (
          <button className="btn" onClick={() => openExternal(item.pr!.url).catch(() => {})}>
            {t("Abrir PR")}
          </button>
        )}
        {item.kind === "branch" && (
          <button className="btn btn-p" disabled={busy} onClick={() => act(() => api.retryBranch(item.task))}>
            {t("Reintentar")}
          </button>
        )}
        {item.kind === "turn-done" && (
          <button className="btn" onClick={() => onOpenTask(item.task)}>{t("Abrir la tarea")}</button>
        )}
        {item.kind === "failed" && (
          <button className="btn" onClick={() => onOpenTask(item.task)}>{t("Abrir la tarea")}</button>
        )}
      </div>
      {error && <div className="error selectable">{error}</div>}
    </article>
  );
}
