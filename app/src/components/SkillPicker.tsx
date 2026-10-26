import { useEffect, useState } from "react";
import { api, type Skill } from "../api";
import { t } from "../i18n";
import "./SkillPicker.css";

/** Loads a project's skills once. `null` while loading or when the request failed (free text still works). */
export function useSkills(project: string): Skill[] | null {
  const [skills, setSkills] = useState<Skill[] | null>(null);
  useEffect(() => {
    let live = true;
    setSkills(null);
    api.projectSkills(project).then(
      (list) => live && setSkills(list),
      () => live && setSkills(null),
    );
    return () => {
      live = false;
    };
  }, [project]);
  return skills;
}

const sourceLabel = (s: Skill): string =>
  s.source === "project" ? t("proyecto") : s.source === "user" ? t("usuario") : t("plugin");

/**
 * The command of a start: an editable text field (anything is accepted) with a searchable
 * list of the skills and commands Claude Code can run in the project. Picking one fills the
 * field with `/<name> {ticket}`.
 */
export function SkillPicker({ skills, value, onChange, label }: {
  /** The project's skills (see `useSkills`); null while loading or when the list failed. */
  skills: Skill[] | null;
  value: string;
  onChange: (command: string) => void;
  /** Accessible name of the field. */
  label?: string;
}) {
  const [query, setQuery] = useState("");
  const all = skills ?? [];

  const q = query.trim().toLowerCase();
  const shown = all.filter((s) => !q || `${s.name} ${s.description}`.toLowerCase().includes(q));

  return (
    <div className="skill-picker">
      <input
        className="input mono"
        aria-label={label ?? t("Comando")}
        value={value}
        placeholder="/fix {ticket}"
        onChange={(e) => onChange(e.target.value)}
      />
      <input
        type="search"
        className="input"
        aria-label={t("Buscar skill")}
        placeholder={t("Buscar skill")}
        value={query}
        onChange={(e) => setQuery(e.target.value)}
      />
      {shown.length === 0 ? (
        <p className="muted small">{t("No hay skills")}</p>
      ) : (
        <ul className="skill-list">
          {shown.map((s) => (
            <li key={`${s.source}:${s.plugin ?? ""}:${s.name}`}>
              <button type="button" className="skill" onClick={() => onChange(`/${s.name} {ticket}`)}>
                <span className="mono skill-name">{`/${s.name}`}</span>
                <span className="badge skill-source">{sourceLabel(s)}</span>
                {s.description && <span className="muted small skill-desc">{s.description}</span>}
              </button>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
