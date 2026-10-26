import { useEffect, useRef, useState } from "react";
import { api, isCheckoutDirty, type BranchProposal, type Project, type Session, type Submodule, type SubmoduleSuggestion, type Task, type TaskView } from "../api";
import { branchName } from "../branchName";
import { isPhased } from "../workflowPresets";
import { t } from "../i18n";
import { AutopilotControls } from "./AutopilotControls";
import { WherePicker, whereOf, type Where } from "./WherePicker";
import { Button, Dialog, Segmented } from "./ui";
import "./NewTaskDialog.css";

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
  const [view, setView] = useState<TaskView>(existing?.view ?? "chat");
  // Experimental headless mode: only a backlog task can carry it now (no control sets it).
  const chatMode = !!existing?.chat_mode;
  const [prompt, setPrompt] = useState("");
  const [idea, setIdea] = useState("");
  const [suggested, setSuggested] = useState<SubmoduleSuggestion[]>([]);
  const [suggesting, setSuggesting] = useState(false);
  const [error, setError] = useState<string | null>(null);
  // The start was refused because the checkout has uncommitted changes (the daemon's reason).
  const [dirtyRefusal, setDirtyRefusal] = useState(false);
  const [fieldErrors, setFieldErrors] = useState<{ key?: string; title?: string; submodules?: string }>({});
  const [busy, setBusy] = useState(false);
  const project = projects.find((p) => p.name === projectName);
  const always = project?.kind === "parent" ? project.settings.submodules.always : [];
  const never = project?.kind === "parent" ? project.settings.submodules.never : [];
  // "always" is in every selection and "never" in none (the daemon enforces both too).
  const withRules = (paths: string[]) => [...always, ...paths.filter((p) => !always.includes(p) && !never.includes(p))];

  useEffect(() => {
    if (!whereTouched.current && project) setWhere(whereOf(null, project, unsetWhere));
  }, [project?.name]);

  // Saving mode and autopilot only exist in a phased workflow; a custom prompt runs outside it.
  const workflow = prompt.trim() ? "prompt" : (existing?.workflow ?? project?.workflow);
  const autopilotAllowed = isPhased(workflow ?? "", project?.settings);
  const autopilotOn = autopilot && autopilotAllowed;
  // Sent as a boolean whenever there is something to clear: absent keeps the stored one.
  const autopilotField = autopilotAllowed || existing?.autopilot ? { autopilot: autopilotOn } : {};
  // The branch name: proposed while the developer hasn't touched it. With the project's
  // own template it is that template's branch; `feature/` stays a fixed prefix only when
  // the name has none of its own (no template, or one that starts with `feature/`).
  const template = project?.settings.branch?.trim() || null;
  const prefixed = !template || template.startsWith("feature/");
  // A free task in a project without a template starts without a branch: the first
  // analysis creates it, so nothing is proposed (nor sent) unless the user types one.
  const analysisNames = origin === "libre" && !template;
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

  // Proposal for the branch, debounced 1 s; an answer to an older request is dropped.
  const pickedKey = picked.join("\n");
  useEffect(() => {
    const ticket = existing ? existing.key : origin === "jira" ? key.trim() : "";
    if (analysisNames && !editedRef.current) {
      sureRef.current = false;
      setBranch("");
      setBranchHint(null);
      return;
    }
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
  }, [projectName, key, title, notes, origin, pickedKey, edited, template, analysisNames]);

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

  const validate = () => {
    const errs: typeof fieldErrors = {};
    if (origin === "jira" && !key.trim()) errs.key = t("Falta el ticket");
    if (origin !== "claude" && !title.trim()) errs.title = origin === "jira" ? t("Falta el título") : t("Cuéntanos qué quieres hacer");
    if (origin !== "claude" && project?.kind === "parent" && picked.length === 0) errs.submodules = t("Elige al menos un submódulo");
    setFieldErrors(errs);
    return Object.keys(errs).length === 0;
  };

  const submit = async (e: React.FormEvent | null, chat = chatMode, inCheckout = where === "checkout") => {
    e?.preventDefault();
    if (!project || busy) return;
    if (!validate()) return;
    setBusy(true);
    setError(null);
    setDirtyRefusal(false);
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
        in_checkout: inCheckout,
        saving_mode: autopilotAllowed && (savingMode || autopilotOn),
        ...autopilotField,
        view,
        ...(chat || existing ? { chat_mode: chat } : {}),
        // A backlog task's notes as they read now ("" clears them).
        ...(existing ? { notes } : notes.trim() ? { notes: notes.trim() } : {}),
        ...(branch.trim() && (!template || sureRef.current) ? { branch: fullOf(branch.trim()) } : {}),
        ...(fromSession ? { from_session: fromSession.id } : {}),
        // A custom prompt runs outside the /feature workflow.
        ...(prompt.trim() ? { workflow: "prompt", prompt: prompt.trim() } : {}),
      });
      onStarted(task);
    } catch (err) {
      setDirtyRefusal(isCheckoutDirty(err));
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
        saving_mode: autopilotAllowed && (savingMode || autopilotOn),
        ...autopilotField,
        view,
        ...(chatMode ? { chat_mode: true } : {}),
      });
      onSaved(task);
    } catch (err) {
      setError((err as Error).message);
      setBusy(false);
    }
  };
  // The daemon refuses to start in a checkout with uncommitted changes: say it in human words.
  const dirtyCheckout = where === "checkout" && !!error && dirtyRefusal;
  const startInWorktree = () => {
    whereTouched.current = true;
    setWhere("worktree");
    submit(null, chatMode, false);
  };
  const openTerminal = async () => {
    if (!project) return;
    try {
      onConversation(project.name, await api.openProjectSession(project.name, { kind: "shell" }));
    } catch (err) {
      setError((err as Error).message);
    }
  };
  const canSave = !!project && !!title.trim() && (origin === "libre" || !!key.trim());

  const formId = "new-task-form";
  const title0 = fromSession ? t("Convertir en tarea") : existing ? t("Iniciar {key}", { key: existing.key }) : t("Nueva tarea");
  const startLabel = busy ? t("Preparando workspace…") : origin === "claude" ? t("Abrir conversación") : fromSession ? t("Convertir") : t("Iniciar");
  const footer = (
    <>
      <span className="ntd-hint">{t("⌘↵ Iniciar")}</span>
      <Button variant="ghost" onClick={onClose}>{t("Cancelar")}</Button>
      {origin !== "claude" && !fromSession && !existing && (
        <Button variant="secondary" disabled={busy || !canSave} onClick={saveToBacklog}
          title={t("Se queda en el backlog del proyecto, sin workspace ni agente, hasta que la empieces")}>
          {t("Guardar en backlog")}
        </Button>
      )}
      <Button variant="primary" type="submit" form={formId} disabled={busy || !project}>{startLabel}</Button>
    </>
  );
  const setSaving = (on: boolean) => {
    savingTouched.current = true;
    setSavingMode(on);
  };

  return (
    <Dialog title={title0} onClose={onClose} size={{ w: 600, h: 700 }} footer={footer}>
      <form id={formId} className="ntd" aria-label={t("Nueva tarea")} noValidate onSubmit={submit}
        onKeyDown={(e) => { if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) { e.preventDefault(); submit(null); } }}>
        {fromSession && (
          <p className="muted ntd-note">
            {t("«{label}» pasa a ser el Claude de una tarea libre: sigue con su conversación, ya puede editar y trabaja en una rama de tu checkout (donde está la conversación).", { label: fromSession.label })}
          </p>
        )}
        {projects.length === 0 ? (
          <p className="muted">{t("Primero añade un proyecto en la vista Proyectos.")}</p>
        ) : (
          <>
            {!draft && !fromSession && !existing && (
              <Segmented mode="radio" label={t("Origen")} value={origin} onChange={(v) => setOrigin(v as typeof origin)}
                options={[
                  { value: "jira", label: t("Ticket de Jira") },
                  { value: "libre", label: t("Tarea libre") },
                  { value: "claude", label: t("Con Claude") },
                ]} />
            )}
            {origin === "claude" ? null : origin === "jira" ? (
              <div className="ntd-ticket">
                <label className="field">
                  <span>{t("Ticket")}</span>
                  <input className="input mono" placeholder="PROJ-1234" value={key} onChange={(e) => setKey(e.target.value)} required aria-invalid={!!fieldErrors.key} autoFocus={!existing} readOnly={!!existing} />
                  {fieldErrors.key && <span className="ntd-err" role="alert">{fieldErrors.key}</span>}
                </label>
                <label className="field">
                  <span>{t("Título")}</span>
                  <input className="input" value={title} onChange={(e) => setTitle(e.target.value)} required aria-invalid={!!fieldErrors.title} />
                  {fieldErrors.title && <span className="ntd-err" role="alert">{fieldErrors.title}</span>}
                </label>
              </div>
            ) : (
              <label className="field">
                <span>{t("Qué quieres hacer")}</span>
                <input className="input" value={title} onChange={(e) => setTitle(e.target.value)} placeholder={t("Probar caché de tarifas en svc-billing")} required aria-invalid={!!fieldErrors.title} autoFocus />
                {fieldErrors.title && <span className="ntd-err" role="alert">{fieldErrors.title}</span>}
                <span className="muted ntd-help">
                  {t("Delilah le pone una clave LIBRE-n. Se inicia con /feature y esta descripción.")}
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
              <div className="ntd-auto">
                {autopilotAllowed ? (
                  <AutopilotControls
                    autopilot={autopilotOn ? { state: "running", since: "" } : null}
                    savingMode={savingMode}
                    onAutopilot={(a) => {
                      if (a === "on") savingTouched.current = true;
                      if (a === "on") setSavingMode(true);
                      setAutopilot(a === "on");
                    }}
                    onSavingMode={setSaving}
                  />
                ) : (
                  <span className="muted ntd-help">{workflow === "prompt" ? t("Ahorro y autopiloto solo están disponibles con el flujo del proyecto, no con un prompt propio.") : t("Ahorro y autopiloto solo están disponibles con un flujo por fases (configúralo en los ajustes del proyecto).")}</span>
                )}
                {autopilotAllowed && <span className="muted ntd-help">{t("Ahorro: una sesión de Claude nueva por fase, para gastar menos contexto")}</span>}
              </div>
            )}
            {origin !== "claude" && project?.kind === "parent" && (
              <div className="field">
                <span className="ntd-sub-hd">
                  {t("Submódulos que toca la tarea")}
                  <Button variant="secondary" size={28} className="ntd-suggest" onClick={suggest}
                    disabled={suggesting || (origin === "jira" && !key.trim()) || !title.trim()}>
                    {suggesting ? t("Claude está mirando el código…") : t("Sugerir con Claude")}
                  </Button>
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
                {fieldErrors.submodules && <span className="ntd-err" role="alert">{fieldErrors.submodules}</span>}
                {suggested.length > 0 && (
                  <ul className="md-list ntd-suggested">
                    {suggested.map((s) => (
                      <li key={s.path}>
                        <span className="mono">{s.path}</span>: {s.reason}
                      </li>
                    ))}
                  </ul>
                )}
              </div>
            )}
            {origin !== "claude" && (
              <details className="ntd-adv">
                <summary>{t("Opciones avanzadas")}</summary>
                <div className="field">
                  <span>{t("Vista")}</span>
                  <Segmented mode="radio" label={t("Vista")} value={view} onChange={(v) => setView(v as TaskView)}
                    options={[{ value: "chat", label: "Chat", tag: "beta" }, { value: "terminal", label: "Terminal" }]} />
                </div>
                <label className="field">
                  <span>
                    {t("Rama")}
                    {branchHint && (
                      <span className="muted ntd-help">
                        {branchHint === "loading" ? t("Proponiendo…") : branchHint === "claude" ? t("propuesta por Claude") : branchHint === "title" ? t("a partir del título") : t("a partir de la clave")}
                      </span>
                    )}
                  </span>
                  <span className="ntd-branch">
                    {prefixed && <span className="mono muted">feature/</span>}
                    <input className="input mono" aria-label={t("Rama")} value={branch} onChange={(e) => editBranch(e.target.value)} placeholder={analysisNames ? t("la creará el análisis") : t("automática si la dejas vacía")} />
                  </span>
                </label>
                {!fromSession && (
                  <label className="field">
                    <span>
                      {t("Primer mensaje (opcional; si lo dejas vacío usa")}{" "}
                      <span className="mono">/feature {origin === "libre" ? t("<lo que quieres hacer>") : key || "TICKET"}</span>)
                    </span>
                    <textarea className="input" value={prompt} onChange={(e) => setPrompt(e.target.value)} />
                  </label>
                )}
              </details>
            )}
          </>
        )}
        {error && !dirtyCheckout && <div className="error selectable">{error}</div>}
        {dirtyCheckout && (
          <div className="ntd-dirty" role="alert">
            <div>{t("Tu checkout tiene cambios sin guardar. Guárdalos o descártalos, o inicia la tarea en un worktree aislado.")}</div>
            <div className="ntd-dirty-actions">
              <Button variant="secondary" size={28} disabled={busy} onClick={startInWorktree}>{t("Iniciar en un worktree")}</Button>
              <Button variant="ghost" size={28} icon="console" disabled={busy} onClick={openTerminal}>{t("Abrir una terminal")}</Button>
            </div>
          </div>
        )}
        {error && chatMode && error.includes("chat mode could not start") && (
          <Button variant="secondary" disabled={busy} onClick={() => submit(null, false)}>
            {t("Iniciar en terminal")}
          </Button>
        )}
      </form>
    </Dialog>
  );
}
