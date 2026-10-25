import { useState, type KeyboardEvent } from "react";
import type { Autopilot, SpecPhase, PrSummary, Project, SpecSummary, Task } from "../api";
import { BOARD_COLUMNS, COLUMNS, boardColumnOf, headerPhaseOf } from "../phases";
import { t } from "../i18n";
import { relativeTime } from "../time";
import { statusOf, type StatusTone } from "../vocab";
import { Icon } from "./Icon";
import { FirstRun } from "./FirstRun";
import { Button, Count, PhaseBars, PrChip, Segmented, Skeleton, StatusBadge } from "./ui";
import "./BoardView.css";

/** A PR summary that may carry the PR's number (the chip needs it). */
type BoardPr = PrSummary & { number?: number };

interface Props {
  tasks: Task[];
  prs?: BoardPr[];
  specs: Record<string, SpecSummary>;
  projects?: Project[];
  /** From the statusbar: only tasks whose status has this tone. */
  statusFilter?: StatusTone | null;
  onClearStatusFilter?: () => void;
  loading?: boolean;
  /** Task key -> the pending ask's text, for the one-line preview on «Te espera» cards. */
  asks?: Record<string, string>;
  /** Task key -> when it finished (RFC 3339), shown on «Hecha» cards. */
  finished?: Record<string, string>;
  onOpen: (key: string) => void;
  onNewTask: () => void;
  onAddProject?: () => void;
  onConnectJira?: () => void;
}

const LIMIT = 12;

/** Every task on one board, in the column of its SpecKit phase. */
export function BoardView({
  tasks, prs = [], specs, projects = [], statusFilter, onClearStatusFilter, loading, asks = {}, finished = {},
  onOpen, onNewTask, onAddProject = () => {}, onConnectJira = () => {},
}: Props) {
  const [project, setProject] = useState("");
  const [expanded, setExpanded] = useState<Record<string, boolean>>({});
  const names = [...new Set([...projects.map((p) => p.name), ...tasks.map((x) => x.project).filter((p): p is string => !!p)])];
  const shown = tasks.filter(
    (x) => (!project || x.project === project) && (!statusFilter || statusOf(x.status).tone === statusFilter),
  );

  return (
    <>
      <div className="view-head board-head" data-tauri-drag-region>
        <h1 data-tauri-drag-region>{t("Tareas")}</h1>
        {names.length > 0 && (
          <Segmented
            mode="radio"
            label={t("Filtrar por proyecto")}
            value={project}
            onChange={setProject}
            options={[{ value: "", label: t("Todos") }, ...names.map((n) => ({ value: n, label: n }))]}
          />
        )}
        {statusFilter && (
          <button className="board-chip" onClick={onClearStatusFilter} title={t("Quitar el filtro de estado")}>
            {t("Estado: {status}", { status: statusLabelOfTone(tasks, statusFilter) })}
            <Icon name="close" />
          </button>
        )}
        <span className="grow" />
        <Button variant="primary" icon="plus" onClick={onNewTask}>
          {t("Nueva tarea")}
        </Button>
      </div>
      {loading ? (
        <div className="board-skel" aria-label={t("Cargando las tareas")}>
          <Skeleton lines={4} width="60%" />
        </div>
      ) : (
        <>
          {projects.length === 0 && (
            <FirstRun projects={projects} tasks={tasks} onAddProject={onAddProject} onConnectJira={onConnectJira} onNewTask={onNewTask} />
          )}
          {(projects.length > 0 || tasks.length > 0) && (
        <div className="board">
          {BOARD_COLUMNS.map((col) => {
            const cards = shown.filter((x) => boardColumnOf(x, specs[x.key]) === col.id);
            const phase = col.id !== "backlog" && col.id !== "done";
            const folded = phase && cards.length === 0;
            const more = !expanded[col.id] && cards.length > LIMIT ? cards.length - LIMIT : 0;
            return (
              <section className={folded ? "col folded" : "col"} key={col.id} aria-label={t(col.label)}>
                <div className="col-head">
                  <b>{t(col.label)}</b>
                  <Count n={cards.length} showZero={!folded} />
                </div>
                {!folded && (
                  <div className="col-body">
                    {cards.slice(0, more ? LIMIT : undefined).map((x) => (
                      <Card
                        key={x.key}
                        task={x}
                        spec={specs[x.key]}
                        pr={prs.find((p) => p.task === x.key)}
                        ask={asks[x.key]}
                        finishedAt={finished[x.key]}
                        onOpen={onOpen}
                      />
                    ))}
                    {more > 0 && (
                      <button className="col-more" onClick={() => setExpanded({ ...expanded, [col.id]: true })}>
                        {t("{n} más", { n: more })}
                      </button>
                    )}
                  </div>
                )}
              </section>
            );
          })}
        </div>
          )}
        </>
      )}
    </>
  );
}

const statusLabelOfTone = (tasks: Task[], tone: StatusTone) =>
  statusOf(tasks.find((x) => statusOf(x.status).tone === tone)?.status ?? "backlog").label;

function AutopilotIcon({ autopilot }: { autopilot?: Autopilot | null }) {
  if (!autopilot) return null;
  const label =
    autopilot.state === "paused" ? t("Autopiloto en pausa") : autopilot.state === "done" ? t("PR abierta por autopiloto") : t("Autopiloto");
  const title = autopilot.state === "paused" && autopilot.reason ? `${label}: ${autopilot.reason}` : label;
  return (
    <span className={`kcard-ap ${autopilot.state}`} title={title} role="img" aria-label={title}>
      <Icon name="autopilot" />
    </span>
  );
}

function Card({
  task, spec, pr, ask, finishedAt, onOpen,
}: { task: Task; spec?: SpecSummary; pr?: BoardPr; ask?: string; finishedAt?: string; onOpen: (key: string) => void }) {
  const col = boardColumnOf(task, spec);
  const bars = col !== "backlog" && col !== "done";
  const open = () => onOpen(task.key);
  const onKey = (e: KeyboardEvent) => {
    if (e.target !== e.currentTarget) return;
    if (e.key === "Enter" || e.key === " ") {
      e.preventDefault();
      open();
    }
  };
  return (
    <div className="kcard" role="link" tabIndex={0} aria-label={`${task.key} ${task.title}`} onClick={open} onKeyDown={onKey}>
      <span className="kcard-top">
        <span className="kcard-key">{task.key}</span>
        <span className="grow" />
        <AutopilotIcon autopilot={task.autopilot} />
        <StatusBadge status={task.status} />
      </span>
      <span className="kcard-title">{task.title}</span>
      {task.project && <span className="kcard-meta">{task.project}</span>}
      {bars && <PhaseBars mini labelInTooltip phase={headerPhaseOf(task, spec) ?? (bars && task.status.type !== "setup-failed" ? { column: col as SpecPhase, step: COLUMNS.findIndex((c) => c.id === col) + 1 } : null)} status={task.status} />}
      {pr && pr.number != null && <PrChip pr={{ number: pr.number, state: pr.state === "merged" ? "merged" : pr.draft ? "draft" : pr.ci === "failure" ? "failing" : "open" }} />}
      {task.status.type === "waiting-for-user" && (
        <span className="kcard-ask">
          {ask && <span className="preview">{ask}</span>}
          <Button
            variant="secondary"
            size={28}
            onClick={(e) => {
              e.stopPropagation();
              open();
            }}
          >
            {t("Responder")}
          </Button>
        </span>
      )}
      {col === "done" && finishedAt && <span className="kcard-meta">{relativeTime(finishedAt)}</span>}
    </div>
  );
}
