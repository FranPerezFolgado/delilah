import { useEffect, useState } from "react";
import { api, type JiraImportPreview, type JiraImportRow, type SubtaskSummary, type Task } from "../api";
import { t, tn } from "../i18n";
import { Button, StatusBadge } from "./ui";
import { JiraImportDialog } from "./JiraImportDialog";
import "./SubtaskList.css";

interface Props {
  /** The parent task. */
  task: Task;
  /** Opens a subtask. */
  onOpenTask?: (key: string) => void;
  /** «Nueva subtarea»: opens the new task dialog for this parent task. */
  onNewSubtask?: (parent: Task) => void;
  onError?: (e: unknown) => void;
}

const prLabel = (state: NonNullable<SubtaskSummary["pr"]>["state"]) =>
  state === "merged" ? t("PR integrado") : state === "closed" ? t("PR cerrado") : state === "draft" ? t("PR en borrador") : t("PR abierto");

/** "N de M subtareas integradas", or "0 subtareas" when none counts. */
export function progressLabel(task: Pick<Task, "subtask_progress">): string {
  const p = task.subtask_progress;
  return !p || p.counted === 0 ? t("0 subtareas") : t("{merged} de {counted} subtareas integradas", { merged: p.merged, counted: p.counted });
}

/** A parent task's subtasks: state, PR state and the ones dropped (archived unmerged). */
export function SubtaskList({ task, onOpenTask, onNewSubtask, onError }: Props) {
  const [subtasks, setSubtasks] = useState<SubtaskSummary[] | null>(null);
  // Reload when the parent task changes (its progress moves when a subtask merges or is added).
  const p = task.subtask_progress;
  const [preview, setPreview] = useState<JiraImportPreview | null>(null);
  const [adding, setAdding] = useState(false);
  const [check, setCheck] = useState(0);
  const stamp = p ? `${p.merged}/${p.counted}/${p.dropped}` : "";
  useEffect(() => {
    let stale = false;
    api
      .subtasks(task.key)
      .then((list) => !stale && setSubtasks(list))
      .catch((e) => !stale && onError?.(e));
    return () => {
      stale = true;
    };
  }, [task.key, stamp]);

  // The Jira preview never blocks the list; failures are silent (the list is the truth).
  useEffect(() => {
    let stale = false;
    api
      .jiraImportPreview(task.key)
      .then((p) => !stale && setPreview(p))
      .catch(() => !stale && setPreview(null));
    return () => {
      stale = true;
    };
  }, [task.key, stamp, check]);

  const rows: JiraImportRow[] = preview?.linked ? preview.subtasks : [];
  const fresh = rows.filter((r) => (r.action === "create" || r.action === "attach") && !r.skipped && r.selected).length;
  const skipped = rows.filter((r) => (r.action === "create" || r.action === "attach") && r.skipped);
  const undo = (key: string) =>
    api
      .jiraImport(task.key, { add: [], skip: [], unskip: [key] })
      .then(() => setCheck((n) => n + 1))
      .catch((e) => onError?.(e));

  return (
    <section className="subtasks" aria-label={t("Subtareas")}>
      <header className="subtasks-hd">
        <h2 className="sec">{t("Subtareas")}</h2>
        <span className="muted">{progressLabel(task)}</span>
        <span className="subtasks-grow" />
        <Button variant="secondary" size={28} onClick={() => onNewSubtask?.(task)} disabled={!onNewSubtask}>{t("Nueva subtarea")}</Button>
      </header>
      {fresh > 0 && (
        <div className="subtasks-banner" role="status">
          <span>{tn(fresh, "{n} subtarea nueva de Jira", "{n} subtareas nuevas de Jira")}</span>
          <Button variant="secondary" size={28} onClick={() => setAdding(true)}>{t("Añadir")}</Button>
        </div>
      )}
      {preview && !preview.linked && preview.reason === "not-connected" && <p className="muted subtasks-note">{t("Jira no está conectado.")}</p>}
      {subtasks && subtasks.length === 0 && <p className="muted">{t("Aún no hay subtareas.")}</p>}
      <ul className="subtasks-list">
        {(subtasks ?? []).map((s) => (
          <li key={s.key} className={`subtask${s.dropped ? " dropped" : ""}`}>
            <button type="button" className="th-link mono" onClick={() => onOpenTask?.(s.key)}>{s.key}</button>
            <span className="trunc subtasks-title">{s.title}</span>
            {s.dropped ? <span className="badge neutral">{t("descartada")}</span> : <StatusBadge status={s.status} />}
            {s.pr && (
              <a className="muted" href={s.pr.url} target="_blank" rel="noreferrer">{prLabel(s.pr.state)}</a>
            )}
          </li>
        ))}
      </ul>
      {skipped.length > 0 && (
        <div className="subtasks-skipped">
          <p className="muted">{t("Omitidas ({n})", { n: skipped.length })}</p>
          <ul className="subtasks-list">
            {skipped.map((r) => (
              <li key={r.key} className="subtask">
                <span className="mono">{r.key}</span>
                <span className="trunc subtasks-title">{r.title}</span>
                <Button variant="secondary" size={28} aria-label={t("Deshacer {key}", { key: r.key })} onClick={() => undo(r.key)}>{t("Deshacer")}</Button>
              </li>
            ))}
          </ul>
        </div>
      )}
      {adding && (
        <JiraImportDialog
          issueKey={task.key}
          mode="add"
          onClose={() => setAdding(false)}
          onDone={() => setCheck((n) => n + 1)}
        />
      )}
    </section>
  );
}
