import { useSyncExternalStore } from "react";
import { api, type Action, type Activity, type Session, type SpecSummary, type Task, type TokenUsage } from "../api";
import { on } from "../live";
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

/** The floors of the owners with a live Claude session, plus those whose helpers are still
 *  leaving (so they can walk out after the lead's session ended). `prev` (the floors of the
 *  last call, by owner) keeps desks in place and carries the leavers; `now` is epoch ms. */
export function buildFloors(input: OfficeInput, prev: Map<string, FloorModel>, now: number): FloorModel[] {
  const byOwner = new Map<string, Session[]>();
  for (const s of input.sessions) {
    if (s.kind !== "claude" || !s.live) continue;
    byOwner.set(s.task, [...(byOwner.get(s.task) ?? []), s]);
  }
  const owners = [...byOwner.keys(), ...[...prev.keys()].filter((o) => !byOwner.has(o))];
  const floors: FloorModel[] = [];
  for (const owner of owners) {
    const sessions = byOwner.get(owner) ?? [];
    const head = headOf(owner, input.tasks, input.specs);
    if (!head) continue;
    const before = prev.get(owner);
    const helpers = sessions.flatMap((s) => input.activity[s.id]?.helpers ?? []);
    const leaving = trackLeavers(
      { helpers: (before?.helpers ?? []).map((d) => ({ id: d.helperId!, slot: d.slot, role: d.lookKey })), leaving: before?.leaving ?? [] },
      helpers.map((h) => h.id),
      now,
    );
    if (sessions.length === 0 && leaving.length === 0) continue;
    const slots = assignSlots(
      new Map([...(before?.leads ?? []).map((d) => [d.session!, d.slot] as const), ...(before?.helpers ?? []).map((d) => [d.helperId!, d.slot] as const)]),
      sessions.map((s) => s.id),
      helpers.map((h) => h.id),
      leaving.map((l) => l.slot),
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
    floors.push({ owner, ...head, leads, helpers: seats, leaving, waiting: leads.filter((d) => d.state === "waiting").length });
  }
  return floors;
}

// The office's data lives in one module-level store, shared by every view (building, floor,
// flow) and kept while none is mounted, so switching tabs or drilling down shows the last
// floors at once and desks keep their place. The first subscriber fetches and listens to the
// change feed; the last one stops listening.
interface Data {
  sessions: Session[];
  tasks: Task[];
  specs: Record<string, SpecSummary>;
  activity: Record<string, Activity>;
  tokens: Record<string, TokenUsage>;
}
interface Snapshot {
  /** Every floor, including those only kept for their leavers. */
  all: FloorModel[];
  /** The floors with somebody working. */
  floors: FloorModel[];
  tasks: Task[];
  specs: Record<string, SpecSummary>;
}

const EMPTY: Data = { sessions: [], tasks: [], specs: {}, activity: {}, tokens: {} };
let data: Data = EMPTY;
let snapshot: Snapshot = { all: [], floors: [], tasks: [], specs: {} };
let prev = new Map<string, FloorModel>();
const listeners = new Set<() => void>();
let offs: (() => void)[] = [];
let dueTimer: ReturnType<typeof setTimeout> | null = null;

function rebuild() {
  const all = buildFloors(data, prev, Date.now());
  prev = new Map(all.map((f) => [f.owner, f]));
  snapshot = { all, floors: all.filter((f) => f.leads.length + f.helpers.length > 0), tasks: data.tasks, specs: data.specs };
  if (dueTimer) clearTimeout(dueTimer);
  dueTimer = null;
  // Rebuild once the oldest leaver is due.
  const since = Math.min(...all.flatMap((f) => f.leaving.map((l) => l.since)));
  if (Number.isFinite(since) && listeners.size > 0) dueTimer = setTimeout(rebuild, Math.max(0, since + LEAVER_MS - Date.now()) + 20);
  listeners.forEach((l) => l());
}

function update(patch: Partial<Data>) {
  data = { ...data, ...patch };
  rebuild();
}

function start() {
  const fetchers = {
    sessions: () => api.sessions().then((sessions) => update({ sessions })),
    tasks: () => api.tasks().then((tasks) => update({ tasks })),
    // An older daemon has no specs: phases fall back to the start.
    specs: () => api.specs().catch(() => ({})).then((specs) => update({ specs })),
    // An older daemon has no activity: every session reads as idle.
    activity: () =>
      api
        .activity()
        .catch(() => ({ sessions: {}, tokens: {} }))
        .then((a) => update({ activity: a.sessions, tokens: a.tokens })),
  };
  const topics = Object.keys(fetchers) as (keyof typeof fetchers)[];
  topics.forEach((t) => fetchers[t]().catch(() => {}));
  offs = topics.map((t) => on(t, () => fetchers[t]().catch(() => {})));
  rebuild();
}

function stop() {
  offs.forEach((off) => off());
  offs = [];
  if (dueTimer) clearTimeout(dueTimer);
  dueTimer = null;
}

function subscribe(listener: () => void): () => void {
  listeners.add(listener);
  if (listeners.size === 1) start();
  return () => {
    listeners.delete(listener);
    if (listeners.size === 0) stop();
  };
}

const getSnapshot = (): Snapshot => snapshot;

/** Forgets everything the store holds (tests). */
export function resetOffice() {
  stop();
  listeners.clear();
  data = EMPTY;
  snapshot = { all: [], floors: [], tasks: [], specs: {} };
  prev = new Map();
}

/** The office's data: live sessions grouped into floors, fresh from the change feed.
 *  `online` is the app's connection state; the caller that holds the feed passes it in
 *  (this never opens a connection of its own), and it reads as online without it. */
export function useOffice(online?: boolean): { floors: FloorModel[]; online: boolean; floorOf: (owner: string) => FloorModel } {
  const snap = useSyncExternalStore(subscribe, getSnapshot);
  const floorOf = (owner: string): FloorModel => snap.all.find((f) => f.owner === owner) ?? emptyFloor(owner, snap.tasks, snap.specs);
  return { floors: snap.floors, online: online ?? true, floorOf };
}

/** One owner's floor (an empty one when nobody is working for it). */
export function useFloor(owner: string, online?: boolean): { floor: FloorModel; online: boolean } {
  const { floorOf, online: on } = useOffice(online);
  return { floor: floorOf(owner), online: on };
}
