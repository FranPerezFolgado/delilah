import { useEffect, useState } from "react";
import { api, ApiError, type Project, type ProjectSettings as Settings, type SettingsError, type SharedMode, type Submodule } from "../api";
import { t } from "../i18n";
import { WorkflowEditor } from "./WorkflowEditor";
import { Button, ConfirmDialog, Dialog, OptionCard } from "./ui";
import "./ProjectSettings.css";

type Fields = SettingsError["fields"];

/** Settings of one project: a dialog that saves the whole set, or leaves after confirming a discard. */
export function ProjectSettings({ project, onClose, onSaved }: { project: Project; onClose: () => void; onSaved: (p: Project) => void }) {
  const [saved] = useState<Settings>(project.settings);
  const [s, setS] = useState<Settings>(project.settings);
  const [subs, setSubs] = useState<Submodule[]>([]);
  const [errors, setErrors] = useState<Fields>([]);
  const [error, setError] = useState<string | null>(null);
  const [subsLoaded, setSubsLoaded] = useState(false);
  const [busy, setBusy] = useState(false);
  const [confirming, setConfirming] = useState(false);

  const parent = project.kind === "parent";
  const [preview, setPreview] = useState(project.preview_command ?? "");
  const dirty = JSON.stringify(s) !== JSON.stringify(saved) || preview.trim() !== (project.preview_command ?? "");

  useEffect(() => {
    if (parent) api.submodules(project.name, { all: true }).then((l) => { setSubs(l); setSubsLoaded(true); }).catch(() => {});
  }, [parent, project.name]);

  const close = () => {
    if (busy) return;
    if (dirty) setConfirming(true);
    else onClose();
  };

  const save = async (e: React.FormEvent) => {
    e.preventDefault();
    setBusy(true);
    setError(null);
    setErrors([]);
    try {
      let updated = await api.updateProjectSettings(project.name, s);
      // The preview command lives on the project itself, not in the settings.
      if (preview.trim() !== (project.preview_command ?? "")) updated = await api.updateProject(project.name, { preview_command: preview.trim() || null });
      onSaved(updated);
    } catch (err) {
      if (err instanceof ApiError && err.fields.length > 0) setErrors(err.fields);
      else setError((err as Error).message);
      setBusy(false);
    }
  };

  const placed = (field: string) =>
    field === "workspace" || field === "branch" || field === "setup" || /^shared\[\d+\]/.test(field) || /^workflow(\.|\[|$)/.test(field) ||
    (parent && /^submodules\.(always|never)(\.|\[|$)/.test(field));
  const unplaced = errors.filter((f) => !placed(f.field));

  const msgs = (prefix: string) =>
    errors.filter((f) => f.field === prefix || f.field.startsWith(prefix + ".") || f.field.startsWith(prefix + "["));
  const Err = ({ field }: { field: string }) => {
    const list = errors.filter((f) => f.field === field);
    return list.length === 0 ? null : (
      <>{list.map((f, i) => <span key={i} className="error field-error selectable" data-field={f.field}>{f.message}</span>)}</>
    );
  };
  const GroupErr = ({ prefix }: { prefix: string }) => (
    <>{msgs(prefix).map((f, i) => <span key={i} className="error field-error selectable" data-field={f.field}>{f.message}</span>)}</>
  );

  const setShared = (i: number, change: Partial<Settings["shared"][number]>) =>
    setS({ ...s, shared: s.shared.map((e, j) => (j === i ? { ...e, ...change } : e)) });
  const toggle = (list: "always" | "never", name: string) => {
    const cur = s.submodules[list];
    const next = cur.includes(name) ? cur.filter((n) => n !== name) : [...cur, name];
    setS({ ...s, submodules: { ...s.submodules, [list]: next } });
  };

  if (confirming) {
    return (
      <ConfirmDialog
        danger
        title={t("¿Descartar los cambios?")}
        body={t("Hay cambios sin guardar en los ajustes.")}
        action={t("Descartar")}
        onClose={() => setConfirming(false)}
        onConfirm={onClose}
      />
    );
  }

  const inCheckout = s.workspace !== "worktree";
  const formId = "project-settings-form";
  const Help = ({ children }: { children: string }) => <span className="ps-help">{children}</span>;

  return (
    <Dialog
      title={t("Ajustes de {name}", { name: project.name })}
      size={{ w: 640, h: 720 }}
      onClose={close}
      footer={
        <>
          <Button variant="secondary" onClick={close} disabled={busy}>{t("Cancelar")}</Button>
          <Button variant="primary" type="submit" form={formId} disabled={busy}>{busy ? t("Guardando…") : t("Guardar")}</Button>
        </>
      }
    >
      <form id={formId} className="ps" onSubmit={save}>
        <div className="field">
          <span id="ps-where">{t("Dónde trabaja por defecto")}</span>
          <div className="ps-where" role="radiogroup" aria-labelledby="ps-where">
            <OptionCard
              name="where"
              checked={inCheckout}
              onClick={() => !inCheckout && setS({ ...s, workspace: null })}
              title={t("En tu checkout") + t(" · por defecto")}
              description={t("Crea la rama en {path} y trabaja ahí. Una tarea a la vez, con el checkout limpio.", { path: project.path })}
            />
            <OptionCard
              name="where"
              checked={!inCheckout}
              onClick={() => setS({ ...s, workspace: "worktree" })}
              title={t("Aislado (worktree)")}
              description={t("Copia aparte. Varias tareas en paralelo sin tocar tu carpeta.")}
            />
          </div>
          <Help>{t("Lo que se usa al empezar una tarea de este proyecto, salvo que elijas otra cosa en esa tarea.")}</Help>
          <Err field="workspace" />
        </div>

        <label className="field">
          <span>{t("Plantilla de rama")}</span>
          <input className="input mono" name="branch" value={s.branch ?? ""} placeholder={t("Delilah propone un nombre descriptivo (por defecto)")}
            onChange={(e) => setS({ ...s, branch: e.target.value.trim() === "" ? null : e.target.value })} />
          <Help>{t("Opcional. Usa {ticket} para el número del ticket; vacía, Delilah propone el nombre.", { ticket: "{ticket}" })}</Help>
          <Err field="branch" />
        </label>

        <div className="field">
          <span>{t("Archivos compartidos")}</span>
          {s.shared.map((e, i) => (
            <div key={i}>
              <div className="shared-row">
                <input className="input mono" name={`shared-path-${i}`} aria-label={t("Ruta dentro del proyecto")} value={e.path}
                  onChange={(ev) => setShared(i, { path: ev.target.value })} />
                <select className="input" aria-label={t("Modo")} value={e.mode ?? ""}
                  onChange={(ev) => setShared(i, { mode: (ev.target.value || null) as SharedMode | null })}>
                  <option value="">{t("Automático")}</option>
                  <option value="copy">{t("Copiar")}</option>
                  <option value="link">{t("Enlazar")}</option>
                </select>
                <Button variant="ghost" onClick={() => setS({ ...s, shared: s.shared.filter((_, j) => j !== i) })}>{t("Quitar")}</Button>
              </div>
              <GroupErr prefix={`shared[${i}]`} />
            </div>
          ))}
          <div>
            <Button variant="secondary" onClick={() => setS({ ...s, shared: [...s.shared, { path: "", mode: null }] })}>{t("Añadir entrada")}</Button>
          </div>
          <Help>{t("Archivos que no están en git (.env, credenciales) y que cada worktree necesita: se copian o se enlazan.")}</Help>
        </div>

        <label className="field">
          <span>{t("Comando de setup")}</span>
          <input className="input mono" name="setup" value={s.setup ?? ""}
            onChange={(e) => setS({ ...s, setup: e.target.value === "" ? null : e.target.value })} />
          <Help>{t("Se ejecuta al preparar el workspace de una tarea, antes de que el agente empiece (por ejemplo npm ci).")}</Help>
          <Err field="setup" />
        </label>

        <label className="field">
          <span>{t("Comando de preview")}</span>
          <input className="input mono" name="preview" value={preview} onChange={(e) => setPreview(e.target.value)} />
          <Help>{t("Delilah lo ejecuta en el workspace de una tarea para que puedas probarla. Déjalo vacío si el proyecto no tiene preview.")}</Help>
        </label>

        <div className="field">
          <span>{t("Flujo de trabajo")}</span>
          <WorkflowEditor project={project.name} value={s.workflow} errors={errors} onChange={(workflow) => setS({ ...s, workflow })} />
          <Help>{t("Qué envía Delilah al empezar una tarea de este proyecto.")}</Help>
        </div>

        {parent && (["always", "never"] as const).map((list) => (
          <div className="field" key={list}>
            <span>{list === "always" ? t("Submódulos siempre incluidos") : t("Submódulos nunca incluidos")}</span>
            <div className="picks">
              {subs.map((m) => (
                <label key={m.path} className="muted">
                  <input type="checkbox" checked={s.submodules[list].includes(m.path)} onChange={() => toggle(list, m.path)} /> {m.name}
                </label>
              ))}
              {subsLoaded && s.submodules[list].filter((p) => !subs.some((m) => m.path === p)).map((p) => (
                <label key={p} className="muted" data-stale={p}>
                  <input type="checkbox" checked onChange={() => toggle(list, p)} /> {p} ({t("ya no existe en el proyecto")})
                </label>
              ))}
            </div>
            <Help>{list === "always" ? t("Entran en todo workspace de este proyecto.") : t("Nunca se clonan ni se incluyen en un workspace.")}</Help>
            <GroupErr prefix={`submodules.${list}`} />
          </div>
        ))}

        {(error || unplaced.length > 0) && (
          <div className="error selectable">
            {error}
            {unplaced.map((f, i) => <div key={i}>{f.field ? `${f.field}: ${f.message}` : f.message}</div>)}
          </div>
        )}
      </form>
    </Dialog>
  );
}
