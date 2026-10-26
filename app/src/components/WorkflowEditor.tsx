import type { ProjectSettings, SettingsError, Start, Task } from "../api";
import { t } from "../i18n";
import { workflowPresets } from "../workflowPresets";
import { SkillPicker, useSkills } from "./SkillPicker";
import "./WorkflowEditor.css";

type ProjectWorkflow = NonNullable<ProjectSettings["workflow"]>;
type Fields = SettingsError["fields"];
type Mode = "builtin" | "message" | "nothing";

/** The skill a template invokes: its leading `/name`, up to whitespace. */
const leadingSkill = (template: string): string | null => {
  const m = /^\s*\/(\S+)/.exec(template);
  return m ? m[1] : null;
};

/**
 * Edits how a project's tasks start: one session (built-in prompt, a custom message or nothing)
 * or phases (plan, implementation, review commands). `value` null/undefined is the built-in prompt.
 * Field errors from the daemon (`workflow.*`) are shown next to their fields.
 */
export function WorkflowEditor({ project, value, errors, onChange }: {
  project: string;
  value: ProjectWorkflow | null | undefined;
  errors: Fields;
  onChange: (w: ProjectWorkflow) => void;
}) {
  const skills = useSkills(project);
  const workflow: ProjectWorkflow = value ?? { kind: "single", start: { type: "builtin" } };
  const presets = workflowPresets();

  const Err = ({ field }: { field: string }) => (
    <>{errors.filter((f) => f.field === field).map((f, i) => (
      <span key={i} className="error field-error selectable" data-field={f.field}>{f.message}</span>
    ))}</>
  );
  const known = ["workflow.start.template", "workflow.plan", "workflow.implement", "workflow.review"];
  const other = errors.filter((f) => (f.field === "workflow" || f.field.startsWith("workflow.") || f.field.startsWith("workflow[")) && !known.includes(f.field));

  const setStart = (mode: Mode) => {
    const start: Start =
      mode === "message" ? { type: "message", template: workflow.kind === "single" && workflow.start.type === "message" ? workflow.start.template : "" } : { type: mode };
    onChange({ kind: "single", start });
  };
  const mode: Mode = workflow.kind === "single" ? workflow.start.type : "builtin";

  const template = workflow.kind === "single" && workflow.start.type === "message" ? workflow.start.template : null;
  const name = template === null ? null : leadingSkill(template);
  const missing = name !== null && skills !== null && skills.length > 0 && !skills.some((s) => s.name === name);

  const help = (key: "plan" | "implement" | "review") =>
    key === "implement"
      ? t("Usa {ticket}, {title} y {task}; {spec_dir} es la carpeta de la spec.", { ticket: "{ticket}", title: "{title}", task: "{task}", spec_dir: "{spec_dir}" })
      : t("Usa {ticket}, {title} y {task}.", { ticket: "{ticket}", title: "{title}", task: "{task}" });

  const phasedField = (key: "plan" | "implement" | "review", label: string) => {
    if (workflow.kind !== "phased") return null;
    return (
      <div className="field" key={key}>
        <span>{label}</span>
        <SkillPicker skills={skills} value={workflow[key]} label={label}
          onChange={(v) => onChange({ ...workflow, [key]: v })} />
        <span className="ps-help">{help(key)}</span>
        <Err field={`workflow.${key}`} />
      </div>
    );
  };

  return (
    <div className="workflow-editor">
      <label className="field">
        <span>{t("Preset")}</span>
        <select className="input" aria-label={t("Preset")} value="" onChange={(e) => {
          const p = presets.find((x) => x.id === e.target.value);
          if (p) onChange(p.workflow);
        }}>
          <option value="">{t("Aplicar un preset…")}</option>
          {presets.map((p) => <option key={p.id} value={p.id}>{p.name}</option>)}
        </select>
      </label>

      {workflow.kind === "single" ? (
        <>
          <label className="field">
            <span>{t("Inicio")}</span>
            <select className="input" aria-label={t("Inicio")} value={mode} onChange={(e) => setStart(e.target.value as Mode)}>
              <option value="builtin">{t("Prompt integrado")}</option>
              <option value="message">{t("Mensaje propio")}</option>
              <option value="nothing">{t("Nada")}</option>
            </select>
          </label>
          {template !== null && (
            <div className="field">
              <SkillPicker
                skills={skills}
                value={template}
                label={t("Comando")}
                onChange={(v) => onChange({ kind: "single", start: { type: "message", template: v } })}
              />
              {missing && <span role="alert" className="wf-warning">{t("Delilah no encuentra la skill {name}; puedes guardar igualmente.", { name: `/${name}` })}</span>}
              <span className="ps-help">{t("Se envía al empezar la tarea. Usa {ticket}, {title} y {task}.", { ticket: "{ticket}", title: "{title}", task: "{task}" })}</span>
              <Err field="workflow.start.template" />
            </div>
          )}
        </>
      ) : (
        <>
          <span className="ps-help">{t("Tres sesiones: plan, implementación y revisión, cada una con su comando.")}</span>
          {phasedField("plan", t("Plan"))}
          {phasedField("implement", t("Implementación"))}
          {phasedField("review", t("Revisión"))}
        </>
      )}
      {other.map((f, i) => <span key={i} className="error field-error selectable" data-field={f.field}>{f.message}</span>)}
    </div>
  );
}

