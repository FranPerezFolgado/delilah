import { useEffect, useState } from "react";
import { api, type Project, type Task } from "../api";
import { t } from "../i18n";
import { Button } from "./ui";
import "./FirstRun.css";

interface Props {
  projects: Project[];
  tasks: Task[];
  onAddProject: () => void;
  onConnectJira: () => void;
  onNewTask: () => void;
}

/** What is remembered between runs, in the `first-run` preference. */
interface Saved {
  jiraSkipped?: boolean;
}

/** The first-run checklist: add a project, connect Jira (or skip it), create a task.
 *  Hides itself once the three steps are done. */
export function FirstRun({ projects, tasks, onAddProject, onConnectJira, onNewTask }: Props) {
  const [saved, setSaved] = useState<Saved | null>(null);
  const [jira, setJira] = useState(false);

  useEffect(() => {
    let live = true;
    api.preference<Saved>("first-run").then((v) => live && setSaved(v ?? {}), () => live && setSaved({}));
    api.jira().then((j) => live && setJira(!!j.settings), () => {});
    return () => {
      live = false;
    };
  }, []);

  if (!saved) return null;
  const skipped = !!saved.jiraSkipped;
  const steps = [
    { id: "project", label: t("Añadir un proyecto"), done: projects.length > 0, act: onAddProject },
    { id: "jira", label: t("Conectar Jira (opcional)"), done: jira || skipped, act: onConnectJira, skip: !jira && !skipped },
    { id: "task", label: t("Crear tu primera tarea"), done: tasks.length > 0, act: onNewTask },
  ];
  if (steps.every((s) => s.done)) return null;

  const skip = () => {
    const next = { ...saved, jiraSkipped: true };
    setSaved(next);
    api.setPreference("first-run", next).catch(() => {});
  };

  return (
    <section className="firstrun" aria-label={t("Primeros pasos")}>
      <h2>{t("Primeros pasos")}</h2>
      <ol>
        {steps.map((s) => (
          <li key={s.id} className={s.done ? "done" : undefined}>
            <span className="fr-mark" role="img" aria-label={s.done ? t("Hecho") : t("Pendiente")}>
              {s.done ? "✓" : ""}
            </span>
            <span className="fr-label">{s.id === "jira" && skipped && !jira ? t("Jira omitido") : s.label}</span>
            {!s.done && (
              <Button variant={s.id === "project" ? "primary" : "secondary"} size={28} onClick={s.act}>
                {s.label}
              </Button>
            )}
            {s.skip && (
              <Button variant="ghost" size={28} onClick={skip}>
                {t("Omitir")}
              </Button>
            )}
          </li>
        ))}
      </ol>
    </section>
  );
}
