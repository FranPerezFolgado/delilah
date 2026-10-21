import { useState } from "react";
import { api, type AskQuestion } from "../api";
import { t } from "../i18n";
import { Markdown } from "./Markdown";

interface Props {
  ask: { id: string; questions: AskQuestion[]; proposal?: { title: string; brief: string } | null };
  /** Opens the task a proposal created. */
  onOpenTask?: (key: string) => void;
  /** Called once answered or sent back to the terminal. */
  onDone?: () => void;
}

/** A question the agent asked with options (AskUserQuestion): pick and the agent goes
 *  on, as if chosen in its terminal. A single question answers on the first click. */
export function AskCard({ ask, onDone, onOpenTask }: Props) {
  const [picked, setPicked] = useState<Record<string, string[]>>({});
  const [own, setOwn] = useState<Record<string, string>>({});
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [created, setCreated] = useState<string | null>(null);
  const instant = ask.questions.length === 1 && !ask.questions[0].multiSelect;

  const act = async (action: () => Promise<unknown>) => {
    setBusy(true);
    setError(null);
    try {
      await action();
      onDone?.();
    } catch (e) {
      setError((e as Error).message);
      setBusy(false);
    }
  };

  const answerOf = (q: AskQuestion) => own[q.question]?.trim() || (picked[q.question] ?? []).join(", ");
  const complete = ask.questions.every((q) => answerOf(q));
  const send = (answers: Record<string, string>) =>
    act(async () => {
      const res = await api.answerAsk(ask.id, answers);
      if (res?.error) throw new Error(res.error);
      if (res?.task) setCreated(res.task);
    });

  const choose = (q: AskQuestion, label: string) => {
    if (instant) return send({ [q.question]: label });
    setPicked((p) => {
      const now = p[q.question] ?? [];
      const next = q.multiSelect ? (now.includes(label) ? now.filter((l) => l !== label) : [...now, label]) : [label];
      return { ...p, [q.question]: next };
    });
  };

  if (ask.proposal) {
    const q = ask.questions[0]?.question ?? "";
    return (
      <div className="ask ask-proposal">
        <h3 className="selectable">{ask.proposal.title}</h3>
        <div className="ask-preview">
          <Markdown text={ask.proposal.brief} remoteImages="click" />
        </div>
        {created ? (
          <div className="ask-created">
            {t("Creada")}{" "}
            <a
              href="#"
              onClick={(e) => {
                e.preventDefault();
                onOpenTask?.(created);
              }}
            >
              {created}
            </a>
          </div>
        ) : (
          <div className="ask-actions">
            <button className="btn btn-p" disabled={busy} onClick={() => send({ [q]: "Crear" })}>
              {t("Crear")}
            </button>
            <button className="btn" disabled={busy} onClick={() => send({ [q]: "Cambiar algo" })}>
              {t("Cambiar algo")}
            </button>
            <button className="btn btn-ghost" disabled={busy} onClick={() => send({ [q]: "Descartar" })}>
              {t("Descartar")}
            </button>
          </div>
        )}
        {error && <div className="error selectable">{error}</div>}
      </div>
    );
  }

  return (
    <div className="ask">
      {ask.questions.map((q) => (
        <fieldset key={q.question} className="ask-q" disabled={busy}>
          <legend>
            {q.header && <span className="tag">{q.header}</span>}
            <span className="selectable">{q.question}</span>
            {q.multiSelect && <span className="muted"> · {t("varias")}</span>}
          </legend>
          <div className="ask-options">
            {q.options.map((o) => {
              const on = (picked[q.question] ?? []).includes(o.label);
              return (
                <button key={o.label} type="button" className={`ask-opt ${on ? "on" : ""}`} onClick={() => choose(q, o.label)}>
                  <b>{o.label}</b>
                  {o.description && <span className="muted">{o.description}</span>}
                </button>
              );
            })}
          </div>
          {(() => {
            const shown = (picked[q.question] ?? [])[0] ?? q.options[0]?.label;
            const preview = q.options.find((o) => o.label === shown)?.preview;
            return (
              preview && (
                <div className="ask-preview">
                  <Markdown text={preview} remoteImages="click" />
                </div>
              )
            );
          })()}
          <input
            className="input"
            placeholder={t("O escribe tu propia respuesta…")}
            aria-label={t("Tu respuesta a: {question}", { question: q.question })}
            value={own[q.question] ?? ""}
            onChange={(e) => setOwn((o) => ({ ...o, [q.question]: e.target.value }))}
            onKeyDown={(e) => {
              if (e.key === "Enter" && instant && e.currentTarget.value.trim()) send({ [q.question]: e.currentTarget.value.trim() });
            }}
          />
        </fieldset>
      ))}
      <div className="ask-actions">
        {(!instant || Object.values(own).some((v) => v.trim())) && (
          <button
            className="btn btn-p"
            disabled={busy || !complete}
            onClick={() => send(Object.fromEntries(ask.questions.map((q) => [q.question, answerOf(q)])))}
          >
            {t("Responder")}
          </button>
        )}
        <button className="btn btn-ghost" disabled={busy} onClick={() => act(() => api.releaseAsk(ask.id))}>
          {t("Responder en la terminal")}
        </button>
      </div>
      {error && <div className="error selectable">{error}</div>}
    </div>
  );
}
