import { useEffect, useState } from "react";
import { api, isCheckoutDirty, type Project, type Task } from "../api";
import { t } from "../i18n";
import { relativeTime } from "../time";
import { AutopilotControls } from "./AutopilotControls";
import { WherePicker, whereOf, type Where } from "./WherePicker";
import { Button, ConfirmDialog, StatusBadge, Toggle, useToast } from "./ui";
import "./BacklogTaskView.css";

interface Props {
  task: Task;
  projects: Project[];
  /** Kept for callers that still pass it; «Iniciar» starts the task here. */
  onStart?: (task: Task) => void;
  /** After a save or a delete (the caller refreshes). */
  onChanged: () => void;
  /** Opens the started task («Abrir» in the toast); falls back to `onChanged`. */
  onOpen?: (key: string) => void;
}

/** A task saved to the backlog: what it is about, editable until it starts. */
export function BacklogTaskView({ task, projects, onChanged, onOpen }: Props) {
  const toast = useToast();
  const savedProject = projects.find((p) => p.name === task.project);
  const [title, setTitle] = useState(task.title);
  const [notes, setNotes] = useState(task.notes ?? "");
  const [project, setProject] = useState(task.project ?? "");
  const [where, setWhere] = useState<Where>(whereOf(task.in_checkout, savedProject));
  const [savingMode, setSavingMode] = useState(task.saving_mode);
  const [autopilot, setAutopilot] = useState(!!task.autopilot);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [refused, setRefused] = useState(false);
  const [deleting, setDeleting] = useState(false);
  /** The task as the daemon returned it after «Iniciar». */
  const [started, setStarted] = useState<Task | null>(null);

  // Edited elsewhere (another window, the CLI): show what is saved now.
  useEffect(() => {
    if (started) return;
    setTitle(task.title);
    setNotes(task.notes ?? "");
    setProject(task.project ?? "");
    setWhere(whereOf(task.in_checkout, savedProject));
    setSavingMode(task.saving_mode);
    setAutopilot(!!task.autopilot);
  }, [task.title, task.notes, task.project, task.in_checkout, savedProject?.settings.workspace, task.saving_mode, task.autopilot]);

  const dirty =
    title.trim() !== task.title ||
    notes !== (task.notes ?? "") ||
    project !== (task.project ?? "") ||
    where !== whereOf(task.in_checkout, savedProject) ||
    savingMode !== task.saving_mode ||
    autopilot !== !!task.autopilot;
  // Only the spec workflow has autopilot.
  const autopilotAllowed = task.workflow === "feature";
  const locked = busy || !!started;
  const open = (key: string) => (onOpen ?? onChanged)(key);

  const guard = async (run: () => Promise<void>) => {
    setBusy(true);
    setError(null);
    setRefused(false);
    try {
      await run();
    } catch (e) {
      const message = (e as Error).message;
      if (isCheckoutDirty(e)) setRefused(true);
      else setError(message);
    } finally {
      setBusy(false);
    }
  };

  const save = () =>
    guard(async () => {
      await api.updateTask(task.key, { title: title.trim(), notes, project: project || undefined, in_checkout: where === "checkout", saving_mode: savingMode || autopilot, autopilot: autopilot && autopilotAllowed });
      onChanged();
    });

  const start = (inCheckout: boolean) =>
    guard(async () => {
      const run = await api.run({
        key: task.key,
        title: title.trim(),
        project,
        submodules: [],
        in_checkout: inCheckout,
        saving_mode: savingMode || autopilot,
        ...(autopilotAllowed ? { autopilot } : {}),
        notes,
      });
      setStarted(run);
      toast({ text: t("{key} iniciada", { key: task.key }), action: { label: t("Abrir"), run: () => open(task.key) } });
    });

  const remove = () =>
    guard(async () => {
      await api.deleteTask(task.key);
      setDeleting(false);
      onChanged();
    });

  return (
    <>
      <div className="bt-head" data-tauri-drag-region>
        <span className="bt-key mono">{task.key}</span>
        <h1 className="bt-title" data-tauri-drag-region>{started?.title ?? task.title}</h1>
        <StatusBadge status={started ? started.status : "backlog"} />
        {task.created_at && <span className="bt-when">{t("Creada {when}", { when: relativeTime(task.created_at) })}</span>}
      </div>
      <div className="view-body bt-body">
        <label className="field">
          <span>{t("Título")}</span>
          <input className="input" value={title} disabled={locked} onChange={(e) => setTitle(e.target.value)} />
        </label>
        <label className="field">
          <span>{t("Proyecto")}</span>
          <select className="input" value={project} disabled={locked} onChange={(e) => setProject(e.target.value)}>
            {!task.project && <option value="">{t("Sin proyecto")}</option>}
            {projects.map((p) => (
              <option key={p.name} value={p.name}>
                {p.name}
              </option>
            ))}
          </select>
        </label>
        <WherePicker value={where} onChange={(w) => setWhere(w ?? "checkout")} project={projects.find((p) => p.name === project)} />
        {autopilotAllowed ? (
          <AutopilotControls
            autopilot={autopilot ? (task.autopilot ?? { state: "running", since: "" }) : null}
            savingMode={savingMode}
            busy={locked}
            onAutopilot={(a) => {
              if (a === "on") setSavingMode(true);
              setAutopilot(a === "on");
            }}
            onSavingMode={setSavingMode}
          />
        ) : (
          <div className="apc">
            <Toggle label={t("Ahorro")} pressed={savingMode} disabled={locked} onChange={setSavingMode} title={t("Ahorro: una sesión nueva por fase, para releer menos contexto")} />
          </div>
        )}
        <label className="field">
          <span>{t("Notas (el agente las recibe al empezar)")}</span>
          <textarea className="input" rows={8} value={notes} disabled={locked} onChange={(e) => setNotes(e.target.value)} />
        </label>
        {refused && (
          <div className="error selectable bt-refused" role="alert">
            <span>{t("Tu checkout tiene cambios sin guardar. Guárdalos o trabaja en un worktree.")}</span>
            <Button variant="secondary" size={28} disabled={busy} onClick={() => start(false)}>{t("Iniciar en un worktree")}</Button>
          </div>
        )}
        {error && <div className="error selectable">{error}</div>}
        {started && <div className="bt-started" role="status">{t("{key} iniciada", { key: task.key })}</div>}
        <div className="bt-actions">
          <Button variant="primary" icon="run" disabled={locked || !title.trim() || !project} onClick={() => start(where === "checkout")}>
            {t("Iniciar")}
          </Button>
          <Button variant="secondary" disabled={locked || !dirty || !title.trim()} onClick={save}>
            {t("Guardar")}
          </Button>
          <Button variant="danger" className="bt-delete" disabled={locked} onClick={() => setDeleting(true)}>
            {t("Eliminar…")}
          </Button>
        </div>
      </div>
      {deleting && (
        <ConfirmDialog
          danger
          title={t("¿Eliminar {key}?", { key: task.key })}
          body={t("Sale del backlog. No se puede deshacer.")}
          action={t("Eliminar")}
          onClose={() => setDeleting(false)}
          onConfirm={remove}
        />
      )}
    </>
  );
}
