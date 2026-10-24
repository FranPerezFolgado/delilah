import { useEffect, useRef, useState } from "react";
import { api, type BranchProposal, type Project, type Session, type Submodule, type SubmoduleSuggestion, type Task } from "../api";
import { branchName } from "../branchName";
import { t } from "../i18n";
import { WherePicker, whereOf, type Where } from "./WherePicker";

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
  /** "Empezar": a backlog task to start; its key stays, the rest comes prefilled. */
  existing?: Task;
  onClose: () => void;
  onStarted: (task: Task) => void;
  /** "Guardar en backlog": saved, not started. */
  onSaved: (task: Task) => void;
  /** "Con Claude": the idea conversation opened in the project. */
  onConversation: (project: string, session: Session) => void;
}

export function NewTaskDialog({ projects, draft, project: initialProject, fromSession, existing, onClose, onStarted, onSaved, onConversation }: Props) {
  // A Jira ticket, or a free task Delilah names itself (LIBRE-n).
  const [origin, setOrigin] = useState<"jira" | "libre" | "claude">(
    fromSession || existing?.key.startsWith("LIBRE-") ? "libre" : "jira",
  );
  const [key, setKey] = useState(existing?.key ?? draft?.key ?? "");
  const [title, setTitle] = useState(existing?.title ?? draft?.title ?? "");
  const [notes, setNotes] = useState(existing?.notes ?? "");
  // A ticket with components most likely belongs to the parent project.
  const preferred =
    projects.find((p) => p.name === existing?.project) ??
    projects.find((p) => p.name === initialProject) ??
    (draft?.components.length ? projects.find((p) => p.kind === "parent") : undefined) ??
    projects[0];
  const [projectName, setProjectName] = useState(preferred?.name ?? "");
  // In the project's checkout (a branch there), or isolated in a worktree: a backlog
  // task's saved choice, else the project's default (a worktree for a saved task, the
  // checkout for a new one when it has none) until the user picks one.
  const unsetWhere: Where = existing ? "worktree" : "checkout";
  const [where, setWhere] = useState<Where>(whereOf(existing?.in_checkout, preferred, unsetWhere));
  const whereTouched = useRef(existing?.in_checkout != null);
  const [submodules, setSubmodules] = useState<Submodule[]>([]);
  const [picked, setPicked] = useState<string[]>([]);
  // Each step in a fresh session: a backlog task keeps its choice, a new one starts from the preference.
  const [savingMode, setSavingMode] = useState(existing?.saving_mode ?? false);
  const savingTouched = useRef(!!existing);
  // Autopilot implies saving mode: ticked and locked while it is on.
  const [autopilot, setAutopilot] = useState(!!existing?.autopilot);
  const [prompt, setPrompt] = useState("");
  const [idea, setIdea] = useState("");
  const [suggested, setSuggested] = useState<SubmoduleSuggestion[]>([]);
  const [suggesting, setSuggesting] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const project = projects.find((p) => p.name === projectName);
  const always = project?.kind === "parent" ? project.settings.submodules.always : [];
  const never = project?.kind === "parent" ? project.settings.submodules.never : [];
  // "always" is in every selection and "never" in none (the daemon enforces both too).
  const withRules = (paths: string[]) => [...always, ...paths.filter((p) => !always.includes(p) && !never.includes(p))];

  useEffect(() => {
    if (!whereTouched.current && project) setWhere(whereOf(null, project, unsetWhere));
  }, [project?.name]);

  // Only the spec workflow has autopilot; a custom prompt runs outside it.
  const workflow = prompt.trim() ? "prompt" : (existing?.workflow ?? project?.workflow);
  const autopilotAllowed = workflow === "feature";
  const autopilotOn = autopilot && autopilotAllowed;
  // Sent as a boolean whenever there is something to clear: absent keeps the stored one.
  const autopilotField = autopilotAllowed || existing?.autopilot ? { autopilot: autopilotOn } : {};
  // The branch name: proposed while the developer hasn't touched it. With the project's
  // own template it is that template's branch; `feature/` stays a fixed prefix only when
  // the name has none of its own (no template, or one that starts with `feature/`).
  const template = project?.settings.branch?.trim() || null;
  const prefixed = !template || template.startsWith("feature/");
  const fieldOf = (full: string) => (prefixed ? full.replace(/^feature\//, "") : full);
  const fullOf = (value: string) => (prefixed ? "feature/" + value : value);
  const [branch, setBranch] = useState("");
  // The field holds a name the daemon proposed (or the user typed), not just our instant guess.
  const sureRef = useRef(false);
  const [edited, setEdited] = useState(false);
  const editedRef = useRef(false);
  const [branchHint, setBranchHint] = useState<"loading" | BranchProposal["source"] | null>(null);

  useEffect(() => {
    if (savingTouched.current) return;
    api
      .preference<boolean>("saving-mode")
      .then((on) => !savingTouched.current && setSavingMode(on === true))
      .catch(() => {});
  }, []);

  useEffect(() => {
    setPicked([]);
    setSubmodules([]);
    if (project?.kind !== "parent") return;
    api
      .submodules(project.name)
      .then((subs) => {
        setSubmodules(subs.filter((s) => !never.includes(s.path)));
        const wanted = draft?.components ?? [];
        const present = subs.map((s) => s.path);
        setPicked(withRules(subs.filter((s) => wanted.includes(s.name) || wanted.includes(s.path)).map((s) => s.path)).filter((p) => present.includes(p)));
      })
      .catch((e) => setError(e.message));
  }, [project?.name, project?.kind]);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && onClose();
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose]);

  // Proposal for the branch, debounced 1 s; an answer to an older request is dropped.
  const pickedKey = picked.join("\n");
  useEffect(() => {
    const ticket = existing ? existing.key : origin === "jira" ? key.trim() : "";
    if (!projectName || !title.trim() || editedRef.current) {
      setBranchHint(null);
      return;
    }
    // With a template, the name it gives is known at once; the daemon's proposal replaces it.
    if (template) {
      sureRef.current = false;
      setBranch(fieldOf(branchName(template, ticket || "LIBRE-?", title)));
    }
    let stale = false;
    setBranchHint("loading");
    const timer = setTimeout(() => {
      api
        .proposeBranch(projectName, {
          ...(ticket ? { key: ticket } : {}),
          title: title.trim(),
          ...(notes.trim() ? { notes: notes.trim() } : {}),
          free: origin === "libre",
          submodules: picked,
        })
        .then((res) => {
          if (stale || editedRef.current) return;
          sureRef.current = !!res.branch;
          if (res.branch || !template) setBranch(res.branch ? fieldOf(res.branch) : "");
          setBranchHint(res.branch ? res.source : null);
        })
        .catch(() => {
          if (!stale) setBranchHint(null);
        });
    }, 1000);
    return () => {
      stale = true;
      clearTimeout(timer);
    };
  }, [projectName, key, title, notes, origin, pickedKey, edited, template]);

  const editBranch = (value: string) => {
    // Raw value in state (a typed space must stay); trimmed only to decide and on submit.
    const v = prefixed ? value.replace(/^feature\//, "") : value;
    const has = v.trim() !== "";
    editedRef.current = has;
    sureRef.current = has;
    setEdited(has);
    setBranch(v);
    if (has) setBranchHint(null);
  };

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
      setPicked(withRules(res.submodules.map((s) => s.path)));
    } catch (e) {
      setError((e as Error).message);
    } finally {
      setSuggesting(false);
    }
  };

  const toggle = (path: string) => !always.includes(path) && setPicked((p) => (p.includes(path) ? p.filter((x) => x !== path) : [...p, path]));

  const submit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!project) return;
    setBusy(true);
    setError(null);
    if (origin === "claude") {
      try {
        const s = await api.openProjectSession(project.name, { kind: "claude", conversation: true, prompt: idea.trim() || undefined });
        onConversation(project.name, s);
      } catch (err) {
        setError((err as Error).message);
        setBusy(false);
      }
      return;
    }
    try {
      const task = await api.run({
        key: existing ? existing.key : origin === "libre" ? "" : key.trim(),
        title: title.trim(),
        project: project.name,
        submodules: picked,
        free: origin === "libre",
        in_checkout: where === "checkout",
        saving_mode: savingMode || autopilotOn,
        ...autopilotField,
        // A backlog task's notes as they read now ("" clears them).
        ...(existing ? { notes } : notes.trim() ? { notes: notes.trim() } : {}),
        ...(branch.trim() && (!template || sureRef.current) ? { branch: fullOf(branch.trim()) } : {}),
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

  const saveToBacklog = async () => {
    if (!project) return;
    setBusy(true);
    setError(null);
    try {
      const task = await api.createTask({
        key: origin === "libre" ? undefined : key.trim(),
        title: title.trim(),
        project: project.name,
        notes: notes.trim() || undefined,
        free: origin === "libre",
        in_checkout: where === "checkout",
        saving_mode: savingMode || autopilotOn,
        ...autopilotField,
      });
      onSaved(task);
    } catch (err) {
      setError((err as Error).message);
      setBusy(false);
    }
  };
  const canSave = !!project && !!title.trim() && (origin === "libre" || !!key.trim());

  return (
    <div className="backdrop" onMouseDown={(e) => e.target === e.currentTarget && onClose()}>
      <form className="dialog" role="dialog" aria-label={t("Nueva tarea")} onSubmit={submit}>
        <h2>{fromSession ? t("Convertir en tarea") : existing ? t("Empezar {key}", { key: existing.key }) : t("Nueva tarea")}</h2>
        {fromSession && (
          <p className="muted" style={{ margin: 0, fontSize: 12.5 }}>
            {t("«{label}» pasa a ser el Claude de una tarea libre: sigue con su conversación, ya puede editar y trabaja en una rama de tu checkout (donde está la conversación).", { label: fromSession.label })}
          </p>
        )}
        {projects.length === 0 ? (
          <p className="muted">{t("Primero añade un proyecto en la vista Proyectos.")}</p>
        ) : (
          <>
            {!draft && !fromSession && !existing && (
              <div className="segs" role="radiogroup" aria-label={t("Origen")} style={{ alignSelf: "flex-start" }}>
                <button type="button" role="radio" aria-checked={origin === "jira"} className={`seg ${origin === "jira" ? "on" : ""}`} onClick={() => setOrigin("jira")}>
                  {t("Ticket de Jira")}
                </button>
                <button type="button" role="radio" aria-checked={origin === "libre"} className={`seg ${origin === "libre" ? "on" : ""}`} onClick={() => setOrigin("libre")}>
                  {t("Tarea libre")}
                </button>
                <button type="button" role="radio" aria-checked={origin === "claude"} className={`seg ${origin === "claude" ? "on" : ""}`} onClick={() => setOrigin("claude")}>
                  {t("Con Claude")}
                </button>
              </div>
            )}
            {origin === "claude" ? null : origin === "jira" ? (
              <div style={{ display: "grid", gridTemplateColumns: "140px 1fr", gap: 10 }}>
                <label className="field">
                  <span>{t("Ticket")}</span>
                  <input className="input mono" placeholder="PROJ-1234" value={key} onChange={(e) => setKey(e.target.value)} required autoFocus={!existing} readOnly={!!existing} />
                </label>
                <label className="field">
                  <span>{t("Título")}</span>
                  <input className="input" value={title} onChange={(e) => setTitle(e.target.value)} required />
                </label>
              </div>
            ) : (
              <label className="field">
                <span>{t("Qué quieres hacer")}</span>
                <input className="input" value={title} onChange={(e) => setTitle(e.target.value)} placeholder={t("Probar caché de tarifas en svc-billing")} required autoFocus />
                <span className="muted" style={{ fontSize: 11.5, fontWeight: 400 }}>
                  {t("Delilah le pone una clave LIBRE-n. Arranca con /feature y esta descripción.")}
                </span>
              </label>
            )}
            <label className="field">
              <span>{t("Proyecto")}</span>
              <select className="input" value={projectName} onChange={(e) => setProjectName(e.target.value)} disabled={!!fromSession}>
                {projects.map((p) => (
                  <option key={p.name} value={p.name}>
                    {p.name} · {p.kind === "parent" ? "parent" : "repo"}
                  </option>
                ))}
              </select>
            </label>
            {origin === "claude" && (
              <label className="field">
                <span>{t("Tu idea")}</span>
                <textarea className="input" value={idea} onChange={(e) => setIdea(e.target.value)} rows={4} autoFocus
                  placeholder={t("Opcional: cuéntale a Claude qué quieres conseguir y lo convertís juntos en una tarea.")} />
              </label>
            )}
            {origin !== "claude" && !fromSession && (
              <label className="field">
                <span>{t("Notas (opcional; el agente las recibe al empezar)")}</span>
                <textarea className="input" value={notes} onChange={(e) => setNotes(e.target.value)} rows={3} />
              </label>
            )}
            {origin !== "claude" && !fromSession && <WherePicker value={where} onChange={(w) => { whereTouched.current = true; setWhere(w ?? "checkout"); }} project={project} />}
            {origin !== "claude" && (
              <label style={{ display: "flex", alignItems: "center", gap: 8, fontSize: 13 }} title={t("Cada paso (plan, implementación, revisión) va en una sesión nueva, con las notas del anterior")}>
                <input
                  type="checkbox"
                  checked={savingMode || autopilotOn}
                  disabled={autopilotOn}
                  onChange={(e) => {
                    savingTouched.current = true;
                    setSavingMode(e.target.checked);
                  }}
                />
                <span>{t("Modo ahorro")}</span>
              </label>
            )}
            {origin !== "claude" && autopilotAllowed && (
              <label style={{ display: "flex", alignItems: "center", gap: 8, fontSize: 13 }} title={t("El modo ahorro queda activo mientras el autopiloto esté activado")}>
                <input
                  type="checkbox"
                  checked={autopilot}
                  onChange={(e) => {
                    if (e.target.checked) {
                      savingTouched.current = true;
                      setSavingMode(true);
                    }
                    setAutopilot(e.target.checked);
                  }}
                />
                <span>{t("Autopiloto")}</span>
              </label>
            )}
            {origin !== "claude" && project?.kind === "parent" && (
              <div className="field">
                <span style={{ display: "flex", alignItems: "center", gap: 8 }}>
                  {t("Submódulos que toca la tarea")}
                  <button type="button" className="btn" style={{ marginLeft: "auto", height: 24 }} onClick={suggest}
                    disabled={suggesting || (origin === "jira" && !key.trim()) || !title.trim()}>
                    {suggesting ? t("Claude está mirando el código…") : t("Sugerir con Claude")}
                  </button>
                </span>
                <div className="choices">
                  {submodules.map((s) => (
                    <label key={s.path} className={`choice mono ${picked.includes(s.path) ? "on" : ""}`}>
                      <input type="checkbox" checked={picked.includes(s.path)} onChange={() => toggle(s.path)}
                        disabled={always.includes(s.path)} title={always.includes(s.path) ? t("Siempre incluido (ajustes del proyecto)") : undefined} />
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
            {origin !== "claude" && (
              <label className="field">
                <span>
                  {t("Rama")}
                  {branchHint && (
                    <span className="muted" style={{ fontSize: 11.5, fontWeight: 400, marginLeft: 8 }}>
                      {branchHint === "loading" ? t("Proponiendo…") : branchHint === "claude" ? t("propuesta por Claude") : branchHint === "title" ? t("a partir del título") : t("a partir de la clave")}
                    </span>
                  )}
                </span>
                <span style={{ display: "flex", alignItems: "center", gap: 6 }}>
                  {prefixed && <span className="mono muted">feature/</span>}
                  <input className="input mono" style={{ flex: 1 }} aria-label={t("Rama")} value={branch} onChange={(e) => editBranch(e.target.value)} placeholder={t("automática si la dejas vacía")} />
                </span>
              </label>
            )}
            {origin !== "claude" && !fromSession && (
            <label className="field">
              <span>
                {t("Primer mensaje (opcional; si lo dejas vacío usa")}{" "}
                <span className="mono">/feature {origin === "libre" ? t("<lo que quieres hacer>") : key || "TICKET"}</span>)
              </span>
              <textarea className="input" value={prompt} onChange={(e) => setPrompt(e.target.value)} />
            </label>
            )}
          </>
        )}
        {error && <div className="error selectable">{error}</div>}
        <div style={{ display: "flex", gap: 8, justifyContent: "flex-end" }}>
          <button type="button" className="btn" onClick={onClose}>{t("Cancelar")}</button>
          {origin !== "claude" && !fromSession && !existing && (
            <button type="button" className="btn" disabled={busy || !canSave} onClick={saveToBacklog}
              title={t("Se queda en el backlog del proyecto, sin workspace ni agente, hasta que la empieces")}>
              {t("Guardar en backlog")}
            </button>
          )}
          <button className="btn btn-p" disabled={busy || !project || (origin !== "claude" && project.kind === "parent" && picked.length === 0)}>
            {busy ? t("Preparando workspace…") : origin === "claude" ? t("Abrir conversación") : fromSession ? t("Convertir") : existing ? t("Empezar") : t("Lanzar")}
          </button>
        </div>
      </form>
    </div>
  );
}
