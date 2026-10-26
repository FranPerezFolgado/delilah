// The office floor's geometry and desk bookkeeping. Pure; units are art pixels (1x).

export interface Point {
  x: number;
  y: number;
}

/** A helper that ended: it walks from its desk to the lounge until `since + LEAVER_MS`. */
export interface Leaver {
  id: string;
  slot: number;
  /** The helper's role, so it keeps its outfit while it walks. */
  role?: string;
  /** Epoch ms when the helper ended. */
  since: number;
}

/** A leaver's whole stay on the floor: ~3 s walking to the lounge, ~5 s there, fading in the last 0.5 s. */
export const LEAVER_MS = 8500;
/** How long a leaver walks from its desk to its lounge seat. */
export const LEAVER_WALK_MS = 3000;

// The north wall: a band across the top of the floor; the pods start below it.
export const WALL_H = 20;
/** The sitter's head rises this far above the desk's top edge (the monitor hides the body). */
const HEAD_ROOM = 5;

// Desks and pods: 2x2 desks, one pod per four slots.
export const DESK_W = 14;
export const DESK_H = 9;
export const DESK_GAP_X = 12;
// Tall enough for the next row's heads (5) and the caption under this row's desks.
const DESK_GAP_Y = 12;
export const POD_W = 2 * DESK_W + DESK_GAP_X;
export const POD_H = 2 * DESK_H + DESK_GAP_Y;
/** Aisles between pods. */
export const AISLE_X = 20;
export const AISLE_Y = 18;
/** Pods per row; further pods start a new row. */
export const POD_COLS = 3;
export const MARGIN = 10;
/** Room under the last pod row for the captions and the aisle. */
const BOTTOM_PAD = 12;
/** A walker beside its desk stands this far from the seat (in the gap between two desks). */
const SIDE_STEP = 13;

// The lounge, right of the pods: pantry, vending machine and cooler along the wall, and a rug
// with the sofa, side table and plant. Rects are relative to the lounge's left edge.
export const LOUNGE_W = 40;
const WALL_FEET = WALL_H + 3;
export const VENDING = { x: 0, y: WALL_FEET - 13, w: 7, h: 13 };
export const PANTRY = { x: 9, y: WALL_FEET - 8, w: 20, h: 8 };
export const WATER_COOLER = { x: 31, y: WALL_FEET - 9, w: 4, h: 9 };
export const RUG = { x: 1, y: WALL_H + 16, w: 38, h: 26 };
export const SOFA = { x: 4, y: RUG.y + 4, w: 20, h: 7 };
export const SIDE_TABLE = { x: 26, y: RUG.y + 7, w: 7, h: 4 };
export const PLANT = { x: 34, y: RUG.y + 4, w: 6, h: 7 };
const LOUNGE_BOTTOM = RUG.y + RUG.h + 6;

// Decor on the north wall (art rects; the sprites stand on the skirting line).
export const WHITEBOARD = { x: MARGIN + 13, y: WALL_FEET - 11, w: 14, h: 11 };
export const BOOKSHELF = { x: MARGIN + POD_W + AISLE_X + 8, y: WALL_FEET - 12, w: 8, h: 12 };
export const WALL_CLOCK = { x: MARGIN + POD_W + AISLE_X + 24, y: 4, w: 7, h: 7 };
export const WINDOWS = [
  { x: MARGIN + 34, y: 5, w: 16, h: 10 },
  { x: MARGIN + POD_W + AISLE_X + 36, y: 5, w: 16, h: 10 },
];

/** Where the pods start (top-left of pod 0): below the wall, with room for the head of the first row. */
const ORIGIN: Point = { x: MARGIN, y: WALL_H + HEAD_ROOM + 5 };

/** Pod columns the floor shows for `pods` pods: at least two, at most POD_COLS. */
export const floorCols = (pods: number): number => Math.min(POD_COLS, Math.max(2, pods));
/** The lounge's left edge for `pods` pods. */
export const loungeLeft = (pods: number): number => MARGIN + floorCols(pods) * POD_W + floorCols(pods) * AISLE_X;

/** Spots leavers stand in the lounge (on the rug, in front of the sofa and by the table). */
export function loungeSeats(pods: number): Point[] {
  const l = loungeLeft(pods);
  const y = RUG.y + 20;
  return [
    { x: l + 10, y },
    { x: l + 18, y },
    { x: l + 28, y },
    { x: l + 22, y: y + 4 },
  ];
}

/** Pods needed for `desks` desks; at least one so an empty office still has its floor. */
export const podCount = (desks: number): number => Math.max(1, Math.ceil(desks / 4));

