import { useState } from "react";
import { api, type PermissionAsk, type PermissionDecision } from "../api";
import { t } from "../i18n";

interface Props {
  sessionId: string;
  ask: PermissionAsk;
  /** Called once the daemon accepted the answer. */
  onDone?: () => void;
}

/** A permission the agent asks for before using a tool. "Always" only exists when the
 *  daemon has an exact rule to remember, and the rule is shown next to it. */
export function PermissionCard({ sessionId, ask, onDone }: Props) {
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [reason, setReason] = useState("");

  const answer = async (decision: PermissionDecision) => {
    setBusy(true);
    setError(null);
    try {
      await api.answerPermission(sessionId, ask.id, decision, decision === "deny" ? reason : undefined);
      onDone?.();
    } catch (e) {
      setError((e as Error).message);
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="ask perm">
      <div className="perm-what selectable">
        <span className="tag">{ask.tool}</span>
        <pre className="perm-summary">{ask.summary}</pre>
      </div>
      <div className="perm-actions">
        <button type="button" className="btn" disabled={busy} onClick={() => answer("allow_once")}>
          {t("Permitir una vez")}
        </button>
        {ask.rule && (
          <>
            <button type="button" className="btn" disabled={busy} onClick={() => answer("allow_always")}>
              {t("Permitir siempre en esta tarea")}
            </button>
            <code className="perm-rule selectable">{ask.rule}</code>
          </>
        )}
      </div>
      <div className="perm-actions">
        <input
          type="text"
          className="perm-reason"
          value={reason}
          disabled={busy}
          placeholder={t("Motivo (opcional)")}
          aria-label={t("Motivo (opcional)")}
          onChange={(e) => setReason(e.target.value)}
        />
        <button type="button" className="btn" disabled={busy} onClick={() => answer("deny")}>
          {t("Denegar")}
        </button>
      </div>
      {error && <div className="error">{error}</div>}
    </div>
  );
}
