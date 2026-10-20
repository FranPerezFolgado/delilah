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

/** Where a task sits: done tasks in "Hecha"; the others by the phase recorded (the
 *  spec, the agent's actions or the user move it), else the spec's, else the start. */
export function columnOf(task: Task, spec?: SpecSummary): Column {
  if (task.status.type === "done") return "done";
  if (task.phase && (PHASES as string[]).includes(task.phase)) return task.phase as SpecPhase;
  if (spec?.dir) return spec.phase;
  return "specify";
}
