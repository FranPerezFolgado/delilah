import { useState } from "react";
import { api, type Project, type ProjectKind } from "../api";

export function ProjectsView({ projects, onChanged }: { projects: Project[]; onChanged: () => void }) {
  const [path, setPath] = useState("");
  const [kind, setKind] = useState<ProjectKind>("repo");
  const [name, setName] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const add = async (e: React.FormEvent) => {
    e.preventDefault();
    setBusy(true);
    setError(null);
    try {
      await api.addProject({ path: path.trim(), kind, name: name.trim() || undefined });
      setPath("");
      setName("");
      onChanged();
    } catch (err) {
      setError((err as Error).message);
    } finally {
      setBusy(false);
    }
  };

  return (
    <>
      <div className="view-head">
        <h1>Proyectos</h1>
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
            <input className="input mono" placeholder="/Users/tu/dev/projects/mi-proyecto" value={path} onChange={(e) => setPath(e.target.value)} required />
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
          {error && <div className="error selectable">{error}</div>}
          <div><button className="btn btn-p" disabled={busy}>Añadir</button></div>
        </form>
      </div>
    </>
  );
}
