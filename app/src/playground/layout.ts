import type { Project, Session, SpecSummary, Task } from "../api";
import { columnOf, type Column } from "../phases";

export interface DeskModel {
  session: Session;
  /** The session's owner: a task key or `@<project>`. */
  owner: string;
  /** The task key, or the project's name for a project session. */
  key: string;
  title: string;
  phase: Column | null;
  project: string | null;
}

export interface ProjectArea {
  /** The project's name, or "—" for tasks without one. */
  project: string;
  desks: DeskModel[];
}

/** Slots for `ids`: an id already in `prev` keeps its slot; a new one takes the lowest free. */
export function assignSlots(prev: Map<string, number>, ids: string[]): Map<string, number> {
  const next = new Map<string, number>();
  const used = new Set<number>();
  for (const id of ids) {
    const slot = prev.get(id);
    if (slot !== undefined) {
      next.set(id, slot);
      used.add(slot);
    }
  }
  let free = 0;
  for (const id of ids) {
    if (next.has(id)) continue;
    while (used.has(free)) free++;
    next.set(id, free);
    used.add(free);
  }
  return next;
}

const NO_PROJECT = "—";

/** Live Claude sessions as desks, grouped by project in the order of `projects`;
 *  those of tasks without a project go last. */
export function groupByProject(
  sessions: Session[],
  tasks: Task[],
  projects: Project[],
  specs: Record<string, SpecSummary>,
): ProjectArea[] {
  const byKey = new Map(tasks.map((t) => [t.key, t]));
  const buckets = new Map<string, DeskModel[]>();
  for (const session of sessions) {
    if (session.kind !== "claude" || !session.live) continue;
    let desk: DeskModel;
    if (session.task.startsWith("@")) {
      const name = session.task.slice(1);
      desk = { session, owner: session.task, key: name, title: name, phase: null, project: name };
    } else {
      const task = byKey.get(session.task);
      if (!task) continue;
      desk = {
        session,
        owner: session.task,
        key: task.key,
        title: task.title,
        phase: columnOf(task, specs[task.key]),
        project: task.project,
      };
    }
    const area = desk.project ?? NO_PROJECT;
    buckets.set(area, [...(buckets.get(area) ?? []), desk]);
  }
  const areas = projects.filter((p) => buckets.has(p.name)).map((p) => ({ project: p.name, desks: buckets.get(p.name)! }));
  // A project that is not registered any more still shows its desks, before "—".
  const known = new Set(projects.map((p) => p.name));
  for (const [project, desks] of buckets) if (project !== NO_PROJECT && !known.has(project)) areas.push({ project, desks });
  if (buckets.has(NO_PROJECT)) areas.push({ project: NO_PROJECT, desks: buckets.get(NO_PROJECT)! });
  return areas;
}
