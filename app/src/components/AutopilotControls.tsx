import { useState } from "react";
import { t } from "../i18n";
import type { Autopilot } from "../api";
import { Button, ConfirmDialog, Toggle } from "./ui";
import "./AutopilotControls.css";

export interface AutopilotControlsProps {
  autopilot?: Autopilot | null;
  savingMode: boolean;
  onAutopilot: (action: "on" | "off" | "resume") => void;
  onSavingMode: (on: boolean) => void;
  /** Disables both while a request is in flight. */
  busy?: boolean;
  /** Compact header: only the switches. */
  iconOnly?: boolean;
}

/** Autopiloto and Ahorro toggles. Turning autopilot on asks first and keeps Ahorro on (disabled). */
export function AutopilotControls({ autopilot, savingMode, onAutopilot, onSavingMode, busy, iconOnly }: AutopilotControlsProps) {
  const [asking, setAsking] = useState(false);
  const active = autopilot?.state === "running" || autopilot?.state === "paused";
  const paused = autopilot?.state === "paused";
  return (
    <div className="apc">
      <Toggle
        label={t("Autopiloto")}
        pressed={active}
        disabled={busy}
        iconOnly={iconOnly}
        onChange={(on) => (on ? setAsking(true) : onAutopilot("off"))}
      />
      <Toggle
        label={t("Ahorro")}
        pressed={savingMode || active}
        disabled={busy || active}
        iconOnly={iconOnly}
        title={active ? t("Ahorro queda activado mientras el autopiloto está encendido") : t("Una sesión de Claude nueva por fase, para gastar menos contexto")}
        onChange={onSavingMode}
      />
      {paused && (
        <span className="apc-paused">
          <Button variant="secondary" size={28} icon="pause" disabled={busy} title={autopilot?.reason} onClick={() => onAutopilot("resume")}>
            {iconOnly ? t("Reanudar") : t("Autopiloto en pausa · Reanudar")}
          </Button>
          {autopilot?.reason && !iconOnly && <span className="apc-reason" title={autopilot.reason}>{autopilot.reason}</span>}
        </span>
      )}
      {asking && (
        <ConfirmDialog
          title={t("Activar autopiloto")}
          body={t("El autopiloto trabaja solo hasta abrir las PR y gasta cuota. Ahorro se queda activado.")}
          action={t("Activar")}
          onClose={() => setAsking(false)}
          onConfirm={() => {
            setAsking(false);
            onAutopilot("on");
          }}
        />
      )}
    </div>
  );
}
