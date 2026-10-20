import { useEffect, useState } from "react";
import { api, ApiError, isActive, type Task } from "../api";
import { t } from "../i18n";

interface Props {
  task: Task;
  onClose: () => void;
  onDeleted: (key: string) => void;
}

/** Confirms deleting a task for good. Unpushed work in its worktree needs a second,
 *  explicit "Eliminar igualmente"; until then nothing is stopped. */
export function DeleteTaskDialog({ task, onClose, onDeleted }: Props) {
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [unpushed, setUnpushed] = useState(false);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && !busy && onClose();
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose, busy]);

  const remove = async (force: boolean) => {
    setBusy(true);
    setError(null);
    try {
      await api.deleteTask(task.key, force);
      onDeleted(task.key);
    } catch (e) {
      setError((e as Error).message);
      setUnpushed(e instanceof ApiError && e.unpushed);
      setBusy(false);
    }
  };

  const backlog = task.status.type === "queued";
  return (
    <div className="backdrop" onMouseDown={(e) => e.target === e.currentTarget && !busy && onClose()}>
      <div className="dialog" role="alertdialog" aria-label={t("Eliminar tarea")}>
        <h2>{t("¿Eliminar {key}?", { key: task.key })}</h2>
        <p style={{ margin: 0 }}>{task.title}</p>
        <p className="muted" style={{ margin: 0, fontSize: 12.5 }}>
          {backlog
            ? t("Sale del backlog. No se puede deshacer.")
            : t("Se borra con su historial y sus sesiones, y se libera su workspace; la rama se queda. No se puede deshacer.")}
          {isActive(task.status) && <> {t("Se parará el agente.")}</>}
        </p>
        {error && <div className="error selectable">{error}</div>}
        <div style={{ display: "flex", gap: 8, justifyContent: "flex-end" }}>
          <button type="button" className="btn" onClick={onClose} disabled={busy}>{t("Cancelar")}</button>
          {unpushed ? (
            <button type="button" className="btn btn-danger" disabled={busy} onClick={() => remove(true)}>
              {t("Eliminar igualmente (se pierde lo no subido)")}
            </button>
          ) : (
            <button type="button" className="btn btn-danger" disabled={busy} onClick={() => remove(false)} autoFocus>
              {busy ? t("Eliminando…") : t("Eliminar")}
            </button>
          )}
        </div>
      </div>
    </div>
  );
}
