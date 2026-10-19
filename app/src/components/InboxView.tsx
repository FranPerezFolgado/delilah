import { useState } from "react";
import { ago, api, hhmm, type InboxItem, type InboxKind } from "../api";
import { Icon, type IconName } from "./Icon";
import { AskCard } from "./AskCard";
import { Markdown } from "./Markdown";

const KINDS: Record<InboxKind, { label: string; icon: IconName; cls: string }> = {
  permission: { label: "Necesita un permiso", icon: "wait", cls: "c-wait" },
  question: { label: "Te pregunta", icon: "wait", cls: "c-wait" },
  error: { label: "Error en el turno", icon: "failed", cls: "c-err" },
  waiting: { label: "Te espera", icon: "wait", cls: "c-wait" },
  review: { label: "Lista para revisar", icon: "review", cls: "c-done" },
  paused: { label: "Pausada por límite", icon: "pause", cls: "c-pause" },
  failed: { label: "Fallida", icon: "failed", cls: "c-err" },
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
        <h1 data-tauri-drag-region>Bandeja</h1>
        <span className="muted" style={{ fontSize: 12 }}>
          lo que te espera, de todas las tareas
        </span>
      </div>
      {items.length === 0 ? (
        <div className="empty">
          <Icon name="done" className="c-done" />
          <span>Nada pendiente. Los agentes siguen a lo suyo.</span>
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
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [expanded, setExpanded] = useState(false);
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
        <b className={kind.cls}>{kind.label}</b>
        <button className="btn btn-ghost inbox-task" onClick={() => onOpenTask(item.task, s?.id)}>
          <span className="mono muted">{item.task}</span>
          <span className="trunc">{item.title}</span>
        </button>
        {s && !s.primary && <span className="tag">{s.label}</span>}
        {item.since && item.kind !== "paused" && (
          <span className="muted" style={{ marginLeft: "auto", fontSize: 11.5, flex: "none" }} title={new Date(item.since).toLocaleString("es-ES")}>
            {ago(item.since)}
          </span>
        )}
      </header>

      {item.ask && <AskCard ask={item.ask} onDone={onChanged} />}
      {item.text && !item.ask && (
        <div className={`inbox-text ${long && !expanded ? "clamped" : ""}`}>
          <Markdown text={item.text} />
        </div>
      )}
      {long && !item.ask && (
        <button className="btn btn-ghost" style={{ alignSelf: "flex-start" }} onClick={() => setExpanded((e) => !e)}>
          {expanded ? "Ver menos" : "Ver todo"}
        </button>
      )}
      {item.kind === "paused" && item.since && (
        <div className="muted" style={{ fontSize: 12.5 }}>
          Se reanuda sola a las <b>{hhmm(item.since)}</b>.
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
          <input
            className="input"
            value={reply}
            onChange={(e) => setReply(e.target.value)}
            placeholder="Responder… (Enter envía)"
            aria-label={`Responder a ${item.task}`}
            disabled={busy}
          />
          <button className="btn btn-p" disabled={busy || !reply.trim()}>Enviar</button>
        </form>
      )}

      <div className="inbox-actions">
        {s && s.live && (
          <button className={`btn ${item.kind === "permission" ? "btn-p" : ""}`} onClick={() => onOpenTask(item.task, s.id)}>
            <Icon name="console" /> {item.kind === "permission" ? "Responder en su terminal" : "Abrir la sesión"}
          </button>
        )}
        {s && !s.live && (
          <>
            <span className="muted" style={{ fontSize: 12 }}>La sesión está parada.</span>
            <button
              className="btn btn-p"
              disabled={busy}
              onClick={() => act(async () => (await api.reopenSession(s.id), onOpenTask(item.task, s.id)))}
            >
              Reabrir y responder
            </button>
          </>
        )}
        {item.kind === "waiting" && !s && (
          <button className="btn" onClick={() => onOpenTask(item.task)}>
            <Icon name="console" /> Abrir la tarea
          </button>
        )}
        {item.kind === "review" && (
          <>
            <button className="btn btn-p" onClick={() => onOpenChanges(item.task)}>
              <Icon name="diff" /> Ver cambios
            </button>
            <button className="btn" onClick={() => onOpenTask(item.task)}>Abrir la tarea</button>
          </>
        )}
        {item.kind === "paused" && (
          <button className="btn" disabled={busy} onClick={() => act(() => api.resume(item.task))}>
            Reanudar ahora
          </button>
        )}
        {item.kind === "failed" && (
          <button className="btn" onClick={() => onOpenTask(item.task)}>Abrir la tarea</button>
        )}
      </div>
      {error && <div className="error selectable">{error}</div>}
    </article>
  );
}