/** How one task starts: the project's own start, a message of its own, or nothing. */
export interface StartChoice {
  kind: "project" | "prompt" | "none";
  /** The message when `kind` is "prompt". */
  prompt: string;
}

/** The choice a stored task carries (the legacy `feature` workflow is the project's). */
export const startChoiceOf = (task?: Pick<Task, "workflow" | "start_message"> | null): StartChoice =>
  task?.workflow === "prompt" ? { kind: "prompt", prompt: task.start_message ?? "" }
  : task?.workflow === "none" ? { kind: "none", prompt: "" }
  : { kind: "project", prompt: "" };

/** The `workflow`/`prompt` request fields; `always` also states the project's own start. */
export const startFields = (c: StartChoice, always = false): { workflow?: string; prompt?: string } => {
  const kind = c.kind;
  if (kind === "prompt") return { workflow: "prompt", prompt: c.prompt.trim() };
  if (kind === "none") return { workflow: "none" };
  return always ? { workflow: "project" } : {};
};

/** A template as the task will send it: placeholders filled, doubled braces made single. */
const fillTemplate = (template: string, vars: Record<string, string>): string =>
  template.replace(/\{\{|\}\}|\{(\w+)\}/g, (m, name?: string) => (m === "{{" ? "{" : m === "}}" ? "}" : name && name in vars ? vars[name] : m));

/** What the project's workflow sends when a task starts, for the typed ticket and title. */
function projectStart(settings: Pick<ProjectSettings, "workflow"> | undefined, ticket: string, title: string): string {
  const vars = { ticket, title, task: ticket || title };
  const w = settings?.workflow;
  if (!w || (w.kind === "single" && w.start.type === "builtin")) return t("Prompt integrado");
  if (w.kind === "phased") return fillTemplate(w.plan, vars);
  if (w.start.type === "message") return fillTemplate(w.start.template, vars);
  return t("Nada");
}

/**
 * The single start of one task: what the project's workflow resolves to (filled with the typed
 * ticket and title), or a message of its own, or nothing. Same picker as the project settings.
 */
export function StartPicker({ project, settings, ticket, title, value, errors = [], disabled, onChange }: {
  project: string;
  settings?: Pick<ProjectSettings, "workflow">;
  ticket: string;
  title: string;
  value: StartChoice;
  errors?: Fields;
  disabled?: boolean;
  onChange: (c: StartChoice) => void;
}) {
  const skills = useSkills(project);
  const name = value.kind === "prompt" ? leadingSkill(value.prompt) : null;
  const missing = name !== null && skills !== null && skills.length > 0 && !skills.some((s) => s.name === name);
  const own = errors.filter((f) => f.field === "workflow" || f.field.startsWith("workflow.") || f.field === "start_message" || f.field === "prompt");
  return (
    <div className="workflow-editor start-picker">
      <label className="field">
        <span>{t("Al empezar")}</span>
        <select className="input" aria-label={t("Al empezar")} value={value.kind} disabled={disabled}
          onChange={(e) => onChange({ ...value, kind: e.target.value as StartChoice["kind"] })}>
          <option value="project">{t("El del proyecto")}</option>
          <option value="prompt">{t("Mensaje propio")}</option>
          <option value="none">{t("Nada")}</option>
        </select>
      </label>
      {value.kind === "project" && (
        <span className="ps-help start-resolved mono selectable">{projectStart(settings, ticket, title)}</span>
      )}
      {value.kind === "prompt" && (
        <div className="field">
          <SkillPicker skills={skills} value={value.prompt} label={t("Comando")} onChange={(v) => onChange({ kind: "prompt", prompt: v })} />
          {missing && <span role="alert" className="wf-warning">{t("Delilah no encuentra la skill {name}; puedes guardar igualmente.", { name: `/${name}` })}</span>}
          <span className="ps-help">{t("Se envía al empezar la tarea. Usa {ticket}, {title} y {task}.", { ticket: "{ticket}", title: "{title}", task: "{task}" })}</span>
        </div>
      )}
      {own.map((f, i) => <span key={i} className="error field-error selectable" data-field={f.field}>{f.message}</span>)}
    </div>
  );
}
