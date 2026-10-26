// The office floor's geometry and desk bookkeeping. Pure; units are art pixels (1x).

export interface Point {
  x: number;
  y: number;
}

/** A helper that ended: it walks from its (already free) slot to the lounge until `since + LEAVER_MS`. */
export interface Leaver {
  id: string;
  slot: number;
  /** Epoch ms when the helper ended. */
  since: number;
}

export const LEAVER_MS = 5000;

// Desks and pods: 2x2 desks, one pod per four slots.
export const DESK_W = 14;
export const DESK_H = 9;
export const DESK_GAP_X = 12;
export const DESK_GAP_Y = 6;
export const POD_W = 2 * DESK_W + DESK_GAP_X;
export const POD_H = 2 * DESK_H + DESK_GAP_Y;
/** Aisles between pods. */
export const AISLE_X = 20;
export const AISLE_Y = 18;
/** Pods per row; further pods start a new row. */
export const POD_COLS = 3;
export const MARGIN = 10;
/** The chair sits this far below the desk's bottom edge. */
const SEAT_DROP = 3;

// The lounge corner, right of the pods.
export const LOUNGE_W = 40;
export const LOUNGE_H = 66;
export const SOFA = { x: 0, y: 14, w: 20, h: 7 };
export const SIDE_TABLE = { x: 24, y: 16, w: 7, h: 4 };
export const PANTRY = { x: 10, y: 0, w: 20, h: 8 };
export const WATER_COOLER = { x: 34, y: 0, w: 4, h: 9 };
export const PLANTS = [
  { x: 0, y: 0, w: 6, h: 10 },
  { x: 34, y: 50, w: 6, h: 7 },
];

/** Where the pods start (top-left of pod 0). */
const ORIGIN: Point = { x: MARGIN, y: MARGIN };
const loungeLeft = (): number => MARGIN + POD_COLS * POD_W + (POD_COLS - 1) * AISLE_X + AISLE_X;

/** Spots leavers stand in the lounge (in front of the sofa, by the table and the pantry). */
export const LOUNGE_SEATS: Point[] = [
  { x: loungeLeft() + 6, y: MARGIN + 28 },
  { x: loungeLeft() + 14, y: MARGIN + 28 },
  { x: loungeLeft() + 24, y: MARGIN + 28 },
  { x: loungeLeft() + 20, y: MARGIN + 38 },
];

/** Pods needed for `desks` desks; at least one so an empty office still has its floor. */
export const podCount = (desks: number): number => Math.max(1, Math.ceil(desks / 4));

/** The floor's size for `pods` pods: grows down by rows of POD_COLS, and always holds the lounge. */
export function floorSize(pods: number): { width: number; height: number } {
  const n = Math.max(1, pods);
  const rows = Math.ceil(n / POD_COLS);
  const podsH = rows * POD_H + (rows - 1) * AISLE_Y + AISLE_Y;
  return {
    width: MARGIN * 2 + POD_COLS * POD_W + (POD_COLS - 1) * AISLE_X + AISLE_X + LOUNGE_W,
    height: MARGIN * 2 + Math.max(podsH, LOUNGE_H),
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

/** Where the character sits: centred below the desk. */
export function seatPosition(slot: number): Point {
  const d = deskPosition(slot);
  return { x: d.x + DESK_W / 2, y: d.y + DESK_H + SEAT_DROP };
}

/** A leaver's way: desk seat, down to the aisle south of its pod, then a lounge seat. */
export function loungePath(slot: number): [Point, Point, Point] {
  const seat = seatPosition(slot);
  const pod = Math.floor(slot / 4);
  const podBottom = ORIGIN.y + Math.floor(pod / POD_COLS) * (POD_H + AISLE_Y) + POD_H;
  return [seat, { x: seat.x, y: podBottom + AISLE_Y / 2 }, LOUNGE_SEATS[slot % LOUNGE_SEATS.length]];
}

/** Slots for the ids: one already in `prev` keeps its slot; new leads take the lowest free
 *  slots first, then new helpers. A freed slot is reused. */
export function assignSlots(prev: Map<string, number>, leads: string[], helpers: string[]): Map<string, number> {
  const ids = [...leads, ...helpers];
  const next = new Map<string, number>();
  const used = new Set<number>();
  for (const id of ids) {
    const slot = prev.get(id);
    if (slot !== undefined && !used.has(slot)) {
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

/** The leavers at time `at` (ms): the unexpired ones of before, plus the helpers of `prev`
 *  that are not in `nowHelpers` any more. */
export function trackLeavers(
  prev: { helpers: { id: string; slot: number }[]; leaving: Leaver[] },
  nowHelpers: string[],
  at: number,
): Leaver[] {
  const now = new Set(nowHelpers);
  const kept = prev.leaving.filter((l) => !now.has(l.id) && at < l.since + LEAVER_MS);
  const known = new Set(kept.map((l) => l.id));
  const gone = prev.helpers.filter((h) => !now.has(h.id) && !known.has(h.id)).map((h) => ({ id: h.id, slot: h.slot, since: at }));
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
