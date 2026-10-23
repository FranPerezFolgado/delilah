import { useCallback, useEffect, useRef, useState } from "react";
import { api, type Conversation, type Session } from "../api";
import { on } from "../live";
import { t } from "../i18n";
import { AskCard } from "./AskCard";
import { Markdown } from "./Markdown";

interface Props {
  session: Session;
  onOpenTerminal: () => void;
  onOpenTask: (key: string) => void;
  onReopen: () => void;
}

/** An idea conversation without a terminal: messages, Claude's replies, questions as
 *  cards and a field to write. Sending is allowed while Claude works (it queues). */
export function ConversationView({ session, onOpenTerminal, onOpenTask, onReopen }: Props) {
  const [conv, setConv] = useState<Conversation | null>(null);
  const [text, setText] = useState("");
  const [error, setError] = useState<string | null>(null);
  const log = useRef<HTMLDivElement>(null);
  const id = session.id;

  // Only the latest request may write: a slow earlier one, or one from the previous
  // session, is ignored.
  const seq = useRef(0);
  const stick = useRef(true);

  const load = useCallback(() => {
    const mine = ++seq.current;
    api.conversation(id).then(
      (c) => {
        if (mine !== seq.current) return;
        setConv(c);
        setError(null);
      },
      (e) => {
        if (mine === seq.current) setError((e as Error).message);
      },
    );
  }, [id]);

  useEffect(() => {
    setConv(null);
    setError(null);
    stick.current = true;
  }, [id]);

  useEffect(() => {
    load();
    const offs = [on("conversations", load), on("inbox", load)];
    return () => offs.forEach((off) => off());
  }, [load]);

  const count = conv?.entries.length ?? 0;
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

  const guard = (p: Promise<unknown>) => p.then(load, (e) => setError((e as Error).message));
  const working = session.state === "working";

  return (
    <div className="conv">
      <div
        className="conv-log"
        ref={log}
        onScroll={(ev) => {
          const el = ev.currentTarget;
          stick.current = el.scrollHeight - el.scrollTop - el.clientHeight < 40;
        }}
      >
        {conv && conv.entries.length === 0 && <div className="muted">{t("Cuéntale tu idea…")}</div>}
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
          const pending = e.outcome === "pending";
          return (
            <div key={e.id} className="conv-card">
              {pending ? (
                <AskCard ask={{ id: e.ask, questions: e.questions, proposal: e.proposal }} onDone={load} onOpenTask={onOpenTask} />
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
                  {e.outcome === "terminal" && <div className="muted">{t("Respondida en la terminal")}</div>}
                  {e.outcome === "withdrawn" && <div className="muted">{t("Retirada")}</div>}
                  {e.error && <div className="error">{e.error}</div>}
                </div>
              )}
            </div>
          );
        })}
      </div>
      {working && conv?.activity && <div className="conv-activity muted">{conv.activity}</div>}
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
        <button className="btn btn-p" disabled={!text.trim()} onClick={send}>
          {t("Enviar")}
        </button>
        {working && (
          <button className="btn" onClick={() => guard(api.stopSession(id))}>
            {t("Detener")}
          </button>
        )}
      </div>
    </div>
  );
}
