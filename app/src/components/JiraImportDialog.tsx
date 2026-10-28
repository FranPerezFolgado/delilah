import { useCallback, useEffect, useRef, useState } from "react";
import { api, type JiraImportPreview, type JiraImportResult, type JiraImportRow, type Project } from "../api";
import { t } from "../i18n";
import { Dialog, Skeleton } from "./ui";

/** Which flow the dialog runs. Only «create» (no parent task yet) is built; the other
 *  modes (convert, attach, add) arrive with their own screens. */
export type JiraImportMode = "create";

interface Props {
  /** The Jira ticket whose sub-tasks are imported. */
  issueKey: string;
  mode?: JiraImportMode;
  onClose: () => void;
  /** After a successful import, so the caller can refresh. */
  onDone?: (result: JiraImportResult) => void;
}

type Linked = Extract<JiraImportPreview, { linked: true }>;

/** Why a row can't be ticked: it already is a task, or conflicts with one. */
const canTick = (r: JiraImportRow) => r.action === "create" || r.action === "attach";
// (conflict and linked rows stay unticked and disabled)

/** Imports a Jira ticket's sub-tasks as Delilah tasks, creating the parent task first.
 *  Rows come from the preview with their default ticks; the project is required. */
export function JiraImportDialog({ issueKey, mode = "create", onClose, onDone }: Props) {
  const [preview, setPreview] = useState<JiraImportPreview | null>(null);
  const [projects, setProjects] = useState<Project[]>([]);
  const [project, setProject] = useState("");
  const [ticked, setTicked] = useState<Set<string>>(new Set());
  const [error, setError] = useState<string | null>(null);
  const [result, setResult] = useState<JiraImportResult | null>(null);
  const busy = useRef(false);

  const load = useCallback(() => {
    let live = true;
    setPreview(null);
    setError(null);
    Promise.all([api.jiraImportPreview(issueKey), api.projects().catch(() => [] as Project[])])
      .then(([p, ps]) => {
        if (!live) return;
        setPreview(p);
        setProjects(ps);
        if (p.linked) setTicked(new Set(p.subtasks.filter((r) => canTick(r) && r.selected).map((r) => r.key)));
      })
      .catch((e: Error) => live && setError(e.message));
    return () => { live = false; };
  }, [issueKey]);
  useEffect(() => load(), [load]);

  const toggle = (key: string) =>
    setTicked((s) => {
      const n = new Set(s);
      if (!n.delete(key)) n.add(key);
      return n;
    });

  const submit = async (p: Linked) => {
    if (busy.current) return;
    busy.current = true;
    setError(null);
    try {
      const rows = p.subtasks.filter((r) => ticked.has(r.key));
      const r = await api.jiraImport(issueKey, {
        project,
        add: rows.map((x) => x.key),
        skip: [],
        unskip: rows.filter((x) => x.skipped).map((x) => x.key),
      });
      setResult(r);
      onDone?.(r);
    } catch (e) {
      setError((e as Error).message);
    } finally {
      busy.current = false;
    }
  };

  const linked = preview?.linked ? preview : null;
  const refused = linked?.parent.action === "refused" ? linked.parent : null;
  const parentAction = linked?.parent.action;
  const needsProject = mode === "create" && (parentAction ?? "create") === "create" && !project;
  const unreachable = preview && !preview.linked && preview.reason === "unreachable";

  const footer = result ? (
    <button type="button" className="btn btn-p" onClick={onClose}>{t("Cerrar")}</button>
  ) : (
    <>
      <button type="button" className="btn" onClick={onClose}>{t("Cancelar")}</button>
      {unreachable || (!preview && error) ? (
        <button type="button" className="btn btn-p" onClick={load}>{t("Reintentar")}</button>
      ) : (
        <button type="button" className="btn btn-p" disabled={!linked || !!refused || needsProject} onClick={() => linked && submit(linked)}>
          {t("Crear")}
        </button>
      )}
    </>
  );

  return (
    <Dialog title={t("Crear tarea padre de {key}", { key: issueKey })} onClose={onClose} footer={footer}>
      {result ? (
        <div className="jid-result">
          <p>{t("Tarea padre {key} creada.", { key: result.parent_task })}</p>
          {result.created.length > 0 && <p>{t("Creadas: {keys}", { keys: result.created.join(", ") })}</p>}
          {result.attached.length > 0 && <p>{t("Unidas: {keys}", { keys: result.attached.join(", ") })}</p>}
          {result.left_out.length > 0 && (
            <>
              <p>{t("Se quedaron fuera")}</p>
              <ul>
                {result.left_out.map((l) => (
                  <li key={l.key}><span className="mono">{l.key}</span> {l.reason}</li>
                ))}
              </ul>
            </>
          )}
        </div>
      ) : !preview && !error ? (
        <Skeleton lines={4} />
      ) : !preview ? (
        <p role="alert">{error}</p>
      ) : !preview.linked ? (
        <p role="alert">
          {preview.reason === "unreachable"
            ? t("No se pudo hablar con Jira.")
            : preview.reason === "not-connected"
              ? t("Jira no está conectado.")
              : t("Jira no conoce este ticket.")}
        </p>
      ) : linked && (
        <>
          {refused && <p role="alert">{refused.message}</p>}
          {parentAction === "convert" && <p>{t("{key} pasa a ser la tarea padre.", { key: issueKey })}</p>}
          {parentAction === "existing" && <p>{t("Se añade a la tarea padre {key}.", { key: issueKey })}</p>}
          {parentAction === "create" && (
            <label className="jid-project">
              <span>{t("Proyecto")}</span>
              <select className="input" aria-label={t("Proyecto")} value={project} onChange={(e) => setProject(e.target.value)}>
                <option value="">{t("Elige un proyecto")}</option>
                {projects.map((p) => <option key={p.name} value={p.name}>{p.name}</option>)}
              </select>
            </label>
          )}
          <ul className="jid-rows">
            {linked.subtasks.map((r) => (
              <li key={r.key} className="jid-row">
                <label>
                  <input type="checkbox" checked={ticked.has(r.key)} disabled={!canTick(r)} onChange={() => toggle(r.key)} />
                  <span className="mono">{r.key}</span> <span>{r.title}</span>{" "}
                  <span className="muted">{r.status}</span>{" "}
                  <span className="muted">{r.assignee ?? t("Sin asignar")}</span>
                </label>
                {r.status_category === "done" && <span className="b b-s sm">{t("hecha en Jira")}</span>}
                {r.assignee && !r.mine && <span className="b b-s sm">{t("asignada a {name}", { name: r.assignee })}</span>}
                {r.action === "attach" ? (
                  <span className="b b-s sm">{t("ya es una tarea, se unirá")}</span>
                ) : (
                  r.task && <span className="b b-s sm">{t("ya es una tarea")}</span>
                )}
                {r.action === "conflict" && r.reason && <span className="muted sm">{r.reason}</span>}
              </li>
            ))}
          </ul>
          {error && <p role="alert">{error}</p>}
        </>
      )}
    </Dialog>
  );
}
