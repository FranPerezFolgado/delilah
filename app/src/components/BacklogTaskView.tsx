import { useEffect, useState } from "react";
import { api, ApiError, isCheckoutDirty, type Project, type Task } from "../api";
import { t } from "../i18n";
import { isPhased } from "../workflowPresets";
import { relativeTime } from "../time";
import { AutofixControl, autofixDefault } from "./AutofixControl";
import { AutopilotControls } from "./AutopilotControls";
import { StartPicker, startChoiceOf, startFields, type StartChoice } from "./WorkflowEditor";
import { WherePicker, whereOf, type Where } from "./WherePicker";
import { Button, ConfirmDialog, StatusBadge, useToast } from "./ui";
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

/** The PATCH fields for a start: `workflow` and the stored `start_message`. */
const startChange = (c: StartChoice): { workflow: string; start_message: string } => {
  const f = startFields(c, true);
  return { workflow: f.workflow!, start_message: f.prompt ?? "" };
};

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
  const [startChoice, setStart] = useState<StartChoice>(startChoiceOf(task));
  const [startErrors, setStartErrors] = useState<NonNullable<ApiError["fields"]>>([]);
  // null until the user touches it: then it follows the project default and autopilot.
  const [autofix, setAutofix] = useState<boolean | null>(task.ci_autofix ?? null);
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
    setStart(startChoiceOf(task));
    setAutofix(task.ci_autofix ?? null);
  }, [task.title, task.notes, task.project, task.in_checkout, savedProject?.settings.workspace, task.saving_mode, task.autopilot, task.workflow, task.start_message, task.ci_autofix]);

  const stored = startChoiceOf(task);
  const startDirty = startChoice.kind !== stored.kind || (startChoice.kind === "prompt" && startChoice.prompt.trim() !== stored.prompt.trim());
  const mode = projects.find((p) => p.name === project)?.settings.ci_autofix;
  const autofixOn = autofix ?? autofixDefault(mode, autopilot);
  const autofixSaved = task.ci_autofix ?? autofixDefault(savedProject?.settings.ci_autofix, !!task.autopilot);
  const dirty =
    title.trim() !== task.title ||
    notes !== (task.notes ?? "") ||
    project !== (task.project ?? "") ||
    where !== whereOf(task.in_checkout, savedProject) ||
    savingMode !== task.saving_mode ||
    autopilot !== !!task.autopilot ||
    startDirty ||
    autofixOn !== autofixSaved;
  const workflow = startChoice.kind;
  const blankStart = startChoice.kind === "prompt" && !startChoice.prompt.trim();
  // Saving mode and autopilot only exist in a phased workflow (of the selected project).
  const autopilotAllowed = isPhased(workflow, projects.find((p) => p.name === project)?.settings);
  const locked = busy || !!started;
  const open = (key: string) => (onOpen ?? onChanged)(key);

  const guard = async (run: () => Promise<void>) => {
    setBusy(true);
    setError(null);
    setStartErrors([]);
    setRefused(false);
    try {
      await run();
    } catch (e) {
      const message = (e as Error).message;
      if (e instanceof ApiError && e.fields.length > 0) setStartErrors(e.fields);
      if (isCheckoutDirty(e)) setRefused(true);
      else setError(message);
    } finally {
      setBusy(false);
    }
  };

  const save = () =>
    guard(async () => {
      await api.updateTask(task.key, { title: title.trim(), notes, project: project || undefined, in_checkout: where === "checkout", saving_mode: autopilotAllowed && (savingMode || autopilot), autopilot: autopilot && autopilotAllowed, ci_autofix: autofixOn, ...(startDirty ? startChange(startChoice) : {}) });
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
        saving_mode: autopilotAllowed && (savingMode || autopilot),
        ...(autopilotAllowed ? { autopilot } : {}),
        ci_autofix: autofixOn,
        notes,
        ...startFields(startChoice, true),
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
          <p className="muted small">{workflow === "none" ? t("Ahorro y autopiloto solo están disponibles con el flujo del proyecto, no sin mensaje de inicio.") : workflow === "prompt" ? t("Ahorro y autopiloto solo están disponibles con el flujo del proyecto, no con un prompt propio.") : t("Ahorro y autopiloto solo están disponibles con un flujo por fases (configúralo en los ajustes del proyecto).")}</p>
        )}
        <AutofixControl on={autofixOn} busy={locked} onChange={setAutofix} />
        <label className="field">
          <span>{t("Notas (el agente las recibe al empezar)")}</span>
          <textarea className="input" rows={8} value={notes} disabled={locked} onChange={(e) => setNotes(e.target.value)} />
        </label>
        <StartPicker project={project} settings={projects.find((p) => p.name === project)?.settings}
          ticket={task.key.startsWith("LIBRE-") ? "" : task.key} title={title.trim()}
          value={startChoice} errors={startErrors} disabled={locked} onChange={setStart} />
        {refused && (
          <div className="error selectable bt-refused" role="alert">
            <span>{t("Tu checkout tiene cambios sin guardar. Guárdalos o trabaja en un worktree.")}</span>
            <Button variant="secondary" size={28} disabled={busy || blankStart} onClick={() => start(false)}>{t("Iniciar en un worktree")}</Button>
          </div>
        )}
        {error && <div className="error selectable">{error}</div>}
        {started && <div className="bt-started" role="status">{t("{key} iniciada", { key: task.key })}</div>}
        <div className="bt-actions">
          <Button variant="primary" icon="run" disabled={locked || blankStart || !title.trim() || !project} onClick={() => start(where === "checkout")}>
            {t("Iniciar")}
          </Button>
          <Button variant="secondary" disabled={locked || blankStart || !dirty || !title.trim()} onClick={save}>
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
