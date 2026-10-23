import { useState } from "react";
import { api } from "../api";
import { t } from "../i18n";
import { Markdown } from "./Markdown";

interface Props {
  ask: { id: string; questions: { question: string }[]; proposal: { title: string; brief: string } };
  /** Opens the task the proposal created. */
  onOpenTask?: (key: string) => void;
  /** Called once the ask is resolved. */
  onDone?: () => void;
}

/** A task proposal from an idea conversation: title, brief and Crear / Cambiar algo /
 *  Descartar. */
export function ProposalCard({ ask, onDone, onOpenTask }: Props) {
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [created, setCreated] = useState<string | null>(null);
  // The daemon answered 200 with an error: the ask is resolved, so no more buttons.
  const [failed, setFailed] = useState(false);
  const q = ask.questions[0]?.question ?? "";

  const send = async (answer: string, launch?: "autopilot") => {
    setBusy(true);
    setError(null);
    try {
      const res = launch ? await api.answerAsk(ask.id, { [q]: answer }, launch) : await api.answerAsk(ask.id, { [q]: answer });
      if (res?.error) {
        setError(res.error);
        setFailed(true);
      }
      if (res?.task) setCreated(res.task);
      onDone?.();
    } catch (e) {
      setError((e as Error).message);
      setBusy(false);
    }
  };

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
        !failed && (
          <div className="ask-actions">
            <button className="btn btn-p" disabled={busy} onClick={() => send("Crear")}>
              {t("Crear")}
            </button>
            <button className="btn" disabled={busy} onClick={() => send("Crear", "autopilot")}>
              {t("Crear y lanzar en autopiloto")}
            </button>
            <button className="btn" disabled={busy} onClick={() => send("Cambiar algo")}>
              {t("Cambiar algo")}
            </button>
            <button className="btn btn-ghost" disabled={busy} onClick={() => send("Descartar")}>
              {t("Descartar")}
            </button>
          </div>
        )
      )}
      {error && <div className="error selectable">{error}</div>}
    </div>
  );
}
