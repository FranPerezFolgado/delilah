import { useState } from "react";
import { api, type FolderInspection, type Project, type ProjectKind } from "../api";
import { isDesktop, pickFolder } from "../platform";

export function ProjectsView({ projects, onChanged }: { projects: Project[]; onChanged: () => void }) {
  const [path, setPath] = useState("");
  const [kind, setKind] = useState<ProjectKind>("repo");
  const [name, setName] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [found, setFound] = useState<FolderInspection | null>(null);

  // Fill the rest of the form from what the folder is.
  const inspect = async (folder: string) => {
    setFound(null);
    if (!folder.trim()) return;
    // Show the chosen folder even if it turns out not to be usable.
    setPath(folder.trim());
    try {
      const info = await api.inspectFolder(folder.trim());
      setFound(info);
      setPath(info.path);
      setKind(info.kind);
      setName((n) => n || info.name);
      setError(null);
    } catch (e) {
      setError((e as Error).message);
    }
  };

  const choose = async () => {
    const folder = await pickFolder("Elige la carpeta del repositorio");
    if (folder) {
      setName("");
      await inspect(folder);
    }
  };

  const add = async (e: React.FormEvent) => {
    e.preventDefault();
    setBusy(true);
    setError(null);
    try {
      await api.addProject({ path: path.trim(), kind, name: name.trim() || undefined });
      setPath("");
      setName("");
      setFound(null);
      onChanged();
    } catch (err) {
      setError((err as Error).message);
    } finally {
      setBusy(false);
    }
  };

  return (
    <>
      <div className="view-head" data-tauri-drag-region>
        <h1 data-tauri-drag-region>Proyectos</h1>
        <span className="muted" style={{ fontSize: 12 }}>{projects.length}</span>
      </div>
      <div className="view-body">
        <div className="card">
          <table className="list">
            <thead>
              <tr><th>Nombre</th><th>Tipo</th><th>Workflow</th><th>Carpeta</th><th /></tr>
            </thead>
            <tbody>
              {projects.length === 0 && (
                <tr><td colSpan={5} className="muted">Todavía no hay proyectos. Añade uno abajo.</td></tr>
              )}
              {projects.map((p) => (
                <ProjectRow key={p.name} project={p} onChanged={onChanged} />
              ))}
            </tbody>
          </table>
        </div>

        <form className="card" style={{ padding: 14, display: "flex", flexDirection: "column", gap: 12 }} onSubmit={add}>
          <div className="sec">Añadir proyecto</div>
          <label className="field">
            <span>Carpeta del repo</span>
            <span style={{ display: "flex", gap: 8 }}>
              <input className="input mono" style={{ flex: 1 }} placeholder="/Users/tu/dev/projects/mi-proyecto" value={path}
                onChange={(e) => setPath(e.target.value)} onBlur={(e) => inspect(e.target.value)} required />
              {isDesktop && (
                <button type="button" className="btn" style={{ height: 30 }} onClick={choose}>Elegir carpeta…</button>
              )}
            </span>
            {found && (
              <span className="muted" style={{ fontSize: 12 }}>
                {found.submodules > 0
                  ? `Repo con ${found.submodules} submódulos: se añadirá como parent.`
                  : "Repo sin submódulos: se añadirá como repo."}
              </span>
            )}
          </label>
          <div className="field">
            <span>Tipo</span>
            <div className="choices" role="radiogroup">
              {(["repo", "parent"] as ProjectKind[]).map((k) => (
                <label key={k} className={`choice ${kind === k ? "on" : ""}`}>
                  <input type="radio" name="kind" checked={kind === k} onChange={() => setKind(k)} />
                  {k === "repo" ? "Repo (un repositorio)" : "Parent (con submódulos)"}
                </label>
              ))}
            </div>
          </div>
          <label className="field">
            <span>Nombre (opcional, por defecto el de la carpeta)</span>
            <input className="input" value={name} onChange={(e) => setName(e.target.value)} />
          </label>
          {error && (
            <div className="notice" style={{ borderColor: "var(--err)" }}>
              <span className="c-err">●</span>
              <span className="selectable">
                {error}
                {error.includes("not a git repo") && " — inicializa el repositorio (git init) y vuelve a elegirlo."}
              </span>
            </div>
          )}
          <div><button className="btn btn-p" disabled={busy}>Añadir</button></div>
        </form>
      </div>
    </>
  );
}

