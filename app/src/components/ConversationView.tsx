import { useCallback, useEffect, useRef, useState } from "react";
import {
  api,
  liveSocket,
  type Conversation,
  type ConversationEntry,
  type Session,
} from "../api";
import { on } from "../live";
import { t } from "../i18n";
import { AskCard } from "./AskCard";
import { Markdown } from "./Markdown";
import { ToolCard } from "./ToolCard";
import { Button, Skeleton } from "./ui";
import "./ConversationView.css";

interface Props {
  session: Session;
  onOpenTerminal: () => void;
  onOpenTask: (key: string) => void;
  onReopen: () => void;
  /** Opens a file in the editor (the path of an edit card). */
  onOpenFile?: (path: string) => void;
}

const noop = () => {};

/** Folds a response into what is on screen. Without a cursor it is everything; with one,
 *  the stored entries (ids not starting with "t:") are replaced by the new set and the
 *  transcript ones are upserted by id (a tool re-sent with its result replaces the earlier
 *  one in place). The result is ordered by time, keeping the order of equal ones. */
/** Entries come with "…000Z" and "+00:00" stamps; compare instants, not strings. An
 *  empty or unparsable stamp counts as 0, so those keep their arrival order. */
const time = (at: string) => Date.parse(at) || 0;

function merge(
  prev: ConversationEntry[],
  next: ConversationEntry[],
  incremental: boolean,
): ConversationEntry[] {
  if (!incremental) return next;
  const out = prev.filter((e) => e.id.startsWith("t:"));
  for (const e of next) {
    const i = e.id.startsWith("t:") ? out.findIndex((o) => o.id === e.id) : -1;
    if (i >= 0) out[i] = e;
    else out.push(e);
  }
  // A card stands in for its AskUserQuestion call, whichever response brought each.
  const asked = new Set(out.flatMap((e) => (e.kind === "card" && e.tool_use_id ? [e.tool_use_id] : [])));
  return out
    .filter((e) => !(e.kind === "tool" && asked.has(e.tool_use_id)))
    .map((e, i) => [e, i] as const)
    .sort(([a, i], [b, j]) => time(a.at) - time(b.at) || i - j)
    .map(([e]) => e);
}

/** Whether the transcript entry of a message being written is in the conversation. */
function arrived(msg: string, w: { seen?: Set<string> }, entries: ConversationEntry[]): boolean {
  return entries.some(
    (e) =>
      e.kind === "claude" &&
      (e.message_id !== undefined ? e.message_id === msg : e.id.startsWith(`t:${msg}:`) || (!!w.seen && !w.seen.has(e.id))),
  );
}

/** An idea conversation without a terminal: messages, Claude's replies, questions as
 *  cards and a field to write. Sending is allowed while Claude works (it queues). */
