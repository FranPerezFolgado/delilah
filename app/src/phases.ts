import type { SpecPhase, SpecSummary, Task } from "./api";

/** The board's columns: SpecKit's phases, then finished tasks. */
export type Column = SpecPhase | "done";

export const COLUMNS: { id: Column; label: string }[] = [
  { id: "specify", label: "Especificación" },
  { id: "plan", label: "Plan" },
  { id: "implement", label: "Implementación" },
  { id: "review", label: "Revisión" },
  { id: "done", label: "Hecha" },
];

const PHASES: SpecPhase[] = ["specify", "plan", "implement", "review"];

export const phaseLabel = (c: Column) => COLUMNS.find((col) => col.id === c)?.label ?? c;

/** Where a task sits: done tasks in "Hecha"; the others by their spec's phase, or the
 *  one last recorded, or the start if nothing is known yet. */
export function columnOf(task: Task, spec?: SpecSummary): Column {
  if (task.status.type === "done") return "done";
  if (spec?.dir) return spec.phase;
  if (task.phase && (PHASES as string[]).includes(task.phase)) return task.phase as SpecPhase;
  return "specify";
}
