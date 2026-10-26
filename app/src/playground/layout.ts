import type { Action, Activity, Helper, Project, Session, SpecSummary, Task } from "../api";
import type { DeskState, Pose } from "./poses";
import type { Look } from "./sprites";
import { t, tn } from "../i18n";
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
  /** What the session is doing now; none when idle or unknown. */
  activity?: Activity;
}

/** What a desk's character is doing: the session's state, and while it works, its action. */
export function deskState(session: Session, activity?: Activity): DeskState {
  switch (session.state) {
    case "starting":
    case "waiting":
    case "paused":
      return session.state;
    default: {
      const action = activity?.action;
      if (!action) return "thinking";
      if (action.kind === "delegate") return action.background ? "thinking" : "supervising";
      return action.kind;
    }
  }
}

/** The action as a short line: `Edit · api.rs`, or just the tool without a target. */
export function caption(action: Action | null | undefined): string | null {
  if (!action) return null;
  // Concatenated, not a template literal: the i18n coverage scan pairs backticks naively.
  return action.target ? action.tool + " · " + action.target : action.tool;
}

const STATE_WORD: Record<DeskState, string> = {
  starting: "Arrancando",
  thinking: "Pensando",
  waiting: "Esperándote",
  paused: "En pausa",
  edit: "Editando",
  read: "Leyendo",
  run: "Ejecutando",
  delegate: "Supervisando",
  supervising: "Supervisando",
  other: "Trabajando",
};

const LOOK_OF: Record<string, Look> = {
  "code-researcher": "researcher",
  Explore: "researcher",
  "doc-searcher": "researcher",
  implementer: "implementer",
  "code-quality-reviewer": "reviewer",
  "spec-compliance-reviewer": "reviewer",
};

/** How a helper looks and is called, from its role; an empty role is just "Ayudante". */
export function roleLook(role: string): { look: Look; label: string } {
  return { look: LOOK_OF[role] ?? "generic", label: role || t("Ayudante") };
}

const HELPER_POSE: Record<Action["kind"], Pose> = { edit: "typing", read: "reading", run: "terminal", delegate: "phone", other: "busy" };

/** A helper's pose, from its action (thinking without one). */
export const helperPose = (h: Helper): Pose => (h.action ? HELPER_POSE[h.action.kind] : "thinking");

const HELPER_WORD: Record<Action["kind"], string> = {
  edit: "Editando",
  read: "Leyendo",
  run: "Ejecutando",
  delegate: "Supervisando",
  other: "Trabajando",
};

export const MAX_HELPERS = 4;

/** The helpers drawn around a desk (the oldest by `since`) and the rest, shown as "+n". */
export function visibleHelpers(helpers: Helper[]): { shown: Helper[]; rest: Helper[] } {
  const sorted = [...helpers].sort((a, b) => a.since.localeCompare(b.since));
  return { shown: sorted.slice(0, MAX_HELPERS), rest: sorted.slice(MAX_HELPERS) };
}

/** "code-researcher leyendo": a helper's role and what it is doing. */
export const helperName = (h: Helper): string =>
  `${roleLook(h.role).label} ${t(h.action ? HELPER_WORD[h.action.kind] : "Pensando").toLowerCase()}`;

/** The character's accessible name: the task (or project), what it is doing, the action and the helpers. */
export function accessibleName(desk: DeskModel, activity?: Activity): string {
  const state = deskState(desk.session, activity);
  const cap = state === "waiting" ? null : caption(activity?.action);
  const helpers = activity?.helpers ?? [];
  const extra = helpers.length
    ? `, ${tn(helpers.length, "{n} ayudante", "{n} ayudantes")}: ${[...helpers].sort((a, b) => a.since.localeCompare(b.since)).map(helperName).join(", ")}`
    : "";
  return `${desk.key}, ${t(STATE_WORD[state])}${cap ? `, ${cap}` : ""}${extra}`;
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
