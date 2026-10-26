import { useEffect, useMemo, useRef, useState } from "react";
import { api, type Action, type Activity, type Project, type Session, type SpecSummary, type Task, type TokenUsage } from "../api";
import { connect, on } from "../live";
import { columnOf, type Column } from "../phases";
import { LEAVER_MS, assignSlots, trackLeavers, type Leaver } from "./floorLayout";
import { floorState, type FloorState } from "./layout";

/** One occupied desk. Exactly one of `session` (a lead) and `helperId` is set. */
export interface DeskSeat {
  slot: number;
  session?: string;
  helperId?: string;
  state: FloorState;
  action: Action | null;
  tokens?: number;
  /** What the character looks like: the project for a lead, the role for a helper. */
  lookKey: string;
}

/** One owner's (task or `@project`) office floor. Plain data. */
export interface FloorModel {
  owner: string;
  title: string;
  phase: Column | null;
  project: string | null;
  leads: DeskSeat[];
  helpers: DeskSeat[];
  leaving: Leaver[];
  /** Lead agents waiting for the user. */
  waiting: number;
}

export interface OfficeInput {
  sessions: Session[];
  tasks: Task[];
  projects: Project[];
  specs: Record<string, SpecSummary>;
  activity: Record<string, Activity>;
  tokens: Record<string, TokenUsage>;
}

type Head = Pick<FloorModel, "title" | "phase" | "project">;

/** Title, phase and project of an owner, or null if its task is unknown. */
function headOf(owner: string, tasks: Task[], specs: Record<string, SpecSummary>): Head | null {
  if (owner.startsWith("@")) {
    const name = owner.slice(1);
    return { title: name, phase: null, project: name };
  }
  const task = tasks.find((t) => t.key === owner);
  return task ? { title: task.title, phase: columnOf(task, specs[task.key]), project: task.project } : null;
}

/** A floor with nobody working, for an owner without a live session. */
export function emptyFloor(owner: string, tasks: Task[], specs: Record<string, SpecSummary>): FloorModel {
  const head = headOf(owner, tasks, specs) ?? { title: owner, phase: null, project: null };
  return { owner, ...head, leads: [], helpers: [], leaving: [], waiting: 0 };
}

/** The floors of the owners with a live Claude session. `prev` (the floors of the last call,
 *  by owner) keeps desks in place and carries the leavers; `now` is epoch ms. */
export function buildFloors(input: OfficeInput, prev: Map<string, FloorModel>, now: number): FloorModel[] {
  const byOwner = new Map<string, Session[]>();
  for (const s of input.sessions) {
    if (s.kind !== "claude" || !s.live) continue;
    byOwner.set(s.task, [...(byOwner.get(s.task) ?? []), s]);
  }
  const floors: FloorModel[] = [];
  for (const [owner, sessions] of byOwner) {
    const head = headOf(owner, input.tasks, input.specs);
    if (!head) continue;
    const before = prev.get(owner);
    const helpers = sessions.flatMap((s) => input.activity[s.id]?.helpers ?? []);
    const slots = assignSlots(
      new Map([...(before?.leads ?? []).map((d) => [d.session!, d.slot] as const), ...(before?.helpers ?? []).map((d) => [d.helperId!, d.slot] as const)]),
      sessions.map((s) => s.id),
      helpers.map((h) => h.id),
    );
    const leads = sessions.map<DeskSeat>((s) => ({
      slot: slots.get(s.id)!,
      session: s.id,
      state: floorState(s),
      action: input.activity[s.id]?.action ?? null,
      tokens: input.tokens[s.id]?.total,
      lookKey: head.project ?? "",
    }));
    const seats = helpers.map<DeskSeat>((h) => ({
      slot: slots.get(h.id)!,
      helperId: h.id,
      state: "working",
      action: h.action,
      lookKey: h.role,
    }));
    const leaving = trackLeavers(
      { helpers: (before?.helpers ?? []).map((d) => ({ id: d.helperId!, slot: d.slot })), leaving: before?.leaving ?? [] },
      helpers.map((h) => h.id),
      now,
    );
    floors.push({ owner, ...head, leads, helpers: seats, leaving, waiting: leads.filter((d) => d.state === "waiting").length });
  }
  return floors;
}

/** The office's data: live sessions grouped into floors, fresh from the change feed.
 *  `online` given (the main window, which already holds the feed) is used as is; without
 *  it, this opens its own connection (connect() is not idempotent). */
export function useOffice(online?: boolean): { floors: FloorModel[]; online: boolean; floorOf: (owner: string) => FloorModel } {
  const [sessions, setSessions] = useState<Session[]>([]);
  const [tasks, setTasks] = useState<Task[]>([]);
  const [projects, setProjects] = useState<Project[]>([]);
  const [specs, setSpecs] = useState<Record<string, SpecSummary>>({});
  const [activity, setActivity] = useState<Record<string, Activity>>({});
  const [tokens, setTokens] = useState<Record<string, TokenUsage>>({});
  const [own, setOwn] = useState(true);
  const [tick, setTick] = useState(0);

  useEffect(() => {
    const fetchers = {
      sessions: () => api.sessions().then(setSessions),
      tasks: () => api.tasks().then(setTasks),
      projects: () => api.projects().then(setProjects),
      // An older daemon has no specs: phases fall back to the start.
      specs: () => api.specs().catch(() => ({})).then(setSpecs),
      // An older daemon has no activity: every session reads as idle.
      activity: () =>
        api
          .activity()
          .catch(() => ({ sessions: {}, tokens: {} }))
          .then((a) => {
            setActivity(a.sessions);
            setTokens(a.tokens);
          }),
    };
    const topics = Object.keys(fetchers) as (keyof typeof fetchers)[];
    topics.forEach((t) => fetchers[t]().catch(() => {}));
    const offs = topics.map((t) => on(t, () => fetchers[t]().catch(() => {})));
    return () => offs.forEach((off) => off());
  }, []);

  const external = online !== undefined;
  useEffect(() => {
    if (external) return;
    return connect(setOwn);
  }, [external]);

  // The last floors live here, not in the view, so desks survive it unmounting.
  const prev = useRef<Map<string, FloorModel>>(new Map());
  // Mutating the ref here is safe: rebuilding from the same inputs gives the same desks.
  const floors = useMemo(() => {
    const next = buildFloors({ sessions, tasks, projects, specs, activity, tokens }, prev.current, Date.now());
    prev.current = new Map(next.map((f) => [f.owner, f]));
    return next;
    // `tick` rebuilds once the oldest leaver is due.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [sessions, tasks, projects, specs, activity, tokens, tick]);

  useEffect(() => {
    const since = Math.min(...floors.flatMap((f) => f.leaving.map((l) => l.since)));
    if (!Number.isFinite(since)) return;
    const id = setTimeout(() => setTick((n) => n + 1), Math.max(0, since + LEAVER_MS - Date.now()) + 20);
    return () => clearTimeout(id);
  }, [floors]);

  const floorOf = (owner: string): FloorModel => floors.find((f) => f.owner === owner) ?? emptyFloor(owner, tasks, specs);
  return { floors, online: online ?? own, floorOf };
}

/** One owner's floor (an empty one when nobody is working for it). */
export function useFloor(owner: string, online?: boolean): { floor: FloorModel; online: boolean } {
  const { floorOf, online: on } = useOffice(online);
  return { floor: floorOf(owner), online: on };
}
