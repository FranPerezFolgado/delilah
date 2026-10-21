import { useState } from "react";
import { api, type Handoff } from "../api";
import { t } from "../i18n";
import { Icon } from "./Icon";

interface Props {
  task: string;
  handoff: Handoff | null;
  /** Called after a request went through. */
  onDone?: () => void;
  /** The error line is left out when the item already shows it. */
  hideError?: boolean;
}

/** What the user can do about a handoff: confirm or decline the offer, retry a failure,
 *  or see that the notes are being saved. */
export function HandoffActions({ task, handoff, onDone, hideError }: Props) {
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const act = async (action: "confirm" | "decline" | "retry") => {
    setBusy(true);
    setError(null);
    try {
      await api.handoff(task, action);
      onDone?.();
    } catch (e) {
      setError((e as Error).message);
    } finally {
      setBusy(false);
    }
  };
  const stage = handoff?.stage;
  return (
    <>
      {stage === "saving" && <span className="muted">{t("Guardando las notas de la sesión…")}</span>}
      {stage === "starting" && <span className="muted">{t("Empezando la sesión nueva…")}</span>}
      {stage === "offered" && (
        <>
          <button className="btn btn-p" disabled={busy} onClick={() => act("confirm")}>
            <Icon name="run" /> {t("Implementar en sesión nueva")}
          </button>
          <button className="btn" disabled={busy} onClick={() => act("decline")}>
            {t("Seguir aquí")}
          </button>
        </>
      )}
      {stage === "failed" && (
        <button className="btn btn-p" disabled={busy} onClick={() => act("retry")}>
          {t("Reintentar")}
        </button>
      )}
      {stage === "failed" && handoff?.error && !hideError && <div className="error selectable">{handoff.error}</div>}
      {error && <div className="error selectable">{error}</div>}
    </>
  );
}