/** The floor's size for `pods` pods: sized to its content (wall, pod rows, aisle) and the lounge. */
export function floorSize(pods: number): { width: number; height: number } {
  const n = Math.max(1, pods);
  const rows = Math.ceil(n / POD_COLS);
  const podsBottom = ORIGIN.y + rows * POD_H + (rows - 1) * AISLE_Y + BOTTOM_PAD;
  return {
    width: loungeLeft(n) + LOUNGE_W + MARGIN,
    height: Math.max(podsBottom, LOUNGE_BOTTOM),
  };
}

/** Top-left of slot `slot`'s desk: slot n is desk n%4 of pod floor(n/4). */
export function deskPosition(slot: number): Point {
  const pod = Math.floor(slot / 4);
  const desk = slot % 4;
  const podX = ORIGIN.x + (pod % POD_COLS) * (POD_W + AISLE_X);
  const podY = ORIGIN.y + Math.floor(pod / POD_COLS) * (POD_H + AISLE_Y);
  return {
    x: podX + (desk % 2) * (DESK_W + DESK_GAP_X),
    y: podY + Math.floor(desk / 2) * (DESK_H + DESK_GAP_Y),
  };
}

/** The character's feet anchor (its sprite's top-left is `x - 4, y - 12`): it sits BEHIND the
 *  desk, centred, so its head shows above the monitor and the desk covers the rest. */
export function seatPosition(slot: number): Point {
  const d = deskPosition(slot);
  return { x: d.x + DESK_W / 2, y: d.y - HEAD_ROOM + 12 };
}

/** Where a walker stands next to the desk, in the gap between the two desks of a pod row. */
export function sidePosition(slot: number): Point {
  const s = seatPosition(slot);
  return { x: s.x + (slot % 2 === 0 ? SIDE_STEP : -SIDE_STEP), y: s.y };
}

/** A leaver's way: desk seat, beside the desk, down to the aisle south of its pod, along it to the
 *  lounge's door, then a lounge seat. */
export function loungePath(slot: number, pods: number): Point[] {
  const seat = seatPosition(slot);
  const side = sidePosition(slot);
  const pod = Math.floor(slot / 4);
  const podBottom = ORIGIN.y + Math.floor(pod / POD_COLS) * (POD_H + AISLE_Y) + POD_H;
  const aisleY = podBottom + AISLE_Y / 2;
  const seats = loungeSeats(pods);
  return [seat, side, { x: side.x, y: aisleY }, { x: loungeLeft(pods) - AISLE_X / 2, y: aisleY }, seats[slot % seats.length]];
}

/** Where along a path (0..1, by length) each point is reached, so a walk at constant speed
 *  can be given as keyframe offsets. */
export function pathOffsets(path: Point[]): number[] {
  const lens = path.slice(1).map((p, i) => Math.hypot(p.x - path[i].x, p.y - path[i].y));
  const total = lens.reduce((a, b) => a + b, 0) || 1;
  let run = 0;
  return [0, ...lens.map((l) => (run += l) / total)];
}

/** Slots for the ids: one already in `prev` keeps its slot; new leads take the lowest free
 *  slots first, then new helpers. A freed slot is reused, except the `reserved` ones (a leaver
 *  is still walking away from them). */
export function assignSlots(prev: Map<string, number>, leads: string[], helpers: string[], reserved: number[] = []): Map<string, number> {
  const ids = [...leads, ...helpers];
  const next = new Map<string, number>();
  const held = new Set(reserved);
  const used = new Set<number>();
  for (const id of ids) {
    const slot = prev.get(id);
    if (slot !== undefined && !used.has(slot) && !held.has(slot)) {
      next.set(id, slot);
      used.add(slot);
    }
  }
  let free = 0;
  for (const id of ids) {
    if (next.has(id)) continue;
    while (used.has(free) || held.has(free)) free++;
    next.set(id, free);
    used.add(free);
  }
  return next;
}

/** The leavers at time `at` (ms): the unexpired ones of before, plus the helpers of `prev`
 *  that are not in `nowHelpers` any more. */
export function trackLeavers(
  prev: { helpers: { id: string; slot: number; role?: string }[]; leaving: Leaver[] },
  nowHelpers: string[],
  at: number,
): Leaver[] {
  const now = new Set(nowHelpers);
  const kept = prev.leaving.filter((l) => !now.has(l.id) && at < l.since + LEAVER_MS);
  const known = new Set(kept.map((l) => l.id));
  const gone = prev.helpers.filter((h) => !now.has(h.id) && !known.has(h.id)).map((h) => ({ id: h.id, slot: h.slot, role: h.role, since: at }));
  return [...kept, ...gone];
}

/** The building's floors, newest on top: a new owner goes first (the last new one topmost),
 *  the ones already in `prev` keep their relative order, and the gone ones are removed. */
export function floorOrder(prev: string[], owners: string[]): string[] {
  const now = new Set(owners);
  const kept = prev.filter((o) => now.has(o));
  const known = new Set(kept);
  const fresh = owners.filter((o) => !known.has(o)).reverse();
  return [...fresh, ...kept];
}
