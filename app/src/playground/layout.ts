import type { Project, Session, SpecSummary, Task } from "../api";
import type { DeskState } from "./poses";
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
  /** The desk's place in its project's grid; stable while the session lives. */
  slot: number;
}

/** What a desk's character is doing, from the session's state; `activity` comes later. */
export function deskState(session: Session, _activity?: unknown): DeskState {
  switch (session.state) {
    case "starting":
    case "waiting":
    case "paused":
      return session.state;
    default:
      return "thinking";
  }
}

/** Slot maps per project (session id -> slot) of `areas`, to pass back to `groupByProject`. */
export function slotMaps(areas: ProjectArea[]): Map<string, Map<string, number>> {
  return new Map(areas.map((a) => [a.project, new Map(a.desks.map((d) => [d.session.id, d.slot]))]));
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
  prevSlots: Map<string, Map<string, number>> = new Map(),
): ProjectArea[] {
  const byKey = new Map(tasks.map((t) => [t.key, t]));
  const buckets = new Map<string, DeskModel[]>();
  for (const session of sessions) {
    if (session.kind !== "claude" || !session.live) continue;
    let desk: DeskModel;
    if (session.task.startsWith("@")) {
      const name = session.task.slice(1);
      desk = { session, owner: session.task, key: name, title: name, phase: null, project: name, slot: 0 };
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
        slot: 0,
      };
    }
    const area = desk.project ?? NO_PROJECT;
    buckets.set(area, [...(buckets.get(area) ?? []), desk]);
  }
  const place = (project: string, desks: DeskModel[]): ProjectArea => {
    const slots = assignSlots(prevSlots.get(project) ?? new Map(), desks.map((d) => d.session.id));
    return { project, desks: desks.map((d) => ({ ...d, slot: slots.get(d.session.id)! })) };
  };
  const areas = projects.filter((p) => buckets.has(p.name)).map((p) => place(p.name, buckets.get(p.name)!));
  // A project that is not registered any more still shows its desks, before "—".
  const known = new Set(projects.map((p) => p.name));
  for (const [project, desks] of buckets) if (project !== NO_PROJECT && !known.has(project)) areas.push(place(project, desks));
  if (buckets.has(NO_PROJECT)) areas.push(place(NO_PROJECT, buckets.get(NO_PROJECT)!));
  return areas;
}
