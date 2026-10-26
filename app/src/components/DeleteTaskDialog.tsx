import { useRef, useState } from "react";
import { api, ApiError, isActive, type Task } from "../api";
import { t } from "../i18n";
import { ConfirmDialog } from "./ui/ConfirmDialog";

interface Props {
  task: Task;
  onClose: () => void;
  onDeleted: (key: string) => void;
}

/** Confirms deleting a task for good, on the kit's ConfirmDialog. Unpushed work in its
 *  worktree needs a second, explicit «Eliminar igualmente»; until then nothing is stopped. */
export function DeleteTaskDialog({ task, onClose, onDeleted }: Props) {
  const [error, setError] = useState<string | null>(null);
  const [unpushed, setUnpushed] = useState(false);
  const busy = useRef(false);

  const remove = async (force: boolean) => {
    if (busy.current) return;
    busy.current = true;
    setError(null);
    try {
      await api.deleteTask(task.key, force);
      onDeleted(task.key);
    } catch (e) {
      setError((e as Error).message);
      setUnpushed(e instanceof ApiError && e.unpushed);
      busy.current = false;
    }
  };

  const consequences =
    task.status.type === "queued"
      ? t("Sale del backlog. No se puede deshacer.")
      : t("Se borra con su historial y sus sesiones, y se libera su workspace; la rama se queda. No se puede deshacer.");
  const body = [
    task.title,
    consequences + (isActive(task.status) ? ` ${t("Se parará el agente.")}` : ""),
    error,
    unpushed ? t("Hay trabajo sin subir en su workspace: se perdería.") : null,
  ]
    .filter(Boolean)
    .join(" ");

  return (
    <ConfirmDialog
      title={t("¿Eliminar {key}?", { key: task.key })}
      body={body}
      action={unpushed ? t("Eliminar igualmente") : t("Eliminar tarea")}
      danger
      onClose={onClose}
      onConfirm={() => remove(unpushed)}
    />
  );
}
