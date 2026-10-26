import { useState } from "react";
import { createPortal } from "react-dom";
import { api, type Project, type Task } from "../api";
import { t, tn } from "../i18n";
import { Icon } from "./Icon";
import { ProjectSettings } from "./ProjectSettings";
import { AddProjectDialog } from "./AddProjectDialog";
import { Button, ConfirmDialog, Dialog, IconButton, Menu } from "./ui";
import "./ProjectsView.css";

interface Props {
  projects: Project[];
  /** All tasks: they give each project's active count and whether it holds a workspace. */
  tasks?: Task[];
  onChanged: () => void;
  onOpenProject?: (name: string) => void;
  /** The add dialog, opened from outside too (the sidebar «+»). Uncontrolled when omitted. */
  adding?: boolean;
  onAddingChange?: (open: boolean) => void;
}

const isActive = (tk: Task) => tk.status.type !== "done" && tk.status.type !== "queued";
const holdsWorkspace = (tk: Task) => isActive(tk) && tk.branch != null;

function where(p: Project): string {
  if (p.settings.workspace === "checkout") return t("Tu checkout");
  if (p.settings.workspace === "worktree") return t("Aislado (worktree)");
  return t("Por defecto");
}

export function ProjectsView({ projects, tasks = [], onChanged, onOpenProject, adding, onAddingChange }: Props) {
  const [ownAdding, setOwnAdding] = useState(false);
  const addOpen = adding ?? ownAdding;
  const setAdd = (open: boolean) => (onAddingChange ? onAddingChange(open) : setOwnAdding(open));

  return (
    <>
      <div className="view-head" data-tauri-drag-region>
        <h1 data-tauri-drag-region>{t("Proyectos")}</h1>
        <span className="muted small">{projects.length}</span>
        <span className="pv-spacer" />
        <Button variant="primary" icon="plus" onClick={() => setAdd(true)}>{t("Añadir proyecto")}</Button>
      </div>
      <div className="view-body">
        {projects.length === 0 ? (
          <div className="pv-empty">
            <p>{t("Todavía no hay proyectos.")}</p>
            <Button variant="primary" icon="plus" onClick={() => setAdd(true)}>{t("Añadir proyecto")}</Button>
          </div>
        ) : (
          <div className="card">
            <table className="list pv-table">
              <thead>
                <tr><th>{t("Nombre")}</th><th>{t("Dónde trabaja")}</th><th>{t("Carpeta")}</th><th /></tr>
              </thead>
              <tbody>
                {projects.map((p) => (
                  <ProjectRow
                    key={p.name}
                    project={p}
                    tasks={tasks.filter((tk) => tk.project === p.name)}
                    onChanged={onChanged}
                    onOpen={onOpenProject}
                  />
                ))}
              </tbody>
            </table>
          </div>
        )}
      </div>
      {addOpen && (
        <AddProjectDialog
          onClose={() => setAdd(false)}
          onAdded={() => {
            setAdd(false);
            onChanged();
          }}
        />
      )}
    </>
  );
}

function ProjectRow({ project, tasks, onChanged, onOpen }: { project: Project; tasks: Task[]; onChanged: () => void; onOpen?: (name: string) => void }) {
  const [dialog, setDialog] = useState<"settings" | "rename" | "remove" | null>(null);
  const active = tasks.filter(isActive).length;
  const blocked = tasks.some(holdsWorkspace) ? t("Una tarea tiene su workspace en este proyecto: termínala o elimínala antes de quitarlo.") : undefined;
  const stop = (e: React.SyntheticEvent) => e.stopPropagation();

  return (
    <tr className="pv-row" onClick={() => onOpen?.(project.name)}>
      <td>
        <span className="pv-name">
          <Icon name={project.kind === "parent" ? "folder" : "repo"} />
          <span className="pv-names">
            <button type="button" className="pv-open" onClick={(e) => { stop(e); onOpen?.(project.name); }}>{project.name}</button>
            <span className="muted small">
              {project.kind === "parent" ? "Multi-repo" : "Repo"} · {tn(active, "{n} tarea activa", "{n} tareas activas")}
            </span>
          </span>
        </span>
      </td>
      <td>{where(project)}</td>
      <td className="mono muted selectable pv-path">{project.path}</td>
      <td className="pv-actions" onClick={stop}>
        <Button variant="ghost" size={28} onClick={() => setDialog("settings")}>{t("Ajustes")}</Button>
        <Menu
          trigger={(p) => <IconButton {...p} icon="more" label={t("Más acciones de {name}", { name: project.name })} />}
          items={[
            { label: t("Renombrar"), onSelect: () => setDialog("rename") },
            { label: t("Quitar…"), danger: true, onSelect: () => setDialog("remove") },
          ]}
        />
        {dialog !== null && createPortal(
          dialog === "settings" ? (
            <ProjectSettings project={project} onClose={() => setDialog(null)} onSaved={() => { setDialog(null); onChanged(); }} />
          ) : dialog === "rename" ? (
            <RenameDialog project={project} onClose={() => setDialog(null)} onDone={() => { setDialog(null); onChanged(); }} />
          ) : (
            <RemoveDialog project={project} blocked={blocked} onClose={() => setDialog(null)} onDone={() => { setDialog(null); onChanged(); }} />
          ),
          document.body,
        )}
      </td>
    </tr>
  );
}

function RenameDialog({ project, onClose, onDone }: { project: Project; onClose: () => void; onDone: () => void }) {
  const [name, setName] = useState(project.name);
  const [error, setError] = useState<string | null>(null);
  const save = async (e: React.FormEvent) => {
    e.preventDefault();
    setError(null);
    try {
      await api.updateProject(project.name, { name: name.trim() });
      onDone();
    } catch (err) {
      setError((err as Error).message);
    }
  };
  return (
    <Dialog
      title={t("Renombrar {name}", { name: project.name })}
      onClose={onClose}
      footer={
        <>
          <Button variant="secondary" onClick={onClose}>{t("Cancelar")}</Button>
          <Button variant="primary" type="submit" form="rename-project-form" disabled={!name.trim() || name.trim() === project.name}>{t("Guardar")}</Button>
        </>
      }
    >
      <form id="rename-project-form" onSubmit={save}>
        <label className="field">
          <span>{t("Nombre")}</span>
          <input className="input" value={name} onChange={(e) => setName(e.target.value)} required />
        </label>
        {error && <p className="apd-error selectable" role="alert">{error}</p>}
      </form>
    </Dialog>
  );
}

function RemoveDialog({ project, blocked, onClose, onDone }: { project: Project; blocked?: string; onClose: () => void; onDone: () => void }) {
  // The daemon refuses with 409 while a task has a workspace; show its reason like the local check.
  const [refused, setRefused] = useState<string | null>(null);
  return (
    <ConfirmDialog
      title={t("Quitar {name}", { name: project.name })}
      body={t("Sus sesiones se cierran; las tareas hechas se quedan sin proyecto. La carpeta no se toca.")}
      action={t("Quitar")}
      danger
      blocked={blocked ?? refused ?? undefined}
      onClose={onClose}
      onConfirm={() => api.deleteProject(project.name).then(onDone).catch((e) => setRefused((e as Error).message))}
    />
  );
}
