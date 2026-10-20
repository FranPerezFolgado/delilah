import { useEffect, useState } from "react";
import { api, type Project, type Submodule, type SubmoduleSuggestion, type Task } from "../api";

/** A ticket to start from, e.g. picked in the Jira view. */
export interface TaskDraft {
  key: string;
  title: string;
  /** Jira components: submodules with these names are preselected. */
  components: string[];
  description?: string;
}

interface Props {
  projects: Project[];
  draft?: TaskDraft;
  /** The project to start in, e.g. the one on screen. */
  project?: string;
  /** "Convertir en tarea": a project's Claude session that becomes the task's. */
  fromSession?: { id: string; label: string };
  onClose: () => void;
  onStarted: (task: Task) => void;
}

export function NewTaskDialog({ projects, draft, project: initialProject, fromSession, onClose, onStarted }: Props) {
  // A Jira ticket, or a free task Delilah names itself (LIBRE-n).
  const [origin, setOrigin] = useState<"jira" | "libre">(fromSession ? "libre" : "jira");
  // In the project's checkout (a branch there), or isolated in a worktree.
  const [where, setWhere] = useState<"checkout" | "worktree">("checkout");
  const [key, setKey] = useState(draft?.key ?? "");
  const [title, setTitle] = useState(draft?.title ?? "");
  // A ticket with components most likely belongs to the parent project.
  const preferred =
    projects.find((p) => p.name === initialProject) ??
    (draft?.components.length ? projects.find((p) => p.kind === "parent") : undefined) ??
    projects[0];
  const [projectName, setProjectName] = useState(preferred?.name ?? "");
  const [submodules, setSubmodules] = useState<Submodule[]>([]);
  const [picked, setPicked] = useState<string[]>([]);
  const [prompt, setPrompt] = useState("");
  const [suggested, setSuggested] = useState<SubmoduleSuggestion[]>([]);
  const [suggesting, setSuggesting] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const project = projects.find((p) => p.name === projectName);

  useEffect(() => {
    setPicked([]);
    setSubmodules([]);
    if (project?.kind !== "parent") return;
    api
      .submodules(project.name)
      .then((subs) => {
        setSubmodules(subs);
        const wanted = draft?.components ?? [];
        setPicked(subs.filter((s) => wanted.includes(s.name) || wanted.includes(s.path)).map((s) => s.path));
      })
      .catch((e) => setError(e.message));
  }, [project?.name, project?.kind]);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && onClose();
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose]);

  // A quick read-only Claude pass over the parent's code picks the submodules.
  const suggest = async () => {
    if (!project) return;
    setSuggesting(true);
    setError(null);
    try {
      const res = await api.suggestSubmodules(project.name, {
        key: key.trim() || "LIBRE",
        title: title.trim(),
        components: draft?.components ?? [],
        description: draft?.description,
      });
      setSuggested(res.submodules);
      setPicked(res.submodules.map((s) => s.path));
    } catch (e) {
      setError((e as Error).message);
    } finally {
      setSuggesting(false);
    }
  };

  const toggle = (path: string) => setPicked((p) => (p.includes(path) ? p.filter((x) => x !== path) : [...p, path]));

  const submit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!project) return;
    setBusy(true);
    setError(null);
    try {
      const task = await api.run({
        key: origin === "libre" ? "" : key.trim(),
        title: title.trim(),
        project: project.name,
        submodules: picked,
        free: origin === "libre",
        in_checkout: where === "checkout",
        ...(fromSession ? { from_session: fromSession.id } : {}),
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
        <h2>{fromSession ? "Convertir en tarea" : "Nueva tarea"}</h2>
        {fromSession && (
          <p className="muted" style={{ margin: 0, fontSize: 12.5 }}>
            «{fromSession.label}» pasa a ser el Claude de una tarea libre: sigue con su conversación, ya puede editar y trabaja en
            una rama de tu checkout (donde está la conversación).
          </p>
        )}
        {projects.length === 0 ? (
          <p className="muted">Primero añade un proyecto en la vista Proyectos.</p>
        ) : (
          <>
            {!draft && !fromSession && (
              <div className="segs" role="radiogroup" aria-label="Origen" style={{ alignSelf: "flex-start" }}>
                <button type="button" role="radio" aria-checked={origin === "jira"} className={`seg ${origin === "jira" ? "on" : ""}`} onClick={() => setOrigin("jira")}>
                  Ticket de Jira
                </button>
                <button type="button" role="radio" aria-checked={origin === "libre"} className={`seg ${origin === "libre" ? "on" : ""}`} onClick={() => setOrigin("libre")}>
                  Tarea libre
                </button>
              </div>
            )}
            {origin === "jira" ? (
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
            ) : (
              <label className="field">
                <span>Qué quieres hacer</span>
                <input className="input" value={title} onChange={(e) => setTitle(e.target.value)} placeholder="Probar caché de tarifas en svc-billing" required autoFocus />
                <span className="muted" style={{ fontSize: 11.5, fontWeight: 400 }}>
                  Delilah le pone una clave LIBRE-n. Arranca con <span className="mono">/feature</span> y esta descripción.
                </span>
              </label>
            )}
            <label className="field">
              <span>Proyecto</span>
              <select className="input" value={projectName} onChange={(e) => setProjectName(e.target.value)} disabled={!!fromSession}>
                {projects.map((p) => (
                  <option key={p.name} value={p.name}>
                    {p.name} · {p.kind === "parent" ? "parent" : "repo"}
                  </option>
                ))}
              </select>
            </label>
            {!fromSession && (
            <div className="field">
              <span>Dónde trabaja el agente</span>
              <div className="where">
                <label className={`where-opt ${where === "checkout" ? "on" : ""}`}>
                  <input type="radio" name="where" checked={where === "checkout"} onChange={() => setWhere("checkout")} />
                  <span>
                    <b>En tu checkout</b>
                    <span className="muted">
                      Crea la rama en <span className="mono">{project?.path ?? "tu carpeta"}</span>
                      {project?.kind === "parent" ? " (en los submódulos elegidos)" : ""} y trabaja ahí. Instantáneo; ves los cambios en tu IDE.
                      Una tarea a la vez, con el checkout limpio.
                    </span>
                  </span>
                </label>
                <label className={`where-opt ${where === "worktree" ? "on" : ""}`}>
                  <input type="radio" name="where" checked={where === "worktree"} onChange={() => setWhere("worktree")} />
                  <span>
                    <b>Aislada (worktree)</b>
                    <span className="muted">Copia aparte. Varias tareas en paralelo sin tocar tu carpeta; la primera vez clona los submódulos.</span>
                  </span>
                </label>
              </div>
            </div>
            )}
            {project?.kind === "parent" && (
              <div className="field">
                <span style={{ display: "flex", alignItems: "center", gap: 8 }}>
                  Submódulos que toca la tarea
                  <button type="button" className="btn" style={{ marginLeft: "auto", height: 24 }} onClick={suggest}
                    disabled={suggesting || (origin === "jira" && !key.trim()) || !title.trim()}>
                    {suggesting ? "Claude está mirando el código…" : "Sugerir con Claude"}
                  </button>
                </span>
                <div className="choices">
                  {submodules.map((s) => (
                    <label key={s.path} className={`choice mono ${picked.includes(s.path) ? "on" : ""}`}>
                      <input type="checkbox" checked={picked.includes(s.path)} onChange={() => toggle(s.path)} />
                      {s.path}
                    </label>
                  ))}
                </div>
                {suggested.length > 0 && (
                  <ul className="md-list" style={{ margin: "4px 0 0", fontSize: 12, color: "var(--tx2)" }}>
                    {suggested.map((s) => (
                      <li key={s.path}>
                        <span className="mono" style={{ color: "var(--tx)" }}>{s.path}</span>: {s.reason}
                      </li>
                    ))}
                  </ul>
                )}
              </div>
            )}
            {!fromSession && (
            <label className="field">
              <span>
                Primer mensaje (opcional; si lo dejas vacío usa{" "}
                <span className="mono">/feature {origin === "libre" ? "<lo que quieres hacer>" : key || "TICKET"}</span>)
              </span>
              <textarea className="input" value={prompt} onChange={(e) => setPrompt(e.target.value)} />
            </label>
            )}
          </>
        )}
        {error && <div className="error selectable">{error}</div>}
        <div style={{ display: "flex", gap: 8, justifyContent: "flex-end" }}>
          <button type="button" className="btn" onClick={onClose}>Cancelar</button>
          <button className="btn btn-p" disabled={busy || !project || (project.kind === "parent" && picked.length === 0)}>
            {busy ? "Preparando workspace…" : fromSession ? "Convertir" : "Lanzar"}
          </button>
        </div>
      </form>
    </div>
  );
}