export function ConversationView({
  session,
  onOpenTerminal,
  onOpenTask,
  onReopen,
  onOpenFile,
}: Props) {
  const [conv, setConv] = useState<Conversation | null>(null);
  const [text, setText] = useState("");
  const [error, setError] = useState<string | null>(null);
  const log = useRef<HTMLDivElement>(null);
  const id = session.id;

  // Only the latest request may write: a slow earlier one, or one from the previous
  // session, is ignored.
  const seq = useRef(0);
  const stick = useRef(true);
  // Where the daemon left off; empty until the first answer.
  const cursor = useRef<string | null>(null);

  const load = useCallback(() => {
    const mine = ++seq.current;
    const from = cursor.current;
    (from ? api.conversation(id, from) : api.conversation(id)).then(
      (c) => {
        if (mine !== seq.current) return;
        cursor.current = c.cursor;
        setConv((prev) => ({
          ...c,
          entries: merge(prev?.entries ?? [], c.entries, !!from),
        }));
        setError(null);
        // More transcript is waiting: take it without waiting for an event.
        if (!c.complete) load();
      },
      (e) => {
        if (mine === seq.current) setError((e as Error).message);
      },
    );
  }, [id]);

  useEffect(() => {
    cursor.current = null;
    setConv(null);
    setError(null);
    stick.current = true;
  }, [id]);

  // Chat mode (stream runtime): the text of the messages being written, by message id, from
  // the live socket. A bubble goes away once its transcript entry is in the conversation.
  // A claude entry carries the id of its message (`message_id`): the bubble goes away exactly
  // when the entry of its message arrives. For an entry without one, besides an id of the
  // form `t:<message id>:<n>` a finished bubble is replaced by the first claude entry that
  // was not on screen when its `done` arrived.
  const [writing, setWriting] = useState<Map<string, { text: string; seen?: Set<string> }>>(() => new Map());
  const claudeIds = useRef<Set<string>>(new Set());
  claudeIds.current = new Set((conv?.entries ?? []).filter((e) => e.kind === "claude").map((e) => e.id));
  // Only a running chat agent has a live socket: a closed one answers 404, which would
  // otherwise be retried forever.
  const stream = session.runtime === "stream" && session.live;
  useEffect(() => {
    setWriting(new Map());
    if (!stream) return;
    let closed = false;
    let ws: WebSocket | null = null;
    let timer: ReturnType<typeof setTimeout> | undefined;
    let delay = 1000;
    const connect = () => {
      const sock = liveSocket(id);
      ws = sock;
      sock.onmessage = (ev) => {
        delay = 1000;
        let m: { message?: string; text?: string; done?: boolean; reset?: boolean };
        try {
          m = JSON.parse(String(ev.data));
        } catch {
          return;
        }
        // The daemon dropped deltas (a slow socket): the bubbles have gaps, the
        // transcript has the text.
        if (m.reset) {
          setWriting(new Map());
          load();
          return;
        }
        const msg = m.message;
        if (!msg) return;
        const seen = m.done ? new Set(claudeIds.current) : undefined;
        setWriting((cur) => {
          const next = new Map(cur);
          const old = next.get(msg);
          if (m.done) {
            if (old) next.set(msg, { ...old, seen });
          } else if (m.text) {
            next.set(msg, { text: (old?.text ?? "") + m.text });
          }
          return next;
        });
        if (m.done) load();
      };
      // The stream agent can restart under the same session id: reconnect with a backoff,
      // except on 4409 (the session is not in chat mode).
      sock.onclose = (ev) => {
        if (closed || ev?.code === 4409) return;
        timer = setTimeout(connect, delay);
        delay = Math.min(delay * 2, 10000);
      };
    };
    connect();
    return () => {
      closed = true;
      clearTimeout(timer);
      if (ws) {
        ws.onmessage = null;
        ws.onclose = null;
        ws.close();
      }
    };
  }, [id, stream, load]);
  useEffect(() => {
    setWriting((cur) => {
      let next: Map<string, { text: string; seen?: Set<string> }> | null = null;
      for (const [msg, w] of cur) {
        if (arrived(msg, w, conv?.entries ?? [])) (next ??= new Map(cur)).delete(msg);
      }
      return next ?? cur;
    });
  }, [conv]);
  // Filtered at render too: between the entry arriving and the effect above dropping
  // its bubble, one frame would show the message twice.
  const shownWriting = [...writing].filter(([msg, w]) => !arrived(msg, w, conv?.entries ?? []));

  useEffect(() => {
    load();
    const offs = [on("conversations", load), on("inbox", load)];
    return () => offs.forEach((off) => off());
  }, [load]);

  const count = (conv?.entries.length ?? 0) + shownWriting.length;

  useEffect(() => {
    const el = log.current;
    if (el && stick.current) el.scrollTop = el.scrollHeight;
  }, [count, id]);

  const send = async () => {
    const msg = text.trim();
    if (!msg) return;
    setText("");
    setError(null);
    try {
      await api.sendMessage(id, msg);
      load();
    } catch (e) {
      // Restore the message unless the user already typed something else meanwhile.
      setText((cur) => (cur.trim() ? cur : msg));
      setError((e as Error).message);
    }
  };

  const guard = (p: Promise<unknown>) =>
    p.then(load, (e) => setError((e as Error).message));
  const working = session.state === "working";
  // Any open Claude session takes messages; a closed one is reopened first and a shell has no chat.
  const canCompose = !session.closed && session.kind !== "shell";

  return (
    <div className="conv">
      <div
        className="conv-log"
        role="log"
        aria-label={t("Conversación")}
        ref={log}
        onScroll={(ev) => {
          const el = ev.currentTarget;
          stick.current = el.scrollHeight - el.scrollTop - el.clientHeight < 40;
        }}
      >
        {!conv && !error && <Skeleton lines={3} />}
        {conv &&
          conv.entries.length === 0 &&
          conv.transcript === "present" &&
          session.conversation && (
            <div className="muted">{t("Cuéntale tu idea…")}</div>
          )}
        {conv && conv.entries.length === 0 && conv.transcript === "pending" && (
          <div className="muted">
            {t("Esperando la conversación de Claude…")}
          </div>
        )}
        {conv && conv.entries.length === 0 && conv.transcript === "missing" && (
          <div className="conv-notice">
            {t("No hay conversación que mostrar")}
            <button className="btn" onClick={onOpenTerminal}>
              {t("Abrir terminal")}
            </button>
          </div>
        )}
        {conv?.entries.map((e) => {
          if (e.kind === "user") {
            return (
              <div key={e.id} className="conv-msg conv-user selectable">
                {e.text}
                {e.state === "queued" && (
                  <div className="conv-meta muted">
                    {t("en cola")}
                    <button
                      className="btn btn-ghost"
                      aria-label={t("Quitar mensaje")}
                      title={t("Quitar mensaje")}
                      onClick={() => guard(api.removeMessage(id, e.id))}
                    >
                      ×
                    </button>
                  </div>
                )}
              </div>
            );
          }
          if (e.kind === "claude") {
            return (
              <div key={e.id} className="conv-msg conv-claude selectable">
                <Markdown text={e.text} remoteImages="click" />
              </div>
            );
          }
          if (e.kind === "tool")
            return (
              <ToolCard
                key={e.id}
                entry={e}
                sessionId={id}
                onOpenFile={onOpenFile ?? noop}
              />
            );
          if (e.kind === "marker") {
            return (
              <div key={e.id} className="conv-marker muted">
                {e.marker === "compacted"
                  ? t("Conversación compactada")
                  : t("Detenida: detuviste a Claude.")}
              </div>
            );
          }
          if (e.ask_kind === "permission") {
            const ask = { id: e.ask, tool: e.tool, summary: e.summary, suggestion: e.suggestion, rule: e.rule };
            return (
              <div key={e.id} className="conv-card">
                <AskCard
                  variant="chat"
                  permission={{
                    sessionId: id,
                    ask,
                    ended: e.outcome === "pending" ? undefined : { outcome: e.outcome === "answered" ? "answered" : e.outcome === "withdrawn" ? "withdrawn" : "expired", decision: e.decision ?? undefined },
                  }}
                  onDone={load}
                />
              </div>
            );
          }
          const pending = e.outcome === "pending";
          return (
            <div key={e.id} className="conv-card">
              {pending ? (
                <AskCard
                  variant="chat"
                  ask={{
                    id: e.ask,
                    questions: e.questions,
                    proposal: e.proposal,
                  }}
                  onDone={load}
                  onOpenTask={onOpenTask}
                />
              ) : (
                <div className="selectable">
                  {e.proposal && <b>{e.proposal.title}</b>}
                  {e.answer && (
                    <div className="muted">
                      {Object.entries(e.answer).map(([q, a]) => (
                        <div key={q}>
                          {q}: {a}
                        </div>
                      ))}
                    </div>
                  )}
                  {e.task && (
                    <div>
                      {t("Creada")}{" "}
                      <a
                        href="#"
                        onClick={(ev) => {
                          ev.preventDefault();
                          onOpenTask(e.task!);
                        }}
                      >
                        {e.task}
                      </a>
                    </div>
                  )}
                  {e.outcome === "terminal" && (
                    <div className="muted">
                      {t("Respondida en la terminal")}
                    </div>
                  )}
                  {e.outcome === "withdrawn" && (
                    <div className="muted">{t("Retirada")}</div>
                  )}
                  {e.error && <div className="error">{e.error}</div>}
                </div>
              )}
            </div>
          );
        })}
        {shownWriting.map(([msg, w]) => (
          <div key={`w:${msg}`} className="conv-msg conv-claude conv-writing selectable" data-testid="writing">
            <Markdown text={w.text} remoteImages="click" />
          </div>
        ))}
      </div>
      {working && conv?.activity && (
        <div className="conv-activity muted">{conv.activity}</div>
      )}
      {conv?.needs_terminal && (
        <div className="conv-notice">
          {t("Claude espera en la terminal")}
          <button className="btn" onClick={onOpenTerminal}>
            {t("Abrir terminal")}
          </button>
        </div>
      )}
      {!session.live && (
        <div className="conv-notice">
          <button className="btn" onClick={onReopen}>
            {t("Reabrir")}
          </button>
        </div>
      )}
      {error && <div className="error selectable conv-notice">{error}</div>}
      {canCompose && (
        <div className="conv-compose">
          <textarea
            className="input"
            rows={2}
            placeholder={t("Escribe un mensaje…")}
            aria-label={t("Mensaje")}
            value={text}
            onChange={(e) => setText(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) {
                e.preventDefault();
                send();
              }
            }}
          />
          <Button variant="primary" disabled={!text.trim()} title={text.trim() ? undefined : t("Escribe un mensaje")} onClick={send}>
            {t("Enviar")} <span className="conv-key">⌘↵</span>
          </Button>
          {working && (
            <Button variant="secondary" onClick={() => guard(api.stopSession(id))}>
              {t("Detener")}
            </Button>
          )}
        </div>
      )}
    </div>
  );
}
