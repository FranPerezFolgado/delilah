import { useEffect, useState } from "react";
import { api, type Project, type Task } from "../api";
import { Icon } from "./Icon";
import { askToDelete } from "./TaskView";
import { t } from "../i18n";
import { WherePicker, whereOf, type Where } from "./WherePicker";

interface Props {
  task: Task;
  projects: Project[];
  /** Opens the start dialog for it ("Empezar"). */
  onStart: (task: Task) => void;
  onChanged: () => void;
}

/** A task saved to the backlog: what it is about, editable until it starts. */
export function BacklogTaskView({ task, projects, onStart, onChanged }: Props) {
  const [title, setTitle] = useState(task.title);
  const [notes, setNotes] = useState(task.notes ?? "");
  const [project, setProject] = useState(task.project ?? "");
  const [where, setWhere] = useState<Where>(whereOf(task.in_checkout));
  const [savingMode, setSavingMode] = useState(task.saving_mode);
  const [autopilot, setAutopilot] = useState(!!task.autopilot);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // Edited elsewhere (another window, the CLI): show what is saved now.
  useEffect(() => {
    setTitle(task.title);
    setNotes(task.notes ?? "");
    setProject(task.project ?? "");
    setWhere(whereOf(task.in_checkout));
    setSavingMode(task.saving_mode);
    setAutopilot(!!task.autopilot);
  }, [task.title, task.notes, task.project, task.in_checkout, task.saving_mode, task.autopilot]);

  const dirty =
    title.trim() !== task.title ||
    notes !== (task.notes ?? "") ||
    project !== (task.project ?? "") ||
    where !== whereOf(task.in_checkout) ||
    savingMode !== task.saving_mode ||
    autopilot !== !!task.autopilot;
  const inCheckout = where === "checkout";
  const save = async () => {
    setBusy(true);
    setError(null);
    try {
      await api.updateTask(task.key, { title: title.trim(), notes, project: project || undefined, in_checkout: inCheckout, saving_mode: savingMode || autopilot, autopilot });
      onChanged();
    } catch (e) {
      setError((e as Error).message);
    } finally {
      setBusy(false);
    }
  };

  return (
    <>
      <div className="view-head" data-tauri-drag-region>
        <h1 data-tauri-drag-region>
          <span className="mono muted">{task.key}</span> {task.title}
        </h1>
        <span className="muted" style={{ fontSize: 12 }}>
          <Icon name="backlog" className="c-mut" /> {t("En el backlog: sin workspace ni agente")}
        </span>
      </div>
      <div className="view-body backlog-task">
        <label className="field">
          <span>{t("Título")}</span>
          <input className="input" value={title} onChange={(e) => setTitle(e.target.value)} />
        </label>
        <label className="field">
          <span>{t("Proyecto")}</span>
          <select className="input" value={project} onChange={(e) => setProject(e.target.value)}>
            {!task.project && <option value="">{t("Sin proyecto")}</option>}
            {projects.map((p) => (
              <option key={p.name} value={p.name}>
                {p.name}
              </option>
            ))}
          </select>
        </label>
        <WherePicker value={where} onChange={setWhere} project={projects.find((p) => p.name === project)} />
        <label style={{ display: "flex", alignItems: "center", gap: 8, fontSize: 13 }} title={t("Cada paso (plan, implementación, revisión) va en una sesión nueva, con las notas del anterior")}>
          <input type="checkbox" checked={savingMode || autopilot} disabled={autopilot} onChange={(e) => setSavingMode(e.target.checked)} />
          <span>{t("Modo ahorro")}</span>
        </label>
        <label style={{ display: "flex", alignItems: "center", gap: 8, fontSize: 13 }} title={t("El modo ahorro queda activo mientras el autopiloto esté activado")}>
          <input
            type="checkbox"
            checked={autopilot}
            onChange={(e) => {
              if (e.target.checked) setSavingMode(true);
              setAutopilot(e.target.checked);
            }}
          />
          <span>{t("Autopiloto")}</span>
        </label>
        <label className="field">
          <span>{t("Notas (el agente las recibe al empezar)")}</span>
          <textarea className="input" rows={8} value={notes} onChange={(e) => setNotes(e.target.value)} />
        </label>
        {error && <div className="error selectable">{error}</div>}
        <div style={{ display: "flex", gap: 8 }}>
          <button className="btn btn-p" disabled={busy || !title.trim()} onClick={() => onStart({ ...task, title: title.trim(), notes, project: project || null, in_checkout: inCheckout, saving_mode: savingMode || autopilot, autopilot: autopilot ? (task.autopilot ?? { state: "running", since: "" }) : null })}>
            <Icon name="run" /> {t("Empezar")}
          </button>
          {dirty && (
            <button className="btn" disabled={busy || !title.trim()} onClick={save}>
              {t("Guardar")}
            </button>
          )}
          <button className="btn btn-ghost c-err" style={{ marginLeft: "auto" }} disabled={busy} onClick={() => askToDelete(task)}>
            <Icon name="trash" /> {t("Eliminar")}
          </button>
        </div>
      </div>
    </>
  );
}
