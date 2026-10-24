import { useCallback, useEffect, useState } from "react";
import { api, ApiError, type PreviewView, type TaskStatus } from "../api";
import { t } from "../i18n";
import { on } from "../live";
import { openExternal } from "../platform";

interface Props {
  taskKey: string;
  /** The task's project name; the control exists only when it has a preview command
   * (or a preview of the task is still up). */
  project: string | null;
  /** The task's status; a finished task (done or failed) cannot start a preview. */
  status?: TaskStatus;
}

/** Starts, shows and stops a task's preview (the project's preview command). */
export function PreviewControl({ taskKey, project, status }: Props) {
  const [hasCommand, setHasCommand] = useState(false);
  const [preview, setPreview] = useState<PreviewView | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(() => {
    if (!project) return setHasCommand(false);
    api.projects().then((ps) => setHasCommand(!!ps.find((p) => p.name === project)?.preview_command)).catch(() => {});
    api.getPreview(taskKey).then(setPreview).catch(() => {});
  }, [taskKey, project]);

  useEffect(() => {
    refresh();
    const off = [on("tasks", refresh), on("projects", refresh)];
    return () => off.forEach((f) => f());
  }, [refresh]);

  // One still up stays stoppable after its command is cleared.
  const active = preview?.state === "starting" || preview?.state === "running";
  if ((!hasCommand && !active) || status?.type === "done" || status?.type === "failed") return null;

  const start = async () => {
    setBusy(true);
    setError(null);
    try {
      setPreview(await api.startPreview(taskKey));
    } catch (e) {
      if (e instanceof ApiError && e.status === 409 && e.reason === "running") refresh();
      else if (e instanceof ApiError && e.status === 409 && e.reason === "no-workspace") setError(t("La tarea aún no tiene espacio de trabajo"));
      else setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  };
  const stop = async () => {
    setBusy(true);
    setError(null);
    try {
      await api.stopPreview(taskKey);
      setPreview(null);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <span className="preview-ctl">
      {!preview && (
        <button className="btn btn-ghost" disabled={busy} onClick={start}>{t("Lanzar preview")}</button>
      )}
      {preview?.state === "starting" && (
        <>
          <button className="btn btn-ghost" disabled>{t("Preview arrancando")}</button>
          <button className="btn btn-ghost" disabled={busy} onClick={stop}>{t("Parar preview")}</button>
        </>
      )}
      {preview?.state === "running" && (
        <>
          {preview.url ? (
            <a
              href={preview.url}
              title={t("Preview en marcha")}
              onClick={(e) => {
                e.preventDefault();
                openExternal(preview.url!);
              }}
            >
              {preview.url}
            </a>
          ) : (
            <span>{t("Preview en marcha")}</span>
          )}
          <button className="btn btn-ghost" disabled={busy} onClick={stop}>{t("Parar preview")}</button>
        </>
      )}
      {preview?.state === "failed" && (
        <>
          <span className="preview-err" title={preview.error}>{t("Preview fallida")}{preview.error ? `: ${preview.error}` : ""}</span>
          <button className="btn btn-ghost" disabled={busy} onClick={start}>{t("Reintentar")}</button>
        </>
      )}
      {error && <span className="preview-err" title={error}>{error}</span>}
    </span>
  );
}
