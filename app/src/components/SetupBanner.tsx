import { useEffect, useState } from "react";
import { api, type SetupInfo, type Task } from "../api";
import { t } from "../i18n";
import { openExternal } from "../platform";
import { Icon } from "./Icon";
import { Button, pushToast } from "./ui";
import "./SetupBanner.css";

interface Props {
  task: Task;
  /** A setup session is live: the workspace is being prepared. */
  running: boolean;
  onChanged: () => void;
  /** Opens a terminal in the task's workspace; without it the button is left out. */
  onOpenTerminal?: () => void;
  /** Opens the project's settings, where the setup command lives. */
  onOpenSettings?: () => void;
}

/** The daemon stores "cancelled"/"interrupted" as stable tokens; anything else is raw output. */
function reasonText(reason: string): string {
  if (reason === "cancelled") return t("Setup cancelado");
  if (reason === "interrupted") return t("Setup interrumpido: el daemon se reinició");
  return reason;
}

function seconds(ms: number): string {
  return t("{s} s", { s: Math.max(1, Math.round(ms / 1000)) });
}

/** Exit code (or why there is none) and how long the run took. */
function outcome(info: SetupInfo): string {
  const how = info.timed_out
    ? t("Superó el tiempo límite")
    : info.cancelled
      ? t("Cancelado")
      : info.exit_code !== null
        ? t("Código de salida {code}", { code: info.exit_code })
        : t("Sin código de salida");
  return info.duration_ms !== null ? `${how} · ${seconds(info.duration_ms)}` : how;
}

/** The project setup of a task: cancel while it runs; once it failed, what happened and what to do. */
export function SetupBanner({ task, running, onChanged, onOpenTerminal, onOpenSettings }: Props) {
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [info, setInfo] = useState<SetupInfo | null>(null);
  const s = task.status;
  const failed = s.type === "setup-failed";

  // The last run's command, exit code and output tail (the status only keeps a short reason).
  useEffect(() => {
    if (!failed) return setInfo(null);
    let live = true;
    api
      .setupInfo(task.key)
      .then((i) => live && setInfo(i))
      .catch(() => live && setInfo(null));
    return () => {
      live = false;
    };
  }, [failed, task.key]);

  if (!failed && !running) return null;

  const act = async (action: (key: string) => Promise<unknown>) => {
    setBusy(true);
    setError(null);
    try {
      await action(task.key);
      onChanged();
    } catch (e) {
      setError((e as Error).message);
    } finally {
      setBusy(false);
    }
  };

  const copy = () => {
    if (!info) return;
    navigator.clipboard
      ?.writeText(info.command)
      .then(() => pushToast({ text: t("Comando copiado") }))
      .catch(() => pushToast({ text: t("No se pudo copiar el comando"), kind: "error" }));
  };

  const reason = s.type === "setup-failed" ? s.reason : null;
  const stable = reason === "cancelled" || reason === "interrupted";
  // The tail when the daemon has it; else whatever the status kept.
  const output = info ? info.tail : stable ? "" : (reason ?? "");

  return (
    <div className="notice err setup-panel">
      <div className="setup-top">
        <Icon name={failed ? "failed" : "run"} className={failed ? "c-err" : "c-run"} />
        <span className="setup-title">
          {failed ? t("El setup del proyecto falló") : t("Preparando el workspace: el setup del proyecto está corriendo.")}
        </span>
        {failed ? (
          <>
            <Button variant="primary" size={28} icon="retry" disabled={busy} onClick={() => act(api.setupRetry)}>
              {t("Reintentar setup")}
            </Button>
            <Button variant="secondary" size={28} disabled={busy} onClick={() => act(api.setupSkip)}>
              {t("Iniciar sin setup")}
            </Button>
          </>
        ) : (
          <Button variant="secondary" size={28} disabled={busy} onClick={() => act(api.setupCancel)}>
            {t("Cancelar setup")}
          </Button>
        )}
      </div>
      {failed && (
        <>
          {reason && stable && <div className="setup-meta">{reasonText(reason)}</div>}
          {info && (
            <div className="setup-meta muted">
              {outcome(info)}
              {onOpenSettings && (
                <>
                  {" · "}
                  <button type="button" className="setup-link" onClick={onOpenSettings}>
                    {t("Comando de setup del proyecto")}
                  </button>
                </>
              )}
            </div>
          )}
          {info && <code className="setup-command mono selectable">{info.command}</code>}
          {output ? (
            <pre className="setup-reason mono selectable">{output}</pre>
          ) : (
            info && !stable && <div className="muted setup-meta">{t("El setup no escribió nada en su salida.")}</div>
          )}
          <div className="setup-actions">
            {info && info.log_bytes > 0 && (
              <Button variant="ghost" size={28} icon="external" onClick={() => openExternal(api.setupLogUrl(task.key))}>
                {t("Abrir el log completo")}
              </Button>
            )}
            {info && (
              <Button variant="ghost" size={28} onClick={copy}>
                {t("Copiar comando")}
              </Button>
            )}
            {onOpenTerminal && (
              <Button variant="ghost" size={28} icon="console" onClick={onOpenTerminal}>
                {t("Abrir terminal aquí")}
              </Button>
            )}
          </div>
        </>
      )}
      {error && <div className="error selectable">{error}</div>}
    </div>
  );
}
