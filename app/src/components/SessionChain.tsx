import { useState } from "react";
import { api, type Session, type Step, type Task } from "../api";
import { chainOf, type ChainState } from "../chain";
import { t } from "../i18n";
import { HandoffActions } from "./HandoffActions";

const LABELS: Record<Step, string> = {
  plan: "Spec y plan",
  implement: "Implementación",
  review: "Revisión",
};
const MARKS: Record<ChainState, string> = { done: "✓", active: "●", pending: "○", waiting: "⏳" };
const STATES: Record<ChainState, string> = {
  done: "terminada",
  active: "en curso",
  pending: "pendiente",
  waiting: "esperando",
};

interface Props {
  task: Task;
  sessions: Session[];
  /** Shows a session's tab. */
  onOpenSession: (sessionId: string) => void;
  onChanged: () => void;
}

/** The steps of a saving-mode task, each in its own session: done, current, waiting for
 *  the handoff, or still to come. Renders nothing without saving mode. */
export function SessionChain({ task, sessions, onOpenSession, onChanged }: Props) {
  const [error, setError] = useState<string | null>(null);
  const chain = chainOf(task, sessions);
  if (!chain) return null;

  const reopen = async (s: Session) => {
    setError(null);
    try {
      await api.reopenSession(s.id);
      onChanged();
      onOpenSession(s.id);
    } catch (e) {
      setError((e as Error).message);
    }
  };

  return (
    <div className="chain" role="list" aria-label={t("Modo ahorro")}>
      {chain.map((c, i) => {
        const label = t(LABELS[c.step]);
        const s = c.session;
        return (
          <div key={c.step} className={`chain-step is-${c.state}`} role="listitem" aria-label={`${label}, ${t(STATES[c.state])}`}>
            {i > 0 && <span className="chain-arrow" aria-hidden="true">→</span>}
            <span className="chain-mark" aria-hidden="true">{MARKS[c.state]}</span>
            <span className="chain-name">{label}</span>
            {c.state === "active" && s && (
              <button className="btn btn-ghost" onClick={() => onOpenSession(s.id)}>
                {t("Abrir")}
              </button>
            )}
            {c.state === "done" && s && (
              <button className="btn btn-ghost" onClick={() => (s.live ? onOpenSession(s.id) : reopen(s))}>
                {s.live ? t("Abrir") : t("Reabrir")}
              </button>
            )}
            {c.state === "waiting" && (
              <HandoffActions task={task.key} handoff={task.handoff} onDone={onChanged} />
            )}
          </div>
        );
      })}
      {error && <div className="error selectable">{error}</div>}
    </div>
  );
}
