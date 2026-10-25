import { useState } from "react";
import { t } from "../i18n";
import { api, type Session } from "../api";
import { ConfirmDialog } from "./ui/ConfirmDialog";
import { Segmented } from "./ui/Segmented";
import "./SessionViewSwitch.css";

export type SessionView = "chat" | "terminal";

export interface SessionViewSwitchProps {
  session: Pick<Session, "id" | "live" | "runtime" | "effective_runtime">;
  /** What is shown for a PTY session. */
  view: SessionView;
  /** A PTY session switches between its chat and terminal views without restarting. */
  chooseView: (view: SessionView) => void;
  /** A turn is running: switching would cut it. */
  busy?: boolean;
  /** After the runtime changed (the caller refreshes its sessions). */
  onChanged?: () => void;
  onError?: (message: string) => void;
}

/** Chat | Terminal for the open session. Changing the runtime asks first. */
export function SessionViewSwitch({ session, view, chooseView, busy, onChanged, onError }: SessionViewSwitchProps) {
  const [target, setTarget] = useState<SessionView | null>(null);
  const running = session.runtime ?? "pty";
  // The next launch decides what a stopped session will be.
  const runtime = session.live ? running : (session.effective_runtime ?? running);
  const value: SessionView = runtime === "stream" ? "chat" : view;
  const reason = busy ? t("Espera a que termine el turno") : undefined;

  const pick = (v: string) => {
    const next = v as SessionView;
    if (next === value) return;
    if (runtime === "stream" && next === "terminal") setTarget("terminal");
    else chooseView(next);
  };

  const confirm = () => {
    const mode = target;
    setTarget(null);
    if (!mode) return;
    api.setSessionRuntime(session.id, mode).then(
      () => onChanged?.(),
      (e) => onError?.(t("No se pudo cambiar: {error}", { error: String(e?.message ?? e) })),
    );
  };

  return (
    <div className="svs">
      <Segmented
        mode="pressed"
        label={t("Vista de la sesión")}
        value={value}
        onChange={pick}
        options={[
          { value: "chat", label: t("Chat"), tag: "beta", disabled: busy, title: reason },
          { value: "terminal", label: t("Terminal"), disabled: busy, title: reason },
        ]}
      />
      {target && (
        <ConfirmDialog
          title={t("Pasar a terminal")}
          body={t("La sesión se reanuda en la terminal, sin modo chat.")}
          action={t("Cambiar")}
          onClose={() => setTarget(null)}
          onConfirm={confirm}
        />
      )}
    </div>
  );
}
