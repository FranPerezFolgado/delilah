import { t } from "../i18n";
import type { CiAutofixMode } from "../api";
import { Toggle } from "./ui";
import "./AutofixControl.css";

export interface AutofixControlProps {
  on: boolean;
  onChange: (on: boolean) => void;
  /** Finished tasks cannot change it. */
  disabled?: boolean;
  /** A request is in flight. */
  busy?: boolean;
}

/** Whether a red CI on the task's PRs is forwarded to the agent to fix. */
export function AutofixControl({ on, onChange, disabled, busy }: AutofixControlProps) {
  return (
    <div className="afc">
      <Toggle
        label={t("Arreglar CI")}
        pressed={on}
        disabled={disabled || busy}
        title={t("Si falla el CI de una PR, el agente recibe el fallo y lo arregla (con un límite de intentos)")}
        onChange={onChange}
      />
    </div>
  );
}

/** What a task gets when the user has not touched the option: the project's default, then autopilot. */
export function autofixDefault(mode: CiAutofixMode | undefined, autopilot: boolean): boolean {
  const m = mode ?? "autopilot";
  return m === "always" || (m === "autopilot" && autopilot);
}
