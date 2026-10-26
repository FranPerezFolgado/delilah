import { useRef, useState, type ReactNode } from "react";
import { api, type PrSnapshot, type SpecPhase, type Task } from "../api";
import { t } from "../i18n";
import { COLUMNS, phaseLabel, type HeaderPhase } from "../phases";
import { keyOf } from "../shortcuts";
import { relativeTime } from "../time";
import { COMPACT_MAIN, useContainerWidth } from "../useWidth";
import { AutopilotControls } from "./AutopilotControls";
import { PreviewControl } from "./PreviewControl";
import { IconButton, Menu, PhaseBars, PrChip, StatusBadge } from "./ui";
import "./TaskHeader.css";

export interface TaskHeaderProps {
  task: Task;
  /** The phase for the bars (`headerPhaseOf`); null hides them. */
  phase: HeaderPhase | null;
  /** The key of the task that owns the workspace (preview, rename, phase, autopilot). */
  owner: string;
  hasWorkspace: boolean;
  showTree: boolean;
  onToggleTree: () => void;
  /** The SessionViewSwitch for the open session; the caller leaves it out on the Tarea tab and plain terminals. */
  sessionSwitch?: ReactNode;
  /** A request of the autopilot controls is in flight. */
  autopilotBusy?: boolean;
  onAutopilot: (action: "on" | "off" | "resume") => void;
  onSavingMode: (on: boolean) => void;
  /** Linked PR and when the task was marked done, for the done variant. */
  pr?: { number: number; snapshot?: PrSnapshot | null } | null;
  doneAt?: string | null;
  /** After a rename or phase change (the caller refreshes the task). */
  onChanged: () => void;
  onError: (message: string) => void;
}

/** The task screen's header: key, editable title, status, phase, controls. Compact when the
 *  main area (not the window) is narrow. */
export function TaskHeader(p: TaskHeaderProps) {
  const { task, phase, owner } = p;
  const root = useRef<HTMLDivElement>(null);
  const compact = useContainerWidth(root) <= COMPACT_MAIN;
  const [editing, setEditing] = useState<string | null>(null);
  const done = task.status.type === "done";
  const fail = (e: unknown) => p.onError(e instanceof Error ? e.message : String(e));

  const save = () => {
    const title = (editing ?? "").trim();
    setEditing(null);
    if (!title || title === task.title) return;
    api.updateTask(owner, { title }).then(p.onChanged).catch(fail);
  };
  const restore = () => {
    if (task.previous_title) api.updateTask(owner, { title: task.previous_title }).then(p.onChanged).catch(fail);
  };
  const setPhase = (column: SpecPhase) => api.setPhase(owner, column).then(p.onChanged).catch(fail);

  const bars = <PhaseBars phase={phase} status={task.status} labelInTooltip={compact} />;
  const phases = COLUMNS.filter((c) => c.id !== "done");

  return (
    <div className={`th${compact ? " compact" : ""}`} ref={root} data-tauri-drag-region role="group" aria-label={t("Cabecera de la tarea")}>
      <span className="th-key mono">{task.key}</span>
      {editing === null ? (
        <h1 className="th-title" data-tauri-drag-region title={`${task.title}\n${t("Clic para renombrar")}`} onClick={() => setEditing(task.title)}>
          {task.title}
        </h1>
      ) : (
        <input
          className="th-edit"
          autoFocus
          aria-label={t("Título de la tarea")}
          value={editing}
          onChange={(e) => setEditing(e.target.value)}
          onBlur={() => setEditing(null)}
          onKeyDown={(e) => {
            if (e.key === "Enter") save();
            else if (e.key === "Escape") setEditing(null);
          }}
        />
      )}
      {task.previous_title && (
        <span className="th-before">
          {t("Antes: {title}", { title: task.previous_title })} ·{" "}
          <button type="button" className="th-link" onClick={restore}>{t("Restaurar")}</button>
        </span>
      )}
      <StatusBadge status={task.status} />
      {done ? (
        <>
          {p.pr && <PrChip pr={p.pr} />}
          {p.doneAt && <span className="th-when">{relativeTime(p.doneAt)}</span>}
        </>
      ) : phase ? (
        <Menu
          trigger={(m) => (
            <button type="button" className="th-phase" aria-label={t("Cambiar de fase")} {...m}>
              {bars}
            </button>
          )}
          items={phases.map((c) => ({
            label: `${c.id === phase.column ? "✓ " : ""}${phaseLabel(c.id)}`,
            onSelect: () => setPhase(c.id as SpecPhase),
          }))}
        />
      ) : (
        bars
      )}
      <span className="th-spacer" />
      {p.sessionSwitch}
      {!done && (
        <AutopilotControls
          autopilot={task.autopilot}
          savingMode={task.saving_mode}
          onAutopilot={p.onAutopilot}
          onSavingMode={p.onSavingMode}
          busy={p.autopilotBusy}
          iconOnly={compact}
        />
      )}
      <PreviewControl taskKey={owner} project={task.project} status={task.status} iconOnly={compact} />
      {p.hasWorkspace && (
        <IconButton
          label={t("Árbol de archivos")}
          title={t("Árbol de archivos ({key})", { key: keyOf("toggle-tree") })}
          icon="sidebar"
          pressed={p.showTree}
          onClick={p.onToggleTree}
        />
      )}
    </div>
  );
}
