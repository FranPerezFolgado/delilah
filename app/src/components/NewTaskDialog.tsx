import { useEffect, useState } from "react";
import { api, type Project, type Submodule, type Task } from "../api";

interface Props {
  projects: Project[];
  onClose: () => void;
  onStarted: (task: Task) => void;
}

export function NewTaskDialog({ projects, onClose, onStarted }: Props) {
  const [key, setKey] = useState("");
  const [title, setTitle] = useState("");
  const [projectName, setProjectName] = useState(projects[0]?.name ?? "");
  const [submodules, setSubmodules] = useState<Submodule[]>([]);
  const [picked, setPicked] = useState<string[]>([]);
  const [prompt, setPrompt] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const project = projects.find((p) => p.name === projectName);

  useEffect(() => {
    setPicked([]);
    setSubmodules([]);
    if (project?.kind === "parent") api.submodules(project.name).then(setSubmodules).catch((e) => setError(e.message));
  }, [project?.name, project?.kind]);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && onClose();
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose]);

  const toggle = (path: string) => setPicked((p) => (p.includes(path) ? p.filter((x) => x !== path) : [...p, path]));

  const submit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!project) return;
    setBusy(true);
    setError(null);
    try {
      const task = await api.run({
        key: key.trim(),
        title: title.trim(),
        project: project.name,
        submodules: picked,
        // A custom prompt runs outside the /feature workflow.
        ...(prompt.trim() ? { workflow: "prompt", prompt: prompt.trim() } : {}),
      });
      onStarted(task);
    } catch (err) {
      setError((err as Error).message);
      setBusy(false);
    }
  };

  return (
    <div className="backdrop" onMouseDown={(e) => e.target === e.currentTarget && onClose()}>
      <form className="dialog" role="dialog" aria-label="Nueva tarea" onSubmit={submit}>
        <h2>Nueva tarea</h2>
        {projects.length === 0 ? (
          <p className="muted">Primero añade un proyecto en la vista Proyectos.</p>
        ) : (
          <>
            <div style={{ display: "grid", gridTemplateColumns: "140px 1fr", gap: 10 }}>
              <label className="field">
                <span>Ticket</span>
                <input className="input mono" placeholder="PROJ-1234" value={key} onChange={(e) => setKey(e.target.value)} required autoFocus />
              </label>
              <label className="field">
                <span>Título</span>
                <input className="input" value={title} onChange={(e) => setTitle(e.target.value)} required />
              </label>
            </div>
            <label className="field">
              <span>Proyecto</span>
              <select className="input" value={projectName} onChange={(e) => setProjectName(e.target.value)}>
                {projects.map((p) => (
                  <option key={p.name} value={p.name}>
                    {p.name} · {p.kind === "parent" ? "parent" : "repo"}
                  </option>
                ))}
              </select>
            </label>
            {project?.kind === "parent" && (
              <div className="field">
                <span>Submódulos que toca la tarea</span>
                <div className="choices">
                  {submodules.map((s) => (
                    <label key={s.path} className={`choice mono ${picked.includes(s.path) ? "on" : ""}`}>
                      <input type="checkbox" checked={picked.includes(s.path)} onChange={() => toggle(s.path)} />
                      {s.path}
                    </label>
                  ))}
                </div>
              </div>
            )}
            <label className="field">
              <span>Primer mensaje (opcional; si lo dejas vacío usa <span className="mono">/feature {key || "TICKET"}</span>)</span>
              <textarea className="input" value={prompt} onChange={(e) => setPrompt(e.target.value)} />
            </label>
          </>
        )}
        {error && <div className="error selectable">{error}</div>}
        <div style={{ display: "flex", gap: 8, justifyContent: "flex-end" }}>
          <button type="button" className="btn" onClick={onClose}>Cancelar</button>
          <button className="btn btn-p" disabled={busy || !project || (project.kind === "parent" && picked.length === 0)}>
            {busy ? "Preparando workspace…" : "Lanzar"}
          </button>
        </div>
      </form>
    </div>
  );
}
