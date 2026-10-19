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
              <tr><th>Nombre</th><th>Tipo</th><th>Workflow</th><th>Carpeta</th></tr>
            </thead>
            <tbody>
              {projects.length === 0 && (
                <tr><td colSpan={4} className="muted">Todavía no hay proyectos. Añade uno abajo.</td></tr>
              )}
              {projects.map((p) => (
                <tr key={p.name}>
                  <td style={{ fontWeight: 600 }}>{p.name}</td>
                  <td>{p.kind === "parent" ? "Parent con submódulos" : "Repo"}</td>
                  <td className="mono">{p.workflow}</td>
                  <td className="mono muted trunc selectable" style={{ maxWidth: 420 }}>{p.path}</td>
                </tr>
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
