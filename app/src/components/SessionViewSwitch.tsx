import { useState } from "react";
import { t } from "../i18n";
import { api, ApiError, type Session } from "../api";
import { ConfirmDialog, Segmented } from "./ui";
import "./SessionViewSwitch.css";

export type SessionView = "chat" | "terminal";

export interface SessionViewSwitchProps {
  session: Pick<Session, "id" | "live" | "runtime" | "effective_runtime">;
  /** @deprecated The view follows the runtime: both directions change it. */
  view?: SessionView;
  chooseView?: (view: SessionView) => void;
  /** A turn is running: switching would cut it. */
  busy?: boolean;
  /** After the runtime changed (the caller refreshes its sessions). */
  onChanged?: () => void;
  onError?: (message: string) => void;
}

/** A failed action as human Spanish text: the daemon's English messages never reach the screen. */
export function humanError(e: unknown): string {
  const m = e instanceof ApiError ? e.message.toLowerCase() : "";
  if (m.includes("wait for the turn")) return t("Espera a que termine el turno");
  if (m.includes("segundo plano") || m.includes("background work")) return t("Hay trabajo en segundo plano: espera a que termine o pulsa Detener");
  if (m.includes("switch runtime")) return t("Solo las sesiones de Claude de una tarea cambian de modo.");
  if (m.includes("already runs in")) return t("La sesión ya está en ese modo.");
  if (m.includes("does not exist") || m.includes("not found")) return t("Ya no existe.");
  return t("No se pudo completar la acción");
}

/** Chat | Terminal for the open session. Changing the runtime asks first. */
export function SessionViewSwitch({ session, busy, onChanged, onError }: SessionViewSwitchProps) {
  const [target, setTarget] = useState<SessionView | null>(null);
  const running = session.runtime ?? "pty";
  // The next launch decides what a stopped session will be.
  const runtime = session.live ? running : (session.effective_runtime ?? running);
  const value: SessionView = runtime === "stream" ? "chat" : "terminal";
  const reason = busy ? t("Espera a que termine el turno") : undefined;

  const pick = (v: string) => {
    const next = v as SessionView;
    if (next === value) return;
    setTarget(next);
  };

  const confirm = () => {
    const mode = target;
    setTarget(null);
    if (!mode) return;
    api.setSessionRuntime(session.id, mode).then(
      () => onChanged?.(),
      (e) => onError?.(t("No se pudo cambiar: {error}", { error: humanError(e) })),
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
          title={target === "chat" ? t("Pasar a chat") : t("Pasar a terminal")}
          body={target === "chat" ? t("La sesión se reanuda en modo chat, sin terminal.") : t("La sesión se reanuda en la terminal, sin modo chat.")}
          action={t("Cambiar")}
          onClose={() => setTarget(null)}
          onConfirm={confirm}
        />
      )}
    </div>
  );
}
