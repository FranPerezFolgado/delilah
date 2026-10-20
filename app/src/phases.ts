import type { SpecPhase, SpecSummary, Task } from "./api";
import { t } from "./i18n";

/** The board's columns: SpecKit's phases, then finished tasks. */
export type Column = SpecPhase | "done";

export const COLUMNS: { id: Column; label: string }[] = [
  { id: "specify", label: "Especificación" },
  { id: "plan", label: "Plan" },
  { id: "implement", label: "Implementación" },
  { id: "review", label: "Revisión" },
  { id: "done", label: "Hecha" },
];

/** The board: the backlog (tasks saved, not started) first, then the phases. Kept
 *  apart from COLUMNS, which are also the phases a task can be moved through. */
export type BoardColumn = Column | "backlog";

export const BOARD_COLUMNS: { id: BoardColumn; label: string }[] = [{ id: "backlog", label: "Backlog" }, ...COLUMNS];

export const boardColumnOf = (task: Task, spec?: SpecSummary): BoardColumn =>
  task.status.type === "queued" ? "backlog" : columnOf(task, spec);

const PHASES: SpecPhase[] = ["specify", "plan", "implement", "review"];

export const phaseLabel = (c: Column) => t(COLUMNS.find((col) => col.id === c)?.label ?? c);

/** The phase the task screen header shows: the board's column and its 1-based step. */
export interface HeaderPhase {
  column: SpecPhase;
  step: number;
  done?: number;
  total?: number;
}

/** Null for backlog and finished tasks and when there is no spec information. */
export function headerPhaseOf(task: Task, spec?: SpecSummary): HeaderPhase | null {
  if (!spec || task.status.type === "queued" || task.status.type === "done") return null;
  const column = columnOf(task, spec) as SpecPhase;
  const step = COLUMNS.findIndex((c) => c.id === column) + 1;
  if (column === "implement" && spec.total > 0) return { column, step, done: spec.done, total: spec.total };
  return { column, step };
}

/** Where a task sits: done tasks in "Hecha"; the others by the phase recorded (the
 *  spec, the agent's actions or the user move it), else the spec's, else the start. */
export function columnOf(task: Task, spec?: SpecSummary): Column {
  if (task.status.type === "done") return "done";
  if (task.phase && (PHASES as string[]).includes(task.phase)) return task.phase as SpecPhase;
  if (spec?.dir) return spec.phase;
  return "specify";
}
