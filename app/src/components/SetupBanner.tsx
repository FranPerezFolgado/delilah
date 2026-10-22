import { useState } from "react";
import { api, type Task } from "../api";
import { t } from "../i18n";
import { Icon } from "./Icon";

interface Props {
  task: Task;
  /** A setup session is live: the workspace is being prepared. */
  running: boolean;
  onChanged: () => void;
}

/** The project setup of a task: cancel while it runs, retry or skip once it failed. */
export function SetupBanner({ task, running, onChanged }: Props) {
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const s = task.status;
  const failed = s.type === "setup-failed";
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

  return (
    <div className="notice err" style={{ flexDirection: "column", alignItems: "stretch" }}>
      <div style={{ display: "flex", alignItems: "center", gap: 12 }}>
        <Icon name={failed ? "failed" : "run"} className={failed ? "c-err" : "c-run"} />
        <span style={{ flex: 1 }}>
          {failed ? t("El setup del proyecto falló. Final de su salida:") : t("Preparando el workspace: el setup del proyecto está corriendo.")}
        </span>
        {failed ? (
          <>
            <button className="btn btn-p" disabled={busy} onClick={() => act(api.setupRetry)}>
              {t("Reintentar setup")}
            </button>
            <button className="btn" disabled={busy} onClick={() => act(api.setupSkip)}>
              {t("Lanzar sin setup")}
            </button>
          </>
        ) : (
          <button className="btn" disabled={busy} onClick={() => act(api.setupCancel)}>
            {t("Cancelar setup")}
          </button>
        )}
      </div>
      {s.type === "setup-failed" && s.reason && <pre className="setup-reason mono selectable">{s.reason}</pre>}
      {error && <div className="error selectable">{error}</div>}
    </div>
  );
}
