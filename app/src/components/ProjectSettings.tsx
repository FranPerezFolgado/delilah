import { useEffect, useState } from "react";
import { api, ApiError, type Project, type ProjectSettings as Settings, type SettingsError, type SharedMode, type Submodule } from "../api";
import { t } from "../i18n";
import { WherePicker } from "./WherePicker";

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
  const dirty = JSON.stringify(s) !== JSON.stringify(saved);

  useEffect(() => {
    if (parent) api.submodules(project.name, { all: true }).then((l) => { setSubs(l); setSubsLoaded(true); }).catch(() => {});
  }, [parent, project.name]);

  const close = () => {
    if (busy) return;
    if (dirty) setConfirming(true);
    else onClose();
  };
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && close();
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  });

  const save = async (e: React.FormEvent) => {
    e.preventDefault();
    setBusy(true);
    setError(null);
    setErrors([]);
    try {
      onSaved(await api.updateProjectSettings(project.name, s));
    } catch (err) {
      if (err instanceof ApiError && err.fields.length > 0) setErrors(err.fields);
      else setError((err as Error).message);
      setBusy(false);
    }
  };

  const placed = (field: string) =>
    field === "workspace" || field === "branch" || field === "setup" || /^shared\[\d+\]/.test(field) ||
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
      <div className="backdrop">
        <div className="dialog" role="alertdialog" aria-label={t("¿Descartar los cambios?")}>
          <h2>{t("¿Descartar los cambios?")}</h2>
          <p className="muted" style={{ margin: 0 }}>{t("Hay cambios sin guardar en los ajustes.")}</p>
          <div style={{ display: "flex", gap: 8, justifyContent: "flex-end" }}>
            <button type="button" className="btn" onClick={() => setConfirming(false)}>{t("Seguir editando")}</button>
            <button type="button" className="btn btn-danger" onClick={onClose}>{t("Descartar")}</button>
          </div>
        </div>
      </div>
    );
  }

  return (
    <div className="backdrop" onMouseDown={(e) => e.target === e.currentTarget && close()}>
      <form className="dialog settings" onSubmit={save} aria-label={t("Ajustes de {name}", { name: project.name })}>
        <h2>{t("Ajustes de {name}", { name: project.name })}</h2>

        <div>
          <WherePicker value={s.workspace} onChange={(workspace) => setS({ ...s, workspace })} project={project} />
          <Err field="workspace" />
        </div>

        <label className="field">
          <span>{t("Plantilla de rama (opcional; usa {ticket})", { ticket: "{ticket}" })}</span>
          <input className="input mono" name="branch" value={s.branch ?? ""} placeholder={t("Delilah propone un nombre descriptivo (por defecto)")}
            onChange={(e) => setS({ ...s, branch: e.target.value.trim() === "" ? null : e.target.value })} />
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
                <button type="button" className="btn btn-ghost" onClick={() => setS({ ...s, shared: s.shared.filter((_, j) => j !== i) })}>
                  {t("Quitar")}
                </button>
              </div>
              <GroupErr prefix={`shared[${i}]`} />
            </div>
          ))}
          <div>
            <button type="button" className="btn" onClick={() => setS({ ...s, shared: [...s.shared, { path: "", mode: null }] })}>
              {t("Añadir entrada")}
            </button>
          </div>
        </div>

        <label className="field">
          <span>{t("Comando de preparación")}</span>
          <input className="input mono" name="setup" value={s.setup ?? ""}
            onChange={(e) => setS({ ...s, setup: e.target.value === "" ? null : e.target.value })} />
          <Err field="setup" />
        </label>

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
            <GroupErr prefix={`submodules.${list}`} />
          </div>
        ))}

        {(error || unplaced.length > 0) && (
          <div className="error selectable">
            {error}
            {unplaced.map((f, i) => <div key={i}>{f.field ? `${f.field}: ${f.message}` : f.message}</div>)}
          </div>
        )}
        <div style={{ display: "flex", gap: 8, justifyContent: "flex-end" }}>
          <button type="button" className="btn" onClick={close} disabled={busy}>{t("Cancelar")}</button>
          <button className="btn btn-p" disabled={busy}>{busy ? t("Guardando…") : t("Guardar")}</button>
        </div>
      </form>
    </div>
  );
}
