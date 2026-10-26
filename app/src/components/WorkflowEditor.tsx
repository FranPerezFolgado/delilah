import type { ProjectSettings, SettingsError, Start } from "../api";
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