/** One project: shown, edited in place, or confirming its deletion. */
function ProjectRow({ project, onChanged }: { project: Project; onChanged: () => void }) {
  const [mode, setMode] = useState<"view" | "edit" | "delete">("view");
  const [name, setName] = useState(project.name);
  const [path, setPath] = useState(project.path);
  const [kind, setKind] = useState<ProjectKind>(project.kind);
  const [workflow, setWorkflow] = useState(project.workflow);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const act = async (action: () => Promise<unknown>) => {
    setBusy(true);
    setError(null);
    try {
      await action();
      setMode("view");
      onChanged();
    } catch (e) {
      setError((e as Error).message);
    } finally {
      setBusy(false);
    }
  };
  const reset = () => {
    setName(project.name);
    setPath(project.path);
    setKind(project.kind);
    setWorkflow(project.workflow);
    setError(null);
    setMode("view");
  };

  if (mode === "edit") {
    return (
      <tr>
        <td colSpan={5}>
          <form
            style={{ display: "grid", gridTemplateColumns: "160px 170px 120px minmax(0,1fr)", gap: 8, alignItems: "end" }}
            onSubmit={(e) => {
              e.preventDefault();
              act(() => api.updateProject(project.name, { name, path, kind, workflow }));
            }}
          >
            <label className="field"><span>Nombre</span><input className="input" value={name} onChange={(e) => setName(e.target.value)} required /></label>
            <label className="field">
              <span>Tipo</span>
              <select className="input" value={kind} onChange={(e) => setKind(e.target.value as ProjectKind)}>
                <option value="repo">Repo</option>
                <option value="parent">Parent con submódulos</option>
              </select>
            </label>
            <label className="field"><span>Workflow</span><input className="input mono" value={workflow} onChange={(e) => setWorkflow(e.target.value)} required /></label>
            <label className="field">
              <span>Carpeta</span>
              <span style={{ display: "flex", gap: 6 }}>
                <input className="input mono" style={{ flex: 1, minWidth: 0 }} value={path} onChange={(e) => setPath(e.target.value)} required />
                {isDesktop && (
                  <button type="button" className="btn" style={{ height: 30 }}
                    onClick={async () => { const f = await pickFolder("Elige la carpeta del repositorio"); if (f) setPath(f); }}>
                    Elegir…
                  </button>
                )}
              </span>
            </label>
            <div style={{ gridColumn: "1 / -1", display: "flex", gap: 8, alignItems: "center" }}>
              <button className="btn btn-p" disabled={busy}>Guardar</button>
              <button type="button" className="btn" onClick={reset}>Cancelar</button>
              {error && <span className="error selectable">{error}</span>}
            </div>
          </form>
        </td>
      </tr>
    );
  }

  return (
    <tr>
      <td style={{ fontWeight: 600 }}>{project.name}</td>
      <td>{project.kind === "parent" ? "Parent con submódulos" : "Repo"}</td>
      <td className="mono">{project.workflow}</td>
      <td className="mono muted trunc selectable" style={{ maxWidth: 380 }}>{project.path}</td>
      <td style={{ whiteSpace: "nowrap", textAlign: "right" }}>
        {mode === "delete" ? (
          <span style={{ display: "inline-flex", gap: 6, alignItems: "center" }}>
            <span className="muted" style={{ fontSize: 12 }}>Sus sesiones se cierran; las tareas hechas se quedan sin proyecto.</span>
            <button className="btn" style={{ color: "var(--err)" }} disabled={busy} onClick={() => act(() => api.deleteProject(project.name))}>
              Borrar
            </button>
            <button className="btn btn-ghost" onClick={reset}>Cancelar</button>
          </span>
        ) : (
          <span style={{ display: "inline-flex", gap: 4 }}>
            <button className="btn btn-ghost" onClick={() => setMode("edit")}>Editar</button>
            <button className="btn btn-ghost" onClick={() => setMode("delete")}>Borrar</button>
          </span>
        )}
        {error && <div className="error selectable" style={{ whiteSpace: "normal" }}>{error}</div>}
      </td>
    </tr>
  );
}
