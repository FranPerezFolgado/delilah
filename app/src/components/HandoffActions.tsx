import { useState } from "react";
import { ago, api, type Autopilot, type Handoff } from "../api";
import { t } from "../i18n";
import { Icon } from "./Icon";

interface Props {
  task: string;
  handoff: Handoff | null;
  /** When paused, a banner with the reason and the ways forward is shown. */
  autopilot?: Autopilot | null;
  /** Called after a request went through. */
  onDone?: () => void;
  /** The error line is left out when the item already shows it. */
  hideError?: boolean;
}

/** What the user can do about a handoff: confirm or decline the offer, retry a failure,
 *  or see that the notes are being saved. */
export function HandoffActions({ task, handoff, autopilot, onDone, hideError }: Props) {
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
  const autopilotAct = async (action: "resume" | "off") => {
    setBusy(true);
    setError(null);
    try {
      await api.setAutopilot(task, action);
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
      {autopilot?.state === "paused" && (
        <div className="autopilot-banner">
          <strong>{t("Autopiloto en pausa")}</strong>
          {autopilot.reason && <span className="selectable">{autopilot.reason}</span>}
          {autopilot.since && <span className="muted">{t("en pausa desde {when}", { when: ago(autopilot.since) })}</span>}
          <button className="btn btn-p" disabled={busy} onClick={() => autopilotAct("resume")}>
            {t("Reanudar autopiloto")}
          </button>
          <button className="btn" disabled={busy} onClick={() => autopilotAct("off")}>
            {t("Seguir a mano")}
          </button>
        </div>
      )}
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
